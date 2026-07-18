use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use garde::Validate;
use uuid::Uuid;

use crate::errors::AppError;
use crate::redaction::redact_config;
use crate::validation::{AgentPermissionsInput, CreateAgentHireSchema, UpdateAgentSchema};
use access::UserActor;
use models::{AgentPermissions, AgentStatus, TrustAuthorizationPolicy, TrustPreset};
use services::{CreateAgentInput, UpdateAgentInput};
use serde_json::json;

use crate::routes::heartbeats::list_scheduler_heartbeats;

/// AppState - 应用状态（使用Arc<dyn Trait>避免泛型）
///
/// 与 `crate::app_state::AppState` 为同一类型（统一状态），
/// 此处仅作为别名以保持路由模块内部的引用一致。
pub use crate::app_state::AppState;

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
        .route("/agents/:id/skills/sync", post(sync_agent_skills))
        .route("/agents/:id/runtime-state/reset-session", post(reset_agent_session))
        // --- P1: Agent 动作 / 子资源 ---
        .route("/agents/:id/runtime-state", get(get_runtime_state))
        .route("/agents/:id/task-sessions", get(get_task_sessions))
        .route("/agents/:id/permissions", patch(update_permissions))
        .route("/agents/:id/instructions-path", patch(update_instructions_path))
        .route("/agents/:id/instructions-bundle", get(get_instructions_bundle).patch(patch_instructions_bundle))
        .route("/agents/:id/instructions-bundle/file", get(get_bundle_file).put(save_bundle_file).delete(delete_bundle_file))
        .route("/agents/:id/keys", get(list_agent_keys).post(create_agent_key))
        .route("/agents/:id/keys/:key_id", delete(revoke_agent_key))
        .route("/agents/:id/pause", post(pause_agent))
        .route("/agents/:id/resume", post(resume_agent))
        .route("/agents/:id/clear-error", post(clear_error_agent))
        .route("/agents/:id/approve", post(approve_agent))
        .route("/agents/:id/terminate", post(terminate_agent))
        .route("/agents/:id/wakeup", post(wakeup_agent))
        .route("/agents/:id/budgets", patch(update_budget))
        .route("/agents/me/inbox-lite", get(get_inbox_lite))
        .route("/agents/me/inbox/mine", get(get_inbox_mine))
        // --- P1.1: 补齐缺失接口 (A1-A6) ---
        .route("/agents/:id/claude-login", post(claude_login))
        .route("/agents/:id/heartbeat/invoke", post(heartbeat_invoke))
        .route("/companies/:company_id/agent-configurations", get(list_agent_configurations))
        .route("/instance/scheduler-heartbeats", get(list_scheduler_heartbeats))
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
        role: payload.role,
        adapter_type: payload.adapter_type,
        adapter_config: payload.adapter_config,
        runtime_config: Some(payload.runtime_config),
        permissions: payload.permissions.map(agent_permissions_from_input),
        budget_monthly_cents: Some(payload.budget_monthly_cents),
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
        reports_to: payload.reports_to.flatten(),
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
///
/// 从 Authorization: Bearer <agent_key> 头中提取 Agent API Key，
/// 验证 key 有效性并返回对应的 Agent 信息。
async fn get_current_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // 从 Authorization header 提取 bearer token
    let agent_key = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::BadRequest("Missing or invalid Authorization header".to_string()))?;

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

/// POST /agents/:id/skills/sync - 同步Agent技能列表
async fn sync_agent_skills(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
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

    // 同步技能
    let skills = state.agent_service.sync_skills(agent_id).await?;

    Ok(Json(skills))
}

/// POST /agents/:id/runtime-state/reset-session - 重置Agent会话
async fn reset_agent_session(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(agent_id).await?;

    // TODO: 从请求中提取Actor（Board管理员）
    let actor = UserActor {
        user_id: Uuid::new_v4(),
        company_id: agent.company_id,
        is_admin: true,
    };

    // 验证更新权限
    state.access_service.assert_can_update_agent(&actor, agent_id).await?;

    // 重置会话
    state.agent_service.reset_session(agent_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// 将校验层输入的权限结构转换为领域模型权限结构
fn agent_permissions_from_input(input: AgentPermissionsInput) -> AgentPermissions {
    let trust_preset = match input.trust_preset.as_deref() {
        Some("restricted") => TrustPreset::Restricted,
        Some("elevated") => TrustPreset::Elevated,
        _ => TrustPreset::Standard,
    };
    let authorization_policy = match input.authorization_policy.as_deref() {
        Some("auto_approve") | Some("autoapprove") => TrustAuthorizationPolicy::AutoApprove,
        _ => TrustAuthorizationPolicy::Manual,
    };

    AgentPermissions {
        can_create_agents: input.can_create_agents.unwrap_or(false),
        can_create_skills: input.can_create_skills.unwrap_or(false),
        trust_preset,
        authorization_policy,
    }
}

// ============================================================================
// P1: Agent 动作 / 子资源 Handlers
// ============================================================================

/// GET /agents/:id/runtime-state - 获取 Agent 运行时状态
async fn get_runtime_state(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let runtime_state = state.agent_service.get_runtime_state(id).await?;
    Ok(Json(runtime_state))
}

/// GET /agents/:id/task-sessions - 获取 Agent 任务会话列表
async fn get_task_sessions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let sessions = state.agent_service.get_task_sessions(id).await?;
    Ok(Json(sessions))
}

/// PATCH /agents/:id/permissions - 更新 Agent 权限
async fn update_permissions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let permissions = payload.get("permissions")
        .ok_or_else(|| AppError::BadRequest("Missing 'permissions' field".to_string()))?;
    let agent = state.agent_service.update_permissions(id, serde_json::from_value(permissions.clone()).map_err(|e| AppError::BadRequest(e.to_string()))?).await?;
    Ok(Json(agent))
}

