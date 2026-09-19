use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use garde::Validate;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::AppError;
use crate::redaction::redact_config;
use crate::validation::{
    AgentPermissionsInput, CreateAgentHireSchema, InstructionsBundleInput, UpdateAgentSchema,
};
use models::{AgentPermissions, AgentStatus, ApprovalType, TrustAuthorizationPolicy, TrustPreset};
use serde_json::{json, Value};
use services::approval_service::CreateApprovalInput;
use services::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService};
use services::{CreateAgentInput, HeartbeatWakeupOptions, UpdateAgentInput};

use crate::routes::heartbeats::list_scheduler_heartbeats;

/// AppState - 应用状态（使用Arc<dyn Trait>避免泛型）
///
/// 与 `crate::app_state::AppState` 为同一类型（统一状态），
/// 此处仅作为别名以保持路由模块内部的引用一致。
pub use crate::app_state::AppState;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentReferenceQuery {
    company_id: Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentWakeupRequest {
    source: Option<String>,
    trigger_detail: Option<String>,
    reason: Option<String>,
    payload: Option<Value>,
    idempotency_key: Option<String>,
    context_snapshot: Option<Value>,
    force_fresh_session: Option<bool>,
}

/// 创建Agent路由
pub fn agent_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/agents", get(list_agents).post(create_agent))
        .route("/companies/:company_id/agent-hires", post(create_agent))
        .route("/agents/:id", get(get_agent))
        .route("/agents/:id", patch(update_agent))
        .route("/agents/:id", delete(delete_agent))
        .route("/agents/me", get(get_current_agent))
        .route("/agents/:id/configuration", get(get_agent_configuration))
        .route("/agents/:id/skills", get(get_agent_skills))
        .route(
            "/agents/:id/config-revisions/:revision_id/rollback",
            post(rollback_config),
        )
        .route("/agents/:id/skills/sync", post(sync_agent_skills))
        .route(
            "/agents/:id/runtime-state/reset-session",
            post(reset_agent_session),
        )
        // --- P1: Agent 动作 / 子资源 ---
        .route("/agents/:id/runtime-state", get(get_runtime_state))
        .route("/agents/:id/task-sessions", get(get_task_sessions))
        .route("/agents/:id/permissions", patch(update_permissions))
        .route(
            "/agents/:id/instructions-path",
            patch(update_instructions_path),
        )
        .route(
            "/agents/:id/instructions-bundle",
            get(get_instructions_bundle).patch(patch_instructions_bundle),
        )
        .route(
            "/agents/:id/instructions-bundle/file",
            get(get_bundle_file)
                .put(save_bundle_file)
                .delete(delete_bundle_file),
        )
        .route(
            "/agents/:id/keys",
            get(list_agent_keys).post(create_agent_key),
        )
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
        .route(
            "/companies/:company_id/agent-configurations",
            get(list_agent_configurations),
        )
        .route(
            "/instance/scheduler-heartbeats",
            get(list_scheduler_heartbeats),
        )
        // --- P1.2: 补齐剩余缺失接口 (A15-A25) ---
        .route("/agents/:id/interrupt", post(interrupt_agent))
        .route("/agents/:id/reset-credentials", post(reset_agent_credentials))
        .route("/agents/:id/credentials", get(get_agent_credentials))
        .route("/agents/:id/metrics", get(get_agent_metrics))
        .route("/agents/:id/activity", get(get_agent_activity))
        .route("/agents/:id/permissions", post(add_agent_permission))
        .route("/agents/:id/permissions/:permission_id", delete(delete_agent_permission))
        .route("/agents/:id/skills/:skill_id", delete(delete_agent_skill))
        .route("/agents/:id/sessions/:session_id", get(get_agent_session).delete(delete_agent_session))
        .route("/agents/:id/runs/:run_id", get(get_agent_run))
}

/// GET /companies/:company_id/agents - 列出公司的所有Agent
async fn list_agents(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    crate::routes::assert_company_access(&auth_actor, company_id, true)
        .map_err(|error| AppError::Forbidden(error.to_string()))?;

    // 查询Agent列表
    let agents = state.agent_service.list(company_id).await?;

    Ok(Json(agents))
}

