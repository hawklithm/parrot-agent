use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use services::{
    BuiltInAgentService, BuiltInAgentStatus, BuiltInAgentDefinition,
    BuiltInAgentError, BuiltInAgentKey, ReconcileResult,
};
use crate::extractors::CompanyIdOrShortname;
use crate::validation::agent_schemas::ProvisionBuiltInAgentSchema;
use garde::Validate;
use std::sync::Arc;

/// AppState 别名，统一状态类型
pub use crate::app_state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltInAgentStateResponse {
    pub definition: BuiltInAgentDefinition,
    pub status: BuiltInAgentStatus,
    pub agent: Option<models::Agent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionBuiltInAgentRequest {
    pub adapter_type: Option<String>,
    pub adapter_config: Option<serde_json::Value>,
    pub budget_monthly_cents: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileBuiltInAgentRequest {}

/// 将 `BuiltInAgentError` 映射为 HTTP 状态码与消息
fn map_built_in_error(e: BuiltInAgentError) -> (StatusCode, String) {
    match e {
        BuiltInAgentError::NotFound(_) => {
            (StatusCode::NOT_FOUND, "Built-in agent not found".to_string())
        }
        BuiltInAgentError::RepositoryError(msg)
        | BuiltInAgentError::InvalidConfiguration(msg)
        | BuiltInAgentError::ProvisionFailed(msg) => (StatusCode::BAD_REQUEST, msg),
        BuiltInAgentError::FeatureNotEnabled(msg) => (StatusCode::FORBIDDEN, msg),
        BuiltInAgentError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
    }
}

/// GET /companies/:companyId/built-in-agents
/// 列出公司所有内置 Agent 的状态
pub async fn list_built_in_agents(
    State(state): State<AppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
) -> Result<Json<Vec<BuiltInAgentStateResponse>>, (StatusCode, String)> {
    let service = state.built_in_agent_service.clone();
    let definitions = service.list_definitions();

    let mut responses = Vec::with_capacity(definitions.len());
    for def in definitions {
        let status = service
            .get_status(company_id, def.key)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get built-in agent status: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get status: {}", e))
            })?;

        responses.push(BuiltInAgentStateResponse {
            definition: def.clone(),
            status,
            agent: None,
        });
    }

    Ok(Json(responses))
}

/// POST /companies/:companyId/built-in-agents/:key/provision
/// 配置指定的内置 Agent
pub async fn provision_built_in_agent(
    State(state): State<AppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
    Json(payload): Json<ProvisionBuiltInAgentRequest>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    let service = state.built_in_agent_service.clone();
    // 验证请求
    let schema = ProvisionBuiltInAgentSchema {
        adapter_type: payload.adapter_type.clone(),
        adapter_config: payload.adapter_config.clone(),
        budget_monthly_cents: payload.budget_monthly_cents,
    };
    schema.validate(&()).map_err(|e| {
        tracing::warn!("Provision validation failed: {:?}", e);
        (StatusCode::BAD_REQUEST, format!("Validation error: {}", e))
    })?;

    // 解析内置 Agent 键
    let key = BuiltInAgentKey::from_str(&key)
        .ok_or((StatusCode::NOT_FOUND, "Unknown built-in agent key".to_string()))?;

    // 调用服务层
    let agent = service
        .provision(company_id, key)
        .await
        .map_err(map_built_in_error)?;

    let definition = service
        .get_definition(key)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Built-in agent definition missing".to_string()))?;

    let response = BuiltInAgentStateResponse {
        definition,
        status: BuiltInAgentStatus::Ready,
        agent: Some(agent),
    };

    Ok(Json(response))
}

/// POST /companies/:companyId/built-in-agents/:key/reconcile
/// 协调指定内置 Agent 的状态（重新应用默认配置）
pub async fn reconcile_built_in_agent(
    State(state): State<AppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
    Json(_payload): Json<ReconcileBuiltInAgentRequest>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    let service = state.built_in_agent_service.clone();
    let key = BuiltInAgentKey::from_str(&key)
        .ok_or((StatusCode::NOT_FOUND, "Unknown built-in agent key".to_string()))?;

    let result = service
        .reconcile(company_id, key)
        .await
        .map_err(map_built_in_error)?;

    let definition = service
        .get_definition(key)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Built-in agent definition missing".to_string()))?;

    let response = BuiltInAgentStateResponse {
        definition,
        status: BuiltInAgentStatus::Ready,
        agent: None,
    };

    // result.changes 可用于后续审计/日志
    if !result.changes.is_empty() {
        tracing::info!("Built-in agent reconcile changes: {:?}", result.changes);
    }

    Ok(Json(response))
}

/// Create built-in agent routes.
///
/// 使用统一的 `AppState` 作为状态类型，返回 `Router<AppState>`，
/// 由 `create_router` 统一在最后调用 `.with_state(state)` 绑定。
pub fn built_in_agent_routes() -> Router<AppState> {
    use axum::routing::{get, post};
    Router::new()
        .route(
            "/companies/:company_id/built-in-agents",
            get(list_built_in_agents),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/provision",
            post(provision_built_in_agent),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/reconcile",
            post(reconcile_built_in_agent),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    // 测试需要 mock BuiltInAgentService
    // 这里只提供框架，实际测试需要实现 mock
}
