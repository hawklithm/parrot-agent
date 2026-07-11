use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use services::{
    BuiltInAgentService, BuiltInAgentStatus, BuiltInAgentDefinition,
    BuiltInAgentError, ReconcileResult,
};
use crate::extractors::CompanyIdOrShortname;
use crate::validation::agent_schemas::ProvisionBuiltInAgentSchema;
use garde::Validate;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltInAgentStateResponse {
    pub definition: BuiltInAgentDefinition,
    pub status: BuiltInAgentStatus,
    pub agent_id: Option<uuid::Uuid>,
    pub agent: Option<serde_json::Value>,
    pub pause_reason: Option<String>,
    pub resources: Vec<serde_json::Value>,
    pub approval: Option<serde_json::Value>,
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

/// GET /companies/:companyId/built-in-agents
/// 列出公司所有内置 Agent 的状态
pub async fn list_built_in_agents(
    State(service): State<Arc<dyn BuiltInAgentService>>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
) -> Result<Json<Vec<BuiltInAgentStateResponse>>, (StatusCode, String)> {
    let states = service
        .list_built_in_agents(company_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list built-in agents: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to list built-in agents: {}", e))
        })?;

    let responses: Vec<BuiltInAgentStateResponse> = states
        .into_iter()
        .map(|state| BuiltInAgentStateResponse {
            definition: state.definition,
            status: state.status,
            agent_id: state.agent_id,
            agent: state.agent.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
            pause_reason: state.pause_reason,
            resources: state.resources,
            approval: state.approval.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
        })
        .collect();

    Ok(Json(responses))
}

/// POST /companies/:companyId/built-in-agents/:key/provision
/// 配置指定的内置 Agent
pub async fn provision_built_in_agent(
    State(service): State<Arc<dyn BuiltInAgentService>>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
    Json(payload): Json<ProvisionBuiltInAgentRequest>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    // 构造 provision 输入
    let schema = ProvisionBuiltInAgentSchema {
        adapter_type: payload.adapter_type.clone(),
        adapter_config: payload.adapter_config.clone(),
        budget_monthly_cents: payload.budget_monthly_cents,
    };

    // 验证输入
    schema.validate().map_err(|e| {
        tracing::warn!("Provision validation failed: {:?}", e);
        (StatusCode::BAD_REQUEST, format!("Validation error: {}", e))
    })?;

    // 调用服务层
    let state = service
        .provision_built_in_agent(
            company_id,
            &key,
            payload.adapter_type,
            payload.adapter_config,
            payload.budget_monthly_cents,
        )
        .await
        .map_err(|e| match e {
            BuiltInAgentError::NotFound => (StatusCode::NOT_FOUND, "Built-in agent not found".to_string()),
            BuiltInAgentError::Forbidden => (StatusCode::FORBIDDEN, "Permission denied".to_string()),
            BuiltInAgentError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            BuiltInAgentError::AlreadyProvisioned => (StatusCode::CONFLICT, "Agent already provisioned".to_string()),
            _ => {
                tracing::error!("Failed to provision built-in agent: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to provision: {}", e))
            }
        })?;

    let response = BuiltInAgentStateResponse {
        definition: state.definition,
        status: state.status,
        agent_id: state.agent_id,
        agent: state.agent.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
        pause_reason: state.pause_reason,
        resources: state.resources,
        approval: state.approval.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
    };

    Ok(Json(response))
}

/// POST /companies/:companyId/built-in-agents/:key/reconcile
/// 协调指定内置 Agent 的状态（重新应用默认配置）
pub async fn reconcile_built_in_agent(
    State(service): State<Arc<dyn BuiltInAgentService>>,
    CompanyIdOrShortname(company_id): CompanyIdOrShortname,
    Path(key): Path<String>,
    Json(_payload): Json<ReconcileBuiltInAgentRequest>,
) -> Result<Json<BuiltInAgentStateResponse>, (StatusCode, String)> {
    let result = service
        .reconcile_built_in_agent(company_id, &key)
        .await
        .map_err(|e| match e {
            BuiltInAgentError::NotFound => (StatusCode::NOT_FOUND, "Built-in agent not found".to_string()),
            BuiltInAgentError::Forbidden => (StatusCode::FORBIDDEN, "Permission denied".to_string()),
            BuiltInAgentError::NotProvisioned => (
                StatusCode::PRECONDITION_FAILED,
                "Agent not provisioned yet".to_string(),
            ),
            _ => {
                tracing::error!("Failed to reconcile built-in agent: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to reconcile: {}", e))
            }
        })?;

    let response = match result {
        ReconcileResult::State(state) => BuiltInAgentStateResponse {
            definition: state.definition,
            status: state.status,
            agent_id: state.agent_id,
            agent: state.agent.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
            pause_reason: state.pause_reason,
            resources: state.resources,
            approval: state.approval.map(|a| serde_json::to_value(a).unwrap_or(serde_json::Value::Null)),
        },
    };

    Ok(Json(response))
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