/// POST /companies/:company_id/agent-hires - 创建Agent
async fn create_agent(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<CreateAgentHireSchema>,
) -> Result<impl IntoResponse, AppError> {
    // 验证请求
    payload
        .validate(&())
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Use the actor resolved by the global auth middleware. This matches
    // Paperclip's assertCanCreateAgentsForCompany behavior, including the
    // local_implicit development bypass and company-scoped role/grant checks.
    let action = AuthorizationAction::AgentHire { company_id };
    let authorization_decision = AuthorizationService::decide(
        &state.pool,
        &auth_actor,
        &action,
        Some(company_id),
    )
    .await;
    if !authorization_decision.allowed {
        tracing::warn!(
            company_id = %company_id,
            actor_id = ?auth_actor.principal_id(),
            decision_reason = %authorization_decision.reason,
            decision_code = ?authorization_decision.code,
            "agent hire authorization denied"
        );
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agents:create permission".to_string(),
        ));
    }

    // Paperclip creates the agent in pending_approval status when the company
    // requires board approval. Keep the company lookup here so the behavior is
    // identical for both the normal and approval-backed paths.
    let requires_approval = sqlx::query_scalar::<_, bool>(
        "SELECT require_board_approval_for_new_agents FROM companies WHERE id = $1",
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(format!("Failed to load company: {error}")))?
    .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;

    let (requested_by_agent_id, requested_by_user_id) = match &auth_actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (None, Some(*user_id)),
        AuthorizationActor::None => (None, None),
    };

    let mut source_issue_ids = payload
        .source_issue_ids
        .clone()
        .unwrap_or_default()
        .into_iter()
        .chain(payload.source_issue_id.iter().copied())
        .collect::<Vec<_>>();
    let mut seen_source_issue_ids = std::collections::HashSet::new();
    source_issue_ids.retain(|issue_id| seen_source_issue_ids.insert(*issue_id));

    // Paperclip keeps the requested skill set in the adapter configuration so
    // the runtime sees it immediately, including while the agent is waiting
    // for board approval. Normalize null configs the same way the shared
    // Paperclip validator does.
    let mut adapter_config = payload.adapter_config.clone();
    if adapter_config.is_null() {
        adapter_config = json!({});
    }
    if let Some(desired_skills) = payload.desired_skills.as_ref() {
        let object = adapter_config.as_object_mut().ok_or_else(|| {
            AppError::Validation("adapterConfig must be a JSON object".to_string())
        })?;
        object.insert("desired_skills".to_string(), json!(desired_skills));
    }

    // The API accepts an omitted entryFile and Paperclip defaults it to
    // AGENTS.md. Store the canonical flat bundle shape that heartbeat and
    // instructions readers consume.
    let instructions_bundle = payload
        .instructions_bundle
        .as_ref()
        .map(|bundle: &InstructionsBundleInput| {
            json!({
                "entryFile": bundle
                    .entry_file
                    .clone()
                    .unwrap_or_else(|| "AGENTS.md".to_string()),
                "files": bundle.files.clone(),
            })
        });

    // 创建Agent
    let input = CreateAgentInput {
        company_id,
        name: payload.name.clone(),
        role: payload.role,
        status: Some(if requires_approval {
            AgentStatus::PendingApproval
        } else {
            AgentStatus::Idle
        }),
        adapter_type: payload.adapter_type.clone(),
        adapter_config,
        instructions_bundle: instructions_bundle.clone(),
        runtime_config: Some(payload.runtime_config.clone()),
        permissions: payload.permissions.map(agent_permissions_from_input),
        budget_monthly_cents: Some(payload.budget_monthly_cents),
        reports_to: payload.reports_to,
    };

    let agent = state.agent_service.create(input).await.map_err(|error| {
        tracing::error!(error = %error, "agent hire failed while creating agent");
        error
    })?;

    let approval = if requires_approval {
        let approval_payload = json!({
            // These two keys are required by ApprovalService's hire payload
            // validation and are also compatible with Paperclip's payload.
            "agent_name": payload.name,
            "agent_role": payload.role,
            "agent_id": agent.id,
            "name": agent.name,
            "role": agent.role,
            "title": payload.title,
            "icon": payload.icon,
            "reportsTo": payload.reports_to,
            "capabilities": payload.capabilities,
            "desiredSkills": payload.desired_skills,
            "instructionsBundle": instructions_bundle,
            "adapterType": agent.adapter_type,
            "adapterConfig": agent.adapter_config.0,
            "runtimeConfig": agent.runtime_config.0,
            "budgetMonthlyCents": agent.budget_monthly_cents,
            "metadata": payload.metadata,
            "sourceIssueId": payload.source_issue_id,
            "sourceIssueIds": source_issue_ids,
            "agentId": agent.id,
            "requestedByAgentId": requested_by_agent_id,
            "requestedConfigurationSnapshot": {
                "adapterType": agent.adapter_type,
                "adapterConfig": agent.adapter_config.0,
                "runtimeConfig": agent.runtime_config.0,
                "budgetMonthlyCents": agent.budget_monthly_cents,
                "desiredSkills": payload.desired_skills,
                "instructionsBundle": instructions_bundle,
            },
        });

        Some(
            state
                .approval_service
                .create(CreateApprovalInput {
                    company_id,
                    approval_type: ApprovalType::HireAgent,
                    requested_by_agent_id,
                    requested_by_user_id,
                    payload: approval_payload,
                    linked_issue_ids: source_issue_ids,
                    validate_payload: true,
                })
                .await
                .map_err(|error| {
                    tracing::error!(error = %error, agent_id = %agent.id, "agent hire approval creation failed");
                    error
                })?,
        )
    } else {
        None
    };

    // Paperclip's hire contract is an envelope. The UI reads `hire.agent.id`
    // and optionally follows `hire.approval.id` during onboarding.
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "agent": agent,
            "approval": approval,
        })),
    ))
}

