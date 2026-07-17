use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use services::{
    BuiltInAgentStatus, BuiltInAgentDefinition,
    BuiltInAgentError, BuiltInAgentKey, ProvisionInput,
};
use crate::extractors::CompanyIdOrShortname;
use crate::validation::agent_schemas::ProvisionBuiltInAgentSchema;
use garde::Validate;

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
    let provision_input = ProvisionInput {
        adapter_type: payload.adapter_type,
        adapter_config: payload.adapter_config,
        budget_monthly_cents: payload.budget_monthly_cents,
    };
    let agent = service
        .provision(company_id, key, Some(&provision_input))
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

/// GET /companies/:companyId/built-in-agents/:key/status
/// 获取指定内置 Agent 的状态
pub async fn get_built_in_agent_status(
    State(state): State<AppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    let service = state.built_in_agent_service.clone();
    let key = BuiltInAgentKey::from_str(&key)
        .ok_or((StatusCode::NOT_FOUND, "Unknown built-in agent key".to_string()))?;

    let status = service
        .get_status(company_id, key)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get built-in agent status: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get status: {}", e))
        })?;

    let definition = service
        .get_definition(key)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Built-in agent definition missing".to_string()))?;

    Ok(Json(BuiltInAgentStateResponse {
        definition,
        status,
        agent: None,
    }))
}

/// POST /companies/:companyId/built-in-agents/:key/reset
/// 重置指定内置 Agent（清除资源 + 恢复初始状态）
pub async fn reset_built_in_agent(
    State(state): State<AppState>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    let service = state.built_in_agent_service.clone();
    let key = BuiltInAgentKey::from_str(&key)
        .ok_or((StatusCode::NOT_FOUND, "Unknown built-in agent key".to_string()))?;

    service
        .reset(company_id, key)
        .await
        .map_err(map_built_in_error)?;

    let status = service
        .get_status(company_id, key)
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get status: {}", e))
        })?;

    let definition = service
        .get_definition(key)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "Built-in agent definition missing".to_string()))?;

    Ok(Json(BuiltInAgentStateResponse {
        definition,
        status,
        agent: None,
    }))
}

/// POST /companies/:companyId/built-in-agents/:key/routines/:routine_key/enable
/// 启用内置 Agent 的定时任务
pub async fn enable_built_in_routine(
    State(_state): State<AppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path((_key, _routine_key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // TODO: 实现 Routine enable 逻辑（需要 RoutineService 集成）
    // 1. 解析 key 和 routine_key
    // 2. 查找对应的 Routine
    // 3. 设置 enabled = true
    // 4. 保存更新
    tracing::warn!("Routine enable not yet implemented (stub)");
    Ok(Json(serde_json::json!({"status": "not_implemented"})))
}

/// POST /companies/:companyId/built-in-agents/:key/routines/:routine_key/disable
/// 禁用内置 Agent 的定时任务
pub async fn disable_built_in_routine(
    State(_state): State<AppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path((_key, _routine_key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // TODO: 实现 Routine disable 逻辑
    tracing::warn!("Routine disable not yet implemented (stub)");
    Ok(Json(serde_json::json!({"status": "not_implemented"})))
}

/// POST /companies/:companyId/built-in-agents/:key/routines/:routine_key/run
/// 手动触发内置 Agent 的定时任务
pub async fn run_built_in_routine(
    State(_state): State<AppState>,
    CompanyIdOrShortname(_company_id): CompanyIdOrShortname,
    Path((_key, _routine_key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // TODO: 实现 Routine run 逻辑
    tracing::warn!("Routine run not yet implemented (stub)");
    Ok(Json(serde_json::json!({"status": "not_implemented"})))
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
            "/companies/:company_id/built-in-agents/:key/status",
            get(get_built_in_agent_status),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/provision",
            post(provision_built_in_agent),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/reconcile",
            post(reconcile_built_in_agent),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/reset",
            post(reset_built_in_agent),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/routines/:routine_key/enable",
            post(enable_built_in_routine),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/routines/:routine_key/disable",
            post(disable_built_in_routine),
        )
        .route(
            "/companies/:company_id/built-in-agents/:key/routines/:routine_key/run",
            post(run_built_in_routine),
        )
}

#[cfg(test)]
mod tests {
    // 测试需要 mock BuiltInAgentService
    // 这里只提供框架，实际测试需要实现 mock
}
