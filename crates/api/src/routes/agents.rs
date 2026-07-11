use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use garde::Validate;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::AppError;
use crate::redaction::redact_config;
use crate::schemas::{CreateAgentHireSchema, UpdateAgentSchema};
use access::{AccessService, UserActor};
use services::{AgentService, ConfigRevisionService, CreateAgentInput, UpdateAgentInput};
use serde_json::json;

/// AppState - 应用状态（使用Arc<dyn Trait>避免泛型）
#[derive(Clone)]
pub struct AppState {
    pub agent_service: Arc<dyn AgentService>,
    pub access_service: Arc<dyn AccessService>,
    pub config_revision_service: Arc<dyn ConfigRevisionService>,
}

/// 创建Agent路由
pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/agents", get(list_agents))
        .route("/companies/:company_id/agent-hires", post(create_agent))
        .route("/agents/:id", get(get_agent))
        .route("/agents/:id", patch(update_agent))
        .route("/agents/:id", delete(delete_agent))
        .route("/agents/me", get(get_current_agent))
        .route("/agents/:id/configuration", get(get_agent_configuration))
        .route("/agents/:id/skills", get(get_agent_skills))
        .route("/agents/:id/config-revisions/:revision_id/rollback", post(rollback_config))
}

/// GET /companies/:company_id/agents - 列出公司的所有Agent
async fn list_agents(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id,
        is_admin: true,
    };

    // 验证公司访问权限
    state.access_service.assert_company_access(&actor, company_id).await?;

    // 查询Agent列表
    let agents = state.agent_service.list(company_id).await?;

    Ok(Json(agents))
}

/// POST /companies/:company_id/agent-hires - 创建Agent
async fn create_agent(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<CreateAgentHireSchema>,
) -> Result<impl IntoResponse, AppError> {
    // 验证请求
    payload.validate(&()).map_err(|e| AppError::Validation(e.to_string()))?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id,
        is_admin: true,
    };

    // 验证创建权限
    state.access_service.assert_can_create_agents_for_company(&actor, company_id).await?;

    // 创建Agent
    let input = CreateAgentInput {
        company_id,
        name: payload.name,
        role: payload.role.unwrap_or(models::AgentRole::General),
        adapter_type: payload.adapter_type,
        adapter_config: payload.adapter_config,
        runtime_config: payload.runtime_config,
        permissions: payload.permissions,
        budget_monthly_cents: payload.budget_monthly_cents,
        reports_to: payload.reports_to,
    };

    let agent = state.agent_service.create(input).await?;

    Ok((StatusCode::CREATED, Json(agent)))
}

/// GET /agents/:id - 获取Agent详情
async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;

    // TODO: 从请求中提取Actor并验证读取权限
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    state.access_service.assert_agent_read_allowed(&actor, id).await?;

    Ok(Json(agent))
}

/// PATCH /agents/:id - 更新Agent
async fn update_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateAgentSchema>,
) -> Result<impl IntoResponse, AppError> {
    // 验证请求
    payload.validate(&()).map_err(|e| AppError::Validation(e.to_string()))?;

    // 查询现有Agent
    let agent = state.agent_service.get_by_id(id).await?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证更新权限
    state.access_service.assert_can_update_agent(&actor, id).await?;

    // 更新Agent
    let input = UpdateAgentInput {
        name: payload.name,
        role: payload.role,
        status: payload.status,
        adapter_config: payload.adapter_config,
        runtime_config: payload.runtime_config,
        budget_monthly_cents: payload.budget_monthly_cents,
        reports_to: payload.reports_to,
    };

    let updated_agent = state.agent_service.update(id, input).await?;

    Ok(Json(updated_agent))
}

/// DELETE /agents/:id - 删除Agent（软删除）
async fn delete_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(id).await?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证删除权限
    state.access_service.assert_can_update_agent(&actor, id).await?;

    // 删除Agent
    state.agent_service.delete(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /agents/me - 获取当前认证的Agent
async fn get_current_agent(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 从请求头中提取Agent Key
    let agent_key = "placeholder_key";

    let agent = state.agent_service.get_me(agent_key).await?;

    Ok(Json(agent))
}

/// GET /agents/:id/configuration - 获取Agent的脱敏配置
async fn get_agent_configuration(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询Agent
    let agent = state.agent_service.get_by_id(id).await?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证配置读取权限
    state.access_service.assert_agent_read_allowed(&actor, id).await?;

    // 构建配置对象并脱敏
    let adapter_config_value = serde_json::to_value(&agent.adapter_config)
        .unwrap_or(json!({}));
    let runtime_config_value = serde_json::to_value(&agent.runtime_config)
        .unwrap_or(json!({}));

    let redacted_config = json!({
        "id": agent.id,
        "name": agent.name,
        "adapter_type": agent.adapter_type,
        "adapter_config": redact_config(&adapter_config_value),
        "runtime_config": redact_config(&runtime_config_value),
        "status": agent.status,
        "budget_monthly_cents": agent.budget_monthly_cents,
    });

    Ok(Json(redacted_config))
}

/// GET /agents/:id/skills - 获取Agent技能快照
async fn get_agent_skills(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询Agent
    let agent = state.agent_service.get_by_id(id).await?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证配置读取权限
    state.access_service.assert_agent_read_allowed(&actor, id).await?;

    // 获取技能快照
    let snapshot = state.agent_service.get_skills(id).await?;

    Ok(Json(snapshot))
}

/// POST /agents/:id/config-revisions/:revision_id/rollback - 回滚配置到指定版本
async fn rollback_config(
    State(state): State<AppState>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(agent_id).await?;

    // TODO: 从请求中提取Actor
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证更新权限
    state.access_service.assert_can_update_agent(&actor, agent_id).await?;

    // 执行回滚
    let updated_agent = state.agent_service.rollback_config_revision(agent_id, revision_id).await?;

    Ok(Json(updated_agent))
}