/// GET /agents/:id - 获取Agent详情
async fn get_agent(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(raw_id): Path<String>,
    Query(query): Query<AgentReferenceQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Paperclip accepts either a UUID or a company-scoped URL key. The key is a
    // derived projection of the name, so the comparison uses the same SQL
    // fragment the row mapper's Rust derivation mirrors.
    let id = match raw_id.parse::<Uuid>() {
        Ok(id) => id,
        Err(_) => {
            let company_id = query.company_id.ok_or_else(|| {
                AppError::Unprocessable(
                    "Agent shortname lookup requires companyId query parameter".to_string(),
                )
            })?;
            let Some(url_key) = models::agent_url_key::normalize_agent_url_key(&raw_id) else {
                return Err(AppError::NotFound("Agent not found".to_string()));
            };
            let matches = sqlx::query_scalar::<_, Uuid>(&format!(
                "SELECT id FROM agents \
                 WHERE company_id = $1 \
                   AND status <> 'terminated' \
                   AND {url_key_sql} = $2 \
                 ORDER BY created_at ASC",
                url_key_sql = models::agent_url_key::agent_url_key_sql("agents"),
            ))
            .bind(company_id)
            .bind(&url_key)
            .fetch_all(&state.pool)
            .await
            .map_err(|error| {
                AppError::InternalServerError(format!("Failed to resolve agent reference: {error}"))
            })?;
            match matches.as_slice() {
                [] => return Err(AppError::NotFound("Agent not found".to_string())),
                [id] => *id,
                _ => {
                    return Err(AppError::Conflict(
                        "Agent shortname is ambiguous in this company. Use the agent ID."
                            .to_string(),
                    ))
                }
            }
        }
    };

    // Paperclip resolves a missing UUID as a normal 404.  The repository
    // abstraction currently returns an error for a missing row, which the
    // generic handler exposed as 500 (this is especially visible for stale
    // links to agents removed during onboarding cleanup).
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1 AND ($2::uuid IS NULL OR company_id = $2))",
    )
    .bind(id)
    .bind(query.company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(format!("Failed to resolve agent: {error}")))?;
    if !exists {
        return Err(AppError::NotFound("Agent not found".to_string()));
    }

    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:read permission".to_string(),
        ));
    }

    // Paperclip exposes the management chain on the normal agent detail
    // response (not only on /agents/me).  Keep it best-effort so a stale
    // hierarchy reference does not hide an otherwise readable agent.
    let chain_of_command = state
        .agent_service
        .get_chain_of_command(agent.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|manager| {
            serde_json::json!({
                "id": manager.id,
                "name": manager.name,
                "role": manager.role,
                "title": serde_json::Value::Null,
            })
        })
        .collect::<Vec<_>>();
    let mut agent_json = serde_json::to_value(&agent)
        .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    if let Some(object) = agent_json.as_object_mut() {
        object.insert("chainOfCommand".to_string(), serde_json::json!(chain_of_command));
    }
    Ok(Json(agent_json))
}

/// PATCH /agents/:id - 更新Agent
async fn update_agent(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateAgentSchema>,
) -> Result<impl IntoResponse, AppError> {
    // 验证请求
    payload
        .validate(&())
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // 查询现有Agent
    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:update permission".to_string(),
        ));
    }

    // 更新Agent
    let input = UpdateAgentInput {
        name: payload.name,
        role: payload.role,
        status: payload.status,
        adapter_type: payload.adapter_type,
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
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentDelete { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:delete permission".to_string(),
        ));
    }

    // 删除Agent
    state.agent_service.delete(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /agents/me - 获取当前认证的Agent
///
/// 从 Authorization: Bearer <agent_key> 头中提取 Agent API Key，
/// 验证 key 有效性并返回对应的 Agent 信息。
/// GET /agents/me - 获取当前认证的 Agent 详细信息
/// 验证 key 有效性并返回对应的 Agent 信息，包含 chainOfCommand 和 access。
async fn get_current_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // 从 Authorization header 提取 bearer token
    let agent_key = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            AppError::BadRequest("Missing or invalid Authorization header".to_string())
        })?;

    let agent = state.agent_service.get_me(agent_key).await?;

    // Build chain of command
    let mut chain_of_command = Vec::new();
    let mut current_id = agent.reports_to;
    let mut visited = std::collections::HashSet::new();
    visited.insert(agent.id);

    while let Some(id) = current_id {
        if visited.contains(&id) || chain_of_command.len() >= 50 {
            break;
        }
        visited.insert(id);

        match state.agent_service.get_by_id(id).await {
            Ok(mgr) => {
                // Try to extract title from metadata if it exists
                let title: Option<String> = serde_json::to_value(&mgr.metadata.0)
                    .ok()
                    .and_then(|v| v.get("title").and_then(|t| t.as_str()).map(|s| s.to_string()));

                chain_of_command.push(serde_json::json!({
                    "id": mgr.id,
                    "name": mgr.name,
                    "role": mgr.role,
                    "title": title
                }));
                current_id = mgr.reports_to;
            }
            Err(_) => break,
        }
    }

    // Build access state
    let can_create_agents = agent.permissions.0.can_create_agents;
    let can_assign_tasks = matches!(agent.role, models::AgentRole::Ceo) || can_create_agents;
    let task_assign_source = if matches!(agent.role, models::AgentRole::Ceo) {
        "ceo_role"
    } else if can_create_agents {
        "agent_creator"
    } else if can_assign_tasks {
        "simple_default"
    } else {
        "none"
    };

    let access = serde_json::json!({
        "canAssignTasks": can_assign_tasks,
        "taskAssignSource": task_assign_source,
    });

    // Combine agent data with chain of command and access
    let mut agent_json = serde_json::to_value(&agent)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    
    if let Some(obj) = agent_json.as_object_mut() {
        obj.insert("chainOfCommand".to_string(), serde_json::json!(chain_of_command));
        obj.insert("access".to_string(), access);
    }

    Ok(Json(agent_json))
}

/// GET /agents/:id/configuration - 获取Agent的脱敏配置
async fn get_agent_configuration(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询Agent
    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:read permission".to_string(),
        ));
    }

    // 构建配置对象并脱敏
    let adapter_config_value = serde_json::to_value(&agent.adapter_config).unwrap_or(json!({}));
    let runtime_config_value = serde_json::to_value(&agent.runtime_config).unwrap_or(json!({}));

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
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询Agent
    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:read permission".to_string(),
        ));
    }

    // 获取技能快照
    let snapshot = state.agent_service.get_skills(id).await?;

    Ok(Json(snapshot))
}

/// POST /agents/:id/config-revisions/:revision_id/rollback - 回滚配置到指定版本
async fn rollback_config(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((agent_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(agent_id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:update permission".to_string(),
        ));
    }

    // 执行回滚
    let updated_agent = state
        .agent_service
        .rollback_config_revision(agent_id, revision_id)
        .await?;

    Ok(Json(updated_agent))
}