/// PATCH /agents/:id/instructions-path - 更新 Agent 指令路径
async fn update_instructions_path(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let path = payload.get("instructionsPath")
        .or_else(|| payload.get("instructions_path"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let agent = state.agent_service.update_instructions_path(id, path).await?;
    Ok(Json(agent))
}

/// GET /agents/:id/instructions-bundle - 获取指令包
async fn get_instructions_bundle(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let bundle = state.agent_service.get_instructions_bundle(id).await?;
    Ok(Json(bundle))
}

/// PATCH /agents/:id/instructions-bundle - 更新指令包
async fn patch_instructions_bundle(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.update_instructions_bundle(id, payload).await?;
    Ok(Json(agent))
}

/// GET /agents/:id/instructions-bundle/file - 获取指令文件
async fn get_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // TODO: 从 query string 提取 file_path
    let content = state.agent_service.get_bundle_file(id, "default.md").await?;
    Ok(Json(serde_json::json!({"content": content})))
}

/// PUT /agents/:id/instructions-bundle/file - 保存指令文件
async fn save_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let content = payload.get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'content' field".to_string()))?
        .to_string();
    let agent = state.agent_service.save_bundle_file(id, "default.md", content).await?;
    Ok(Json(agent))
}

/// DELETE /agents/:id/instructions-bundle/file - 删除指令文件
async fn delete_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.delete_bundle_file(id, "default.md").await?;
    Ok(Json(agent))
}

/// GET /agents/:id/keys - 列出 API Key
async fn list_agent_keys(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let keys = state.agent_service.list_keys(id).await?;
    Ok(Json(keys))
}

/// POST /agents/:id/keys - 创建 API Key
async fn create_agent_key(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let name = payload.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'name' field".to_string()))?
        .to_string();
    let key = state.agent_service.create_key(id, name).await?;
    Ok((StatusCode::CREATED, Json(key)))
}

/// DELETE /agents/:id/keys/:key_id - 吊销 API Key
async fn revoke_agent_key(
    State(state): State<AppState>,
    Path((id, key_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    state.agent_service.revoke_key(id, key_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /agents/:id/pause - 暂停 Agent
async fn pause_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Paused).await?;
    Ok(Json(agent))
}

/// POST /agents/:id/resume - 恢复 Agent
async fn resume_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Running).await?;
    Ok(Json(agent))
}

/// POST /agents/:id/clear-error - 清除 Agent 错误状态
async fn clear_error_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Idle).await?;
    Ok(Json(agent))
}

/// POST /agents/:id/approve - 批准 Agent
async fn approve_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Idle).await?;
    Ok(Json(agent))
}

/// POST /agents/:id/terminate - 终止 Agent
async fn terminate_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Terminated).await?;
    Ok(Json(agent))
}

/// POST /agents/:id/wakeup - 唤醒 Agent
async fn wakeup_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.set_status(id, AgentStatus::Running).await?;
    Ok(Json(agent))
}

/// PATCH /agents/:id/budgets - 更新 Agent 预算
async fn update_budget(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let budget_monthly_cents = payload
        .get("budgetMonthlyCents")
        .or_else(|| payload.get("budget_monthly_cents"))
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::BadRequest("Missing 'budgetMonthlyCents' field".to_string()))? as i32;
    let agent = state.agent_service.update_budget(id, budget_monthly_cents).await?;
    Ok(Json(agent))
}

/// GET /agents/me/inbox-lite - 当前 Agent 轻量收件箱
async fn get_inbox_lite(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let key = extract_agent_key(&headers)?;
    let agent = state.agent_service.get_me(&key).await?;
    let inbox = state.agent_service.inbox_lite(agent.id).await?;
    Ok(Json(inbox))
}

/// GET /agents/me/inbox/mine - 当前 Agent 收件箱
async fn get_inbox_mine(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let key = extract_agent_key(&headers)?;
    let agent = state.agent_service.get_me(&key).await?;
    let inbox = state.agent_service.inbox_mine(agent.id).await?;
    Ok(Json(inbox))
}

// ============================================================================
// P1.1: 补齐缺失接口 (A1-A6) Handlers
// ============================================================================

/// POST /agents/:id/claude-login - Claude 登录 (A1)
async fn claude_login(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.agent_service.claude_login(id).await?;
    Ok(Json(result))
}

/// POST /agents/:id/heartbeat/invoke - 触发心跳调用 (A2)
async fn heartbeat_invoke(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    let evaluated = state.watchdog_service.evaluate_all(agent.company_id).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "heartbeatInvoked": true,
        "watchdogsEvaluated": evaluated,
        "agentId": id,
    })))
}

/// GET /companies/:company_id/agent-configurations - 公司级 Agent 配置列表 (A5)
async fn list_agent_configurations(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let configs = state.agent_service.list_configurations(company_id).await?;
    Ok(Json(configs))
}

/// 从 Authorization 头提取 Agent Key
fn extract_agent_key(headers: &HeaderMap) -> Result<String, AppError> {
    headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(String::from)
        .ok_or_else(|| AppError::BadRequest("Missing or invalid Authorization header".to_string()))
}