/// POST /agents/:id/skills/sync - 同步Agent技能列表
#[derive(Debug, Deserialize)]
struct SyncAgentSkillsInput {
    #[serde(rename = "desiredSkills")]
    desired_skills: Vec<serde_json::Value>,
}

async fn sync_agent_skills(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<SyncAgentSkillsInput>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(agent_id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:update permission".to_string(),
        ));
    }

    // 同步技能：把 desiredSkills 写入 adapter_config.desired_skills 并返回最新快照。
    // 元素支持旧字符串 key 或带 versionId 的对象。
    let desired_skills: Vec<models::AgentDesiredSkillEntry> = body
        .desired_skills
        .iter()
        .filter_map(|value| {
            if let Some(key) = value.as_str() {
                return Some(models::AgentDesiredSkillEntry {
                    key: key.to_string(),
                    version_id: None,
                });
            }
            let key = value.get("key").and_then(|key| key.as_str())?;
            Some(models::AgentDesiredSkillEntry {
                key: key.to_string(),
                version_id: value
                    .get("versionId")
                    .and_then(|version| version.as_str())
                    .map(String::from),
            })
        })
        .collect();

    let skills = state
        .agent_service
        .sync_skills(agent_id, desired_skills)
        .await?;

    Ok(Json(skills))
}

/// POST /agents/:id/runtime-state/reset-session - 重置Agent会话
async fn reset_agent_session(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // 查询现有Agent
    let agent = state.agent_service.get_by_id(agent_id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:update permission".to_string(),
        ));
    }

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
    let permissions = payload
        .get("permissions")
        .ok_or_else(|| AppError::BadRequest("Missing 'permissions' field".to_string()))?;
    let agent = state
        .agent_service
        .update_permissions(
            id,
            serde_json::from_value(permissions.clone())
                .map_err(|e| AppError::BadRequest(e.to_string()))?,
        )
        .await?;
    Ok(Json(agent))
}

/// PATCH /agents/:id/instructions-path - 更新 Agent 指令路径
async fn update_instructions_path(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let path = payload
        .get("instructionsPath")
        .or_else(|| payload.get("instructions_path"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let agent = state
        .agent_service
        .update_instructions_path(id, path)
        .await?;
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
    state
        .agent_service
        .update_instructions_bundle(id, payload)
        .await?;
    let bundle = state.agent_service.get_instructions_bundle(id).await?;
    Ok(Json(bundle))
}

/// GET /agents/:id/instructions-bundle/file - 获取指令文件
async fn get_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<InstructionsFileQuery>,
) -> Result<impl IntoResponse, AppError> {
    let path = required_instruction_path(query.path)?;
    let content = state
        .agent_service
        .get_bundle_file(id, &path)
        .await?;
    Ok(Json(serde_json::json!({"path": path, "content": content})))
}

/// PUT /agents/:id/instructions-bundle/file - 保存指令文件
async fn save_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let path = payload
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'path' field".to_string()))?;
    let content = payload
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'content' field".to_string()))?
        .to_string();
    let agent = state
        .agent_service
        .save_bundle_file(id, path, content)
        .await?;
    Ok(Json(agent))
}

/// DELETE /agents/:id/instructions-bundle/file - 删除指令文件
async fn delete_bundle_file(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<InstructionsFileQuery>,
) -> Result<impl IntoResponse, AppError> {
    let path = required_instruction_path(query.path)?;
    let agent = state
        .agent_service
        .delete_bundle_file(id, &path)
        .await?;
    Ok(Json(agent))
}

#[derive(Debug, serde::Deserialize)]
struct InstructionsFileQuery { path: Option<String> }

fn required_instruction_path(path: Option<String>) -> Result<String, AppError> {
    let path = path.ok_or_else(|| AppError::Validation("Query parameter 'path' is required".to_string()))?;
    if path.trim().is_empty() { return Err(AppError::Validation("Query parameter 'path' is required".to_string())); }
    Ok(path)
}

/// `GET /agents/:id/keys` 的响应元素 —— Paperclip `listKeys`
/// （`services/agents.ts:1095-1114`）。
///
/// 注意 Paperclip **不**回 `agentId` / `companyId` / `lastUsedAt`；scope 经过
/// `normalizeAgentApiKeyScope` 归一化后再出网。
fn agent_key_json(key: &models::AgentApiKey) -> serde_json::Value {
    json!({
        "id": key.id,
        "name": key.name,
        "scope": normalize_agent_api_key_scope(&key.scope),
        "responsibleUserId": key.responsible_user_id,
        "createdAt": key.created_at,
        "revokedAt": key.revoked_at,
    })
}

/// `normalizeAgentApiKeyScope`（`validators/agent.ts:182-185`）。
///
/// 任何解析失败都折叠成 `{kind:"standard"}`，因此响应里的 scope 永远是一个
/// 合法联合成员。
fn normalize_agent_api_key_scope(scope: &serde_json::Value) -> serde_json::Value {
    match services::auth::AgentApiKeyScope::from_json(scope.clone()) {
        Some(parsed) => {
            // 只出网 Paperclip 认识的字段；Parrot 侧的 `agentId`/`companyId`
            // 审计信息不外泄。
            let mut value = serde_json::to_value(&parsed).unwrap_or_else(|_| json!({}));
            if let Some(object) = value.as_object_mut() {
                object.retain(|key, _| key != "agentId" && key != "companyId");
                object.retain(|_, value| !is_empty_json_value(value));
            }
            value
        }
        None => json!({ "kind": "standard" }),
    }
}

fn is_empty_json_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Array(items) => items.is_empty(),
        _ => false,
    }
}

/// 校验并归一化 `POST /agents/:id/keys` 的请求体。
///
/// 对应 Paperclip `validate(createAgentKeySchema)`
/// （`validators/agent.ts:187-190` + `middleware/validate.ts`）：任何
/// Zod failure 都是 **400** `{"error":"Validation error"}`。
///
/// 返回值为要落库的 scope（`None` 表示 standard，不落边界配置），与
/// Paperclip `scopeConfig: scope.kind === "standard" ? null : scope` 一致。
fn parse_create_agent_key_body(
    body: &serde_json::Value,
) -> Result<(String, Option<serde_json::Value>), AppError> {
    let object = body.as_object().ok_or_else(|| {
        AppError::Validation("Expected a JSON object body".to_string())
    })?;

    // `z.object({...})` 在 Zod 里默认剥离未知键而非报错，所以这里同样容忍
    // 额外字段（Paperclip 该 schema 没有 `.strict()`）。
    let name = match object.get("name") {
        None => "default".to_string(),
        Some(value) => value
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                AppError::Validation("Invalid agent key name".to_string())
            })?
            .to_string(),
    };

    let scope = match object.get("scope") {
        None | Some(serde_json::Value::Null) => {
            return Ok((name, None));
        }
        Some(value) => value,
    };

    let scope_object = scope
        .as_object()
        .ok_or_else(|| AppError::Validation("Invalid agent key scope".to_string()))?;

    let kind = scope_object
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::Validation("Invalid agent key scope".to_string()))?;
    if !services::auth::actor::AGENT_KEY_SCOPE_KINDS.contains(&kind) {
        return Err(AppError::Validation("Invalid agent key scope".to_string()));
    }

    // `standardAgentKeyScopeSchema` / `skillTestAgentKeyScopeSchema` 都是
    // `.strict()`；`taskBridgeAgentKeyScopeSchema` 亦然。
    const STANDARD_FIELDS: [&str; 1] = ["kind"];
    const TASK_BRIDGE_FIELDS: [&str; 6] = [
        "kind",
        "projectId",
        "projectIds",
        "parentIssueId",
        "parentIssueIds",
        "allowedAssigneeAgentIds",
    ];
    const SKILL_TEST_FIELDS: [&str; 2] = ["kind", "issueId"];
    let allowed: &[&str] = match kind {
        "standard" => &STANDARD_FIELDS,
        "task_bridge" => &TASK_BRIDGE_FIELDS,
        _ => &SKILL_TEST_FIELDS,
    };
    if let Some(unknown) = scope_object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(AppError::Validation(format!(
            "Unrecognized key(s) in agent key scope: {unknown}"
        )));
    }

    // 形状检查交给 serde；`task_bridge` 的「至少一个边界」与 `skill_test`
    // 的必填 issue 由 `from_json` 的归一化规则判定（它会折叠成 standard，
    // 因此这里需要显式判断，见下）。
    let parsed: services::auth::AgentApiKeyScope = serde_json::from_value(scope.clone())
        .map_err(|_| AppError::Validation("Invalid agent key scope".to_string()))?;

    match parsed.kind.as_str() {
        "task_bridge" => {
            // `superRefine`：`"task_bridge keys require at least one project or
            // parent issue boundary"`（`validators/agent.ts:154-160`）。
            if !parsed.has_task_bridge_boundary() {
                return Err(AppError::Validation(
                    "task_bridge keys require at least one project or parent issue boundary"
                        .to_string(),
                ));
            }
            Ok((name, Some(scope.clone())))
        }
        "skill_test" => {
            if parsed.issue_id.is_none() {
                return Err(AppError::Validation(
                    "Invalid agent key scope".to_string(),
                ));
            }
            Ok((name, Some(scope.clone())))
        }
        _ => Ok((name, None)),
    }
}

/// 解析 `GET`/`DELETE /agents/:id/keys*` 的目标 agent，并执行
/// Paperclip `getAccessibleAgent`（`routes/agents.ts:1294-1301`）的授权。
///
/// 顺序很重要：资源查找 + `hasCompanyAccess` 先折叠成 **404**
/// （`getAccessibleResource`，`authz.ts:182-195`），只有公司可见之后才做
/// `assertBoardCanManageAgentsForCompany`（`assertBoard` → 403
/// `"Board access required"`，公司访问 → 403，
/// `agents:create` 决策 → 403 + 决策说明）。
async fn accessible_agent_for_keys(
    state: &AppState,
    actor: &AuthorizationActor,
    id: Uuid,
) -> Result<models::Agent, AppError> {
    let agent = match state.agent_service.get_by_id(id).await {
        Ok(agent) => agent,
        // 仓库层对缺失行返回 `RepositoryError::NotFound`（而不是 `Ok(None)`），
        // Paperclip 同路径解析成 404 `"Agent not found"`。
        Err(services::agent_service::ServiceError::Repository(
            repositories::RepositoryError::NotFound(_),
        )) => {
            return Err(AppError::NotFound("Agent not found".to_string()));
        }
        Err(error) => return Err(services::errors::ServiceError::from(error).into()),
    };

    if !crate::routes::has_company_access(actor, agent.company_id) {
        return Err(AppError::NotFound("Agent not found".to_string()));
    }

    crate::routes::assert_board(actor).map_err(|_| {
        AppError::Forbidden("Board access required".to_string())
    })?;
    crate::routes::assert_company_access(actor, agent.company_id, false).map_err(|_| {
        AppError::Forbidden("User does not have access to this company".to_string())
    })?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        actor,
        &AuthorizationAction::AgentCreate {
            company_id: agent.company_id,
        },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agents:create permission".to_string(),
        ));
    }

    Ok(agent)
}

/// GET /agents/:id/keys - 列出 API Key
async fn list_agent_keys(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let _agent = accessible_agent_for_keys(&state, &auth_actor, id).await?;
    let keys = state.agent_service.list_keys(id).await?;
    let response: Vec<serde_json::Value> = keys.iter().map(agent_key_json).collect();
    Ok(Json(response))
}

/// POST /agents/:id/keys - 创建 API Key
async fn create_agent_key(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // Paperclip 把签发 key 的 board 用户记为 key 的 responsible user
    // （`server/src/routes/agents.ts:4177`）。这个值决定 agent 用该 key
    // 调用公司级接口时代表谁，缺失即 key 不可用。
    let responsible_user_id = match &auth_actor {
        AuthorizationActor::Board { user_id, .. } => Some(*user_id),
        _ => None,
    };
    let agent = accessible_agent_for_keys(&state, &auth_actor, id).await?;
    let (name, scope) = parse_create_agent_key_body(&payload)?;

    let created = state
        .agent_service
        .create_key(id, name, scope, responsible_user_id)
        .await?;

    crate::routes::log_activity(
        &state.pool,
        agent.company_id,
        "agent.key_created",
        &auth_actor,
        "agent",
        agent.id,
        json!({
            "keyId": created.key.id,
            "name": created.key.name,
            "scope": agent_key_json(&created.key)["scope"],
            "responsibleUserId": created.key.responsible_user_id,
        }),
    )
    .await;

    // Paperclip `res.status(201).json(key)` —— 明文 token 只在这里出现一次。
    let response = json!({
        "id": created.key.id,
        "name": created.key.name,
        "scope": agent_key_json(&created.key)["scope"],
        "responsibleUserId": created.key.responsible_user_id,
        "token": created.token,
        "createdAt": created.key.created_at,
    });
    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /agents/:id/keys/:key_id - 吊销 API Key
async fn revoke_agent_key(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((id, key_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let agent = accessible_agent_for_keys(&state, &auth_actor, id).await?;

    // Paperclip 先 `getKeyById` 判断归属（`routes/agents.ts:4207-4211`），
    // 再按 `where(id, agentId)` 撤销（:4213）。两步都可能 404。
    let key = state
        .agent_service
        .list_keys(id)
        .await?
        .into_iter()
        .find(|key| key.id == key_id)
        .filter(|key| key.agent_id == agent.id);
    let Some(key) = key else {
        return Err(AppError::NotFound("Key not found".to_string()));
    };

    if state.agent_service.revoke_key(id, key_id).await?.is_none() {
        return Err(AppError::NotFound("Key not found".to_string()));
    }

    crate::routes::log_activity(
        &state.pool,
        agent.company_id,
        "agent.key_revoked",
        &auth_actor,
        "agent",
        agent.id,
        json!({ "keyId": key.id, "name": key.name }),
    )
    .await;

    Ok(Json(json!({ "ok": true })))
}

/// POST /agents/:id/pause - 暂停 Agent
async fn pause_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state
        .agent_service
        .set_status(id, AgentStatus::Paused)
        .await?;
    Ok(Json(agent))
}

/// POST /agents/:id/resume - 恢复 Agent
async fn resume_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state
        .agent_service
        .set_status(id, AgentStatus::Running)
        .await?;
    Ok(Json(agent))
}

/// POST /agents/:id/clear-error - 清除 Agent 错误状态
async fn clear_error_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state
        .agent_service
        .set_status(id, AgentStatus::Idle)
        .await?;
    Ok(Json(agent))
}

/// POST /agents/:id/approve - 批准 Agent
async fn approve_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state
        .agent_service
        .set_status(id, AgentStatus::Idle)
        .await?;
    Ok(Json(agent))
}

/// POST /agents/:id/terminate - 终止 Agent
async fn terminate_agent(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;

    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentDelete { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:delete permission".to_string(),
        ));
    }

    let terminated = state.agent_service.terminate(id).await?;
    Ok(Json(terminated))
}

/// POST /agents/:id/wakeup - 唤醒 Agent
async fn wakeup_agent(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<AgentWakeupRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    let issue_id: Uuid = sqlx::query_scalar("SELECT id FROM issues WHERE company_id=$1 AND assignee_agent_id=$2 AND status IN ('todo','in_progress') ORDER BY updated_at DESC LIMIT 1")
        .bind(agent.company_id).bind(id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::BadRequest("Agent has no assigned executable issue".to_string()))?;
    let request = body.map(|Json(value)| value).unwrap_or_default();
    let mut context_snapshot = request
        .context_snapshot
        .unwrap_or_else(|| json!({}));
    if let Some(object) = context_snapshot.as_object_mut() {
        object.insert("triggeredBy".to_string(), json!("agent_wakeup_route"));
        if request.force_fresh_session == Some(true) {
            object.insert("forceFreshSession".to_string(), json!(true));
        }
    }
    state
        .heartbeat_service
        .wakeup_with_options(
            id,
            issue_id,
            agent.company_id,
            HeartbeatWakeupOptions {
                source: Some(request.source.unwrap_or_else(|| "on_demand".to_string())),
                trigger_detail: Some(
                    request
                        .trigger_detail
                        .unwrap_or_else(|| "manual".to_string()),
                ),
                reason: Some(
                    request
                        .reason
                        .unwrap_or_else(|| "manual_agent_wakeup".to_string()),
                ),
                payload: request.payload,
                idempotency_key: request.idempotency_key,
                context_snapshot: Some(context_snapshot),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    // Paperclip returns the queued/running HeartbeatRun (rather than the
    // agent projection).  The run is inserted before the executor is spawned,
    // so this query is available even when the adapter starts immediately.
    let run = sqlx::query(
        "SELECT id, company_id, agent_id, invocation_source, status::text AS status,
                responsible_user_id, started_at, finished_at, error, exit_code,
                context_snapshot, output, result_json, created_at, updated_at
         FROM heartbeat_runs
         WHERE company_id = $1 AND agent_id = $2
           AND context_snapshot->>'issueId' = $3
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(agent.company_id)
    .bind(id)
    .bind(issue_id.to_string())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    if let Some(row) = run {
        use sqlx::Row;
        return Ok(Json(json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "agentId": row.get::<Uuid, _>("agent_id"),
            "invocationSource": row.get::<String, _>("invocation_source"),
            "status": row.get::<String, _>("status"),
            "responsibleUserId": row.get::<Option<String>, _>("responsible_user_id"),
            "startedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
            "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
            "error": row.get::<Option<String>, _>("error"),
            "exitCode": row.get::<Option<i32>, _>("exit_code"),
            "contextSnapshot": row.get::<Option<Value>, _>("context_snapshot"),
            "output": row.get::<Option<String>, _>("output"),
            "resultJson": row.get::<Option<Value>, _>("result_json"),
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        })));
    }

    // A budget gate can legitimately skip a wake without creating a run.
    Ok(Json(json!({ "status": "skipped", "agentId": id })))
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
        .ok_or_else(|| AppError::BadRequest("Missing 'budgetMonthlyCents' field".to_string()))?
        as i32;
    let agent = state
        .agent_service
        .update_budget(id, budget_monthly_cents)
        .await?;
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
    let evaluated = state
        .watchdog_service
        .evaluate_all(agent.company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    // A direct invoke is still a real execution request. With no issue id the
    // coordinator uses the agent's latest assigned issue, if one exists.
    let issue_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM issues WHERE company_id=$1 AND assignee_agent_id=$2 AND status IN ('todo','in_progress') ORDER BY updated_at DESC LIMIT 1")
        .bind(agent.company_id).bind(id).fetch_optional(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    if let Some(issue_id) = issue_id {
        state
            .heartbeat_service
            .wakeup_with_options(
                id,
                issue_id,
                agent.company_id,
                HeartbeatWakeupOptions {
                    source: Some("on_demand".to_string()),
                    trigger_detail: Some("manual".to_string()),
                    reason: Some("manual_agent_invoke".to_string()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    }
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


/// A15: POST /agents/:id/interrupt
async fn interrupt_agent(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden(
            "Insufficient permissions: Missing agent:update permission".to_string(),
        ));
    }

    let runs: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, (context_snapshot->>'issueId')::uuid
         FROM heartbeat_runs
         WHERE company_id = $1
           AND agent_id = $2
           AND status IN ('queued','running','scheduled_retry')
           AND (context_snapshot->>'issueId') IS NOT NULL
         ORDER BY created_at ASC",
    )
    .bind(agent.company_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?;

    let mut interrupted = Vec::new();
    for (run_id, issue_id) in runs {
        state
            .heartbeat_service
            .cancel_run(id, issue_id, agent.company_id, "Interrupted by operator")
            .await
            .map_err(|error| AppError::InternalServerError(error.to_string()))?;
        interrupted.push(run_id);
    }
    if !interrupted.is_empty() && agent.status == AgentStatus::Running {
        let _ = state.agent_service.set_status(id, AgentStatus::Idle).await?;
    }
    Ok(Json(json!({
        "agentId": id,
        "interruptedRunIds": interrupted,
        "interruptedCount": interrupted.len(),
    })))
}

/// A16: POST /agents/:id/reset-credentials
async fn reset_agent_credentials(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:update permission".into()));
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    let revoked = sqlx::query(
        "UPDATE agent_api_keys
         SET revoked_at = COALESCE(revoked_at, NOW()), updated_at = NOW()
         WHERE agent_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?
    .rows_affected();
    sqlx::query(
        "UPDATE agent_runtime_states
         SET session_id = NULL, session_display_id = NULL,
             session_params_json = NULL, updated_at = NOW()
         WHERE agent_id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    tx.commit()
        .await
        .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    Ok(Json(json!({
        "agentId": id,
        "credentialsReset": true,
        "revokedKeyCount": revoked,
        "sessionReset": true,
    })))
}

/// A17: GET /agents/:id/credentials
async fn get_agent_credentials(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:read permission".into()));
    }
    let keys = state.agent_service.list_keys(id).await?;
    Ok(Json(json!({
        "agentId": id,
        "credentials": keys.into_iter().map(|key| json!({
            "id": key.id,
            "name": key.name,
            "scope": key.scope,
            "lastUsedAt": key.last_used_at,
            "revokedAt": key.revoked_at,
            "createdAt": key.created_at,
            "active": key.is_active(),
        })).collect::<Vec<_>>(),
    })))
}

/// A18: GET /agents/:id/metrics
async fn get_agent_metrics(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:read permission".into()));
    }
    let row = sqlx::query(
        "SELECT COUNT(*)::bigint AS total_runs,
                COUNT(*) FILTER (WHERE status = 'succeeded')::bigint AS succeeded_runs,
                COUNT(*) FILTER (WHERE status IN ('queued','running','scheduled_retry'))::bigint AS active_runs,
                COALESCE(AVG(EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000)
                    FILTER (WHERE started_at IS NOT NULL AND finished_at IS NOT NULL), 0)::double precision AS avg_response_time_ms
         FROM heartbeat_runs
         WHERE company_id = $1 AND agent_id = $2",
    )
    .bind(agent.company_id)
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    use sqlx::Row;
    let total_runs: i64 = row.get("total_runs");
    let succeeded_runs: i64 = row.get("succeeded_runs");
    Ok(Json(json!({
        "agentId": id,
        "totalRuns": total_runs,
        "succeededRuns": succeeded_runs,
        "activeRuns": row.get::<i64, _>("active_runs"),
        "successRate": if total_runs == 0 { 0.0 } else { succeeded_runs as f64 / total_runs as f64 },
        "avgResponseTime": row.get::<f64, _>("avg_response_time_ms"),
    })))
}

/// A19: GET /agents/:id/activity
async fn get_agent_activity(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:read permission".into()));
    }
    let rows = sqlx::query(
        "SELECT id, event_type, actor_type, actor_id, resource_type, resource_id,
                metadata, run_id, created_at
         FROM activity_logs
         WHERE company_id = $1
           AND (agent_id = $2 OR (resource_type = 'agent' AND resource_id = $2))
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(agent.company_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    use sqlx::Row;
    Ok(Json(rows.into_iter().map(|row| json!({
        "id": row.get::<Uuid, _>("id"),
        "eventType": row.get::<String, _>("event_type"),
        "actorType": row.get::<String, _>("actor_type"),
        "actorId": row.get::<Uuid, _>("actor_id"),
        "resourceType": row.get::<String, _>("resource_type"),
        "resourceId": row.get::<Uuid, _>("resource_id"),
        "metadata": row.get::<serde_json::Value, _>("metadata"),
        "runId": row.get::<Option<Uuid>, _>("run_id"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
}

/// A20: POST /agents/:id/permissions
async fn add_agent_permission(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:update permission".into()));
    }
    let permissions_value = payload.get("permissions").cloned().unwrap_or(payload);
    let permissions = serde_json::from_value::<models::AgentPermissions>(permissions_value)
        .map_err(|error| AppError::BadRequest(format!("Invalid agent permissions: {error}")))?;
    let updated = state.agent_service.update_permissions(id, permissions).await?;
    Ok(Json(updated))
}

/// A21: DELETE /agents/:id/permissions/:permission_id
async fn delete_agent_permission(
    State(_state): State<AppState>,
    Path((_id, _permission_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

/// A22: DELETE /agents/:id/skills/:skill_id
async fn delete_agent_skill(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((id, skill_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:update permission".into()));
    }
    state.agent_service.remove_skill(id, &skill_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A23: GET /agents/:id/sessions/:session_id
async fn get_agent_session(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((id, session_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:read permission".into()));
    }
    let row = sqlx::query(
        "SELECT id, adapter_type, task_key, session_display_id, session_params_json,
                last_run_id, last_error, created_at, updated_at
         FROM agent_task_sessions
         WHERE agent_id = $1 AND id = $2",
    )
    .bind(id)
    .bind(session_id.to_string())
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?
    .ok_or_else(|| AppError::NotFound(format!("Agent session not found: {session_id}")))?;
    use sqlx::Row;
    Ok(Json(json!({
        "agentId": id,
        "sessionId": row.get::<Uuid, _>("id"),
        "adapterType": row.get::<String, _>("adapter_type"),
        "taskKey": row.get::<String, _>("task_key"),
        "sessionDisplayId": row.get::<Option<String>, _>("session_display_id"),
        "sessionParams": row.get::<Option<serde_json::Value>, _>("session_params_json"),
        "state": json!({}),
        "lastRunId": row.get::<Option<Uuid>, _>("last_run_id"),
        "lastRunStatus": json!(null),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })))
}

/// A24: DELETE /agents/:id/sessions/:session_id
async fn delete_agent_session(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((id, session_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentUpdate { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:update permission".into()));
    }
    let result = sqlx::query(
        "DELETE FROM agent_task_sessions WHERE agent_id = $1 AND id = $2",
    )
    .bind(id)
    .bind(session_id)
    .execute(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("Agent session not found: {session_id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A25: GET /agents/:id/runs/:run_id
async fn get_agent_run(
    State(state): State<AppState>,
    Extension(auth_actor): Extension<AuthorizationActor>,
    Path((id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agent = state.agent_service.get_by_id(id).await?;
    if !services::auth::decision_engine::decide_access(
        &state.pool,
        &auth_actor,
        &AuthorizationAction::AgentRead { agent_id: id },
        Some(agent.company_id),
    )
    .await
    {
        return Err(AppError::Forbidden("Insufficient permissions: Missing agent:read permission".into()));
    }
    let row = sqlx::query(
        "SELECT id, company_id, agent_id, invocation_source, status::text AS status,
                responsible_user_id, started_at, finished_at, error, exit_code,
                context_snapshot, output, result_json, scheduled_retry_at,
                scheduled_retry_attempt, scheduled_retry_reason, created_at, updated_at
         FROM heartbeat_runs
         WHERE id = $1 AND agent_id = $2 AND company_id = $3",
    )
    .bind(run_id)
    .bind(id)
    .bind(agent.company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| AppError::InternalServerError(error.to_string()))?
    .ok_or_else(|| AppError::NotFound(format!("Agent run not found: {run_id}")))?;
    use sqlx::Row;
    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "agentId": row.get::<Uuid, _>("agent_id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "invocationSource": row.get::<String, _>("invocation_source"),
        "status": row.get::<String, _>("status"),
        "responsibleUserId": row.get::<Option<String>, _>("responsible_user_id"),
        "startedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
        "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
        "error": row.get::<Option<String>, _>("error"),
        "exitCode": row.get::<Option<i32>, _>("exit_code"),
        "contextSnapshot": row.get::<Option<serde_json::Value>, _>("context_snapshot"),
        "output": row.get::<Option<String>, _>("output"),
        "resultJson": row.get::<Option<serde_json::Value>, _>("result_json"),
        "scheduledRetryAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("scheduled_retry_at"),
        "scheduledRetryAttempt": row.get::<Option<i32>, _>("scheduled_retry_attempt"),
        "scheduledRetryReason": row.get::<Option<String>, _>("scheduled_retry_reason"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })))
}
