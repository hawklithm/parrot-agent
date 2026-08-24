//! Tool Access 补齐域 —— 对齐 Paperclip `routes/tool-access.ts` 中 Parrot 缺失的
//! company tools 子资源、tool-connections CRUD/子端点、tool-profiles、tool-applications。
//! gallery/examples/trust-rules/stdio-templates/runtime-health 以静态/聚合语义实现。

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

// ---------- companies/:cid/tools/* 只读聚合 ----------

/// GET /companies/:cid/tools/gallery —— 内置工具目录（静态，与 tools.rs 内置集一致）。
async fn tools_gallery(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![
        json!({ "name": "code-review", "description": "Perform automated code review", "category": "engineering" }),
        json!({ "name": "test-generation", "description": "Generate unit tests from code", "category": "engineering" }),
        json!({ "name": "documentation", "description": "Auto-generate API documentation", "category": "engineering" }),
        json!({ "name": "refactoring", "description": "Suggest and apply refactoring patterns", "category": "engineering" }),
    ]))
}

/// GET /companies/:cid/tools/examples —— 内置示例（静态）。
async fn tools_examples(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![
        json!({ "id": "review-pr", "title": "Review a pull request", "prompt": "Review the changes in this PR for correctness and security" }),
        json!({ "id": "fix-bug", "title": "Investigate a failing test", "prompt": "Investigate the failing test and propose a fix" }),
    ]))
}

/// GET /companies/:cid/tools/apps/attention —— attention 应用（静态空集，后续接入）。
async fn tools_attention_apps(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /companies/:cid/tools/action-requests —— 从 tool_action_requests 列表。
async fn tools_action_requests(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, agent_id, action, tool_name, status, created_at \
         FROM tool_action_requests WHERE company_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool action requests: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Option<Uuid>, _>("agent_id"),
                    "action": r.get::<String, _>("action"),
                    "toolName": r.get::<String, _>("tool_name"),
                    "status": r.get::<String, _>("status"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// GET /companies/:cid/tools/applications —— tool_applications 列表。
async fn tools_applications(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT * FROM tool_applications WHERE company_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool applications: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Option<Uuid>, _>("agent_id"),
                    "connectionId": r.get::<Option<Uuid>, _>("connection_id"),
                    "status": r.get::<String, _>("status"),
                    "justification": r.get::<Option<String>, _>("justification"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// POST /companies/:cid/tools/applications —— 创建申请。
#[derive(Debug, Deserialize)]
struct CreateToolApplicationRequest {
    #[serde(rename = "connectionId")]
    connection_id: Uuid,
    justification: Option<String>,
}
async fn create_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateToolApplicationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => *agent_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tool_applications (id, company_id, agent_id, connection_id, justification) \
         VALUES ($1,$2,$3,$4,$5) ON CONFLICT (agent_id, connection_id) DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(company_id)
    .bind(agent_id)
    .bind(request.connection_id)
    .bind(request.justification.as_deref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.created",
        &actor,
        "tool_application",
        id,
        json!({}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "status": "pending" })),
    ))
}

/// GET /companies/:cid/tools/profiles —— tool_profiles 列表。
async fn tools_profiles(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows =
        sqlx::query("SELECT * FROM tool_profiles WHERE company_id = $1 ORDER BY created_at ASC")
            .bind(company_id)
            .fetch_all(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list tool profiles: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "name": r.get::<String, _>("name"),
                    "description": r.get::<Option<String>, _>("description"),
                    "status": r.get::<Option<String>, _>("status"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// GET /companies/:cid/tools/runtime-health —— 连接健康聚合。
async fn tools_runtime_health(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT status, COUNT(*) AS cnt FROM tool_connections WHERE company_id = $1 GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to aggregate tool health: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut by_status = serde_json::Map::new();
    for r in &rows {
        by_status.insert(r.get::<String, _>("status"), json!(r.get::<i64, _>("cnt")));
    }
    Ok(Json(
        json!({ "connectionsByStatus": by_status, "healthy": true }),
    ))
}

/// GET /companies/:cid/tools/runtime-slots —— 当前工具运行时槽位。
async fn tools_runtime_slots(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, connection_id, slot_key, runtime_kind, status, health_status, process_id, last_error, last_used_at, updated_at FROM tool_runtime_slots WHERE company_id = $1 ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool runtime slots: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(|row| json!({
        "id": row.get::<Uuid, _>("id"),
        "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
        "slotKey": row.get::<String, _>("slot_key"),
        "runtimeKind": row.get::<String, _>("runtime_kind"),
        "status": row.get::<String, _>("status"),
        "healthStatus": row.get::<String, _>("health_status"),
        "processId": row.get::<Option<i32>, _>("process_id"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "lastUsedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })).collect()))
}

/// GET /companies/:cid/tools/trust-rules —— 已持久化的工具策略。
async fn tools_trust_rules(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, description, policy_type, priority, enabled, selectors, conditions, config, created_at, updated_at
         FROM tool_policies WHERE company_id = $1 ORDER BY priority DESC, created_at ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool policies: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "description": row.get::<Option<String>, _>("description"),
                    "scope": row.get::<String, _>("policy_type"),
                    "priority": row.get::<i32, _>("priority"),
                    "enabled": row.get::<bool, _>("enabled"),
                    "selectors": row.get::<Value, _>("selectors"),
                    "conditions": row.get::<Option<Value>, _>("conditions"),
                    "config": row.get::<Option<Value>, _>("config"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

/// GET /companies/:cid/tools/stdio-templates —— 已登记的 stdio 模板。
async fn tools_stdio_templates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, template_key, name, description, status, command, args, env_keys, tools, created_at, updated_at
         FROM tool_stdio_command_templates WHERE company_id = $1 ORDER BY name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list stdio templates: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "templateKey": row.get::<String, _>("template_key"),
                    "name": row.get::<String, _>("name"),
                    "description": row.get::<Option<String>, _>("description"),
                    "status": row.get::<String, _>("status"),
                    "command": row.get::<String, _>("command"),
                    "args": row.get::<Value, _>("args"),
                    "envKeys": row.get::<Value, _>("env_keys"),
                    "tools": row.get::<Value, _>("tools"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

// ---------- tool-applications ----------

/// PATCH /api/tool-applications/:id —— 更新状态（board 审批）。
#[derive(Debug, Deserialize)]
struct UpdateToolApplicationRequest {
    status: Option<String>,
}
async fn update_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
    Json(request): Json<UpdateToolApplicationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_applications SET status = COALESCE($2, status), reviewed_by_user_id = $3, \
         reviewed_at = NOW(), updated_at = NOW() WHERE id = $1 RETURNING company_id, *",
    )
    .bind(application_id)
    .bind(request.status.as_deref())
    .bind(user_id.to_string())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.updated",
        &actor,
        "tool_application",
        application_id,
        json!({ "status": request.status }),
    )
    .await;
    Ok(Json(
        json!({ "id": application_id, "status": request.status }),
    ))
}

/// DELETE /api/tool-applications/:id
async fn delete_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let row = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT company_id FROM tool_applications WHERE id = $1",
    )
    .bind(application_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(company_id) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM tool_applications WHERE id = $1")
        .bind(application_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- tool-connections ----------

fn connection_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "toolType": row.get::<String, _>("tool_type"),
        "name": row.get::<String, _>("name"),
        "status": row.get::<String, _>("status"),
        "config": row.get::<Option<Value>, _>("config"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

async fn get_connection_by_id(
    state: &AppState,
    company_id: Uuid,
    connection_id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, StatusCode> {
    sqlx::query("SELECT * FROM tool_connections WHERE id = $1 AND company_id = $2")
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load tool connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/tool-connections/:id
async fn get_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = get_connection_by_id(&state, actor_company(&actor)?, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(connection_json(&row)))
}

fn actor_company(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { company_id, .. }
        | AuthorizationActor::Agent { company_id, .. } => Ok(*company_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

/// PATCH /api/tool-connections/:id —— 更新配置。
#[derive(Debug, Deserialize)]
struct UpdateToolConnectionRequest {
    name: Option<String>,
    config: Option<Value>,
}
async fn update_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<UpdateToolConnectionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "UPDATE tool_connections SET name = COALESCE($3, name), config = COALESCE($4, config), \
         updated_at = NOW() WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(request.name.as_deref())
    .bind(request.config.as_ref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.updated",
        &actor,
        "tool_connection",
        connection_id,
        json!({}),
    )
    .await;
    Ok(Json(connection_json(&row)))
}

/// DELETE /api/tool-connections/:id
async fn delete_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM tool_connections WHERE id = $1 AND company_id = $2")
        .bind(connection_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.deleted",
        &actor,
        "tool_connection",
        connection_id,
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/grants
async fn list_connection_grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT * FROM tool_connection_grants WHERE connection_id = $1 ORDER BY created_at DESC",
    )
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list grants: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Uuid, _>("agent_id"),
                    "grantType": r.get::<String, _>("grant_type"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// DELETE /api/tool-connections/:id/grants/:grant_id
async fn delete_connection_grant(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((connection_id, grant_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM tool_connection_grants WHERE id = $1 AND connection_id = $2")
        .bind(grant_id)
        .bind(connection_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete grant: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/usage —— 聚合 tool_invocations。
async fn connection_usage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tool_invocations WHERE connection_id = $1")
            .bind(connection_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    Ok(Json(json!({ "totalInvocations": total })))
}

/// GET /api/tool-connections/:id/installs —— 连接安装记录。
async fn connection_installs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, target_type, target_id, created_by_user_id, created_at FROM tool_connection_installs WHERE connection_id = $1 ORDER BY created_at DESC",
    )
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection installs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "targetType": row.get::<String, _>("target_type"),
                    "targetId": row.get::<String, _>("target_id"),
                    "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-connections/:id/catalog —— MCP 发现得到的工具目录。
async fn connection_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, tool_name, title, description, input_schema, output_schema, risk_level, status, last_seen_at FROM tool_catalog_entries WHERE connection_id = $1 ORDER BY name ASC",
    )
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection catalog: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "toolName": row.get::<String, _>("tool_name"),
                    "title": row.get::<Option<String>, _>("title"),
                    "description": row.get::<Option<String>, _>("description"),
                    "inputSchema": row.get::<Value, _>("input_schema"),
                    "outputSchema": row.get::<Option<Value>, _>("output_schema"),
                    "riskLevel": row.get::<String, _>("risk_level"),
                    "status": row.get::<String, _>("status"),
                    "lastSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("last_seen_at"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-connections/:id/activity —— 最近调用事件。
async fn connection_activity(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, tool_name, status, occurred_at FROM tool_call_events \
         WHERE connection_id = $1 ORDER BY occurred_at DESC LIMIT 50",
    )
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection activity: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "toolName": r.get::<String, _>("tool_name"),
                    "status": r.get::<String, _>("status"),
                    "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("occurred_at"),
                })
            })
            .collect(),
    ))
}

// ---------- tool-profiles ----------

/// GET /api/tool-profiles/:id/new-tools —— profile 尚未审核的目录工具。
async fn profile_new_tools(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT c.id, c.connection_id, c.name, c.tool_name, c.title, c.description, c.input_schema,
                c.output_schema, c.risk_level, c.status, c.first_seen_at, c.last_seen_at
           FROM tool_catalog_entries c
           JOIN tool_profiles p ON p.id = $1 AND p.company_id = c.company_id
          WHERE c.company_id = $2 AND c.reviewed_at IS NULL AND c.status = 'active'
          ORDER BY c.first_seen_at DESC",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list profile new tools: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "connectionId": row.get::<Uuid, _>("connection_id"),
                    "name": row.get::<String, _>("name"),
                    "toolName": row.get::<String, _>("tool_name"),
                    "title": row.get::<Option<String>, _>("title"),
                    "description": row.get::<Option<String>, _>("description"),
                    "inputSchema": row.get::<Value, _>("input_schema"),
                    "outputSchema": row.get::<Option<Value>, _>("output_schema"),
                    "riskLevel": row.get::<String, _>("risk_level"),
                    "status": row.get::<String, _>("status"),
                    "firstSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("first_seen_at"),
                    "lastSeenAt": row.get::<chrono::DateTime<chrono::Utc>, _>("last_seen_at"),
                })
            })
            .collect(),
    ))
}

/// DELETE /api/tool-profiles/:id
async fn delete_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM tool_profiles WHERE id = $1 AND company_id = $2")
        .bind(profile_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/tool-profiles/:id
#[derive(Debug, Deserialize)]
struct UpdateToolProfileRequest {
    name: Option<String>,
    description: Option<String>,
}
async fn update_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<UpdateToolProfileRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_profiles SET name = COALESCE($3, name), description = COALESCE($4, description), \
         updated_at = NOW() WHERE id = $1 AND company_id = $2 RETURNING *",
    )
    .bind(profile_id)
    .bind(company_id)
    .bind(request.name.as_deref())
    .bind(request.description.as_deref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool profile: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(json!({
        "id": profile_id,
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
    })))
}

// ---------- tool-gateway ----------

/// GET /api/tool-gateway/audit —— 从 tool_invocations 聚合。
async fn gateway_audit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, agent_id, tool_name, status, created_at \
         FROM tool_invocations WHERE company_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list gateway audit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Option<Uuid>, _>("agent_id"),
                    "toolName": r.get::<String, _>("tool_name"),
                    "status": r.get::<String, _>("status"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-gateway/runtime-slots —— 网关运行时槽位。
async fn gateway_runtime_slots(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, connection_id, slot_key, runtime_kind, status, health_status, process_id, last_error, last_used_at, updated_at
           FROM tool_runtime_slots WHERE company_id = $1 ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list gateway runtime slots: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "connectionId": row.get::<Option<Uuid>, _>("connection_id"),
                    "slotKey": row.get::<String, _>("slot_key"),
                    "runtimeKind": row.get::<String, _>("runtime_kind"),
                    "status": row.get::<String, _>("status"),
                    "healthStatus": row.get::<String, _>("health_status"),
                    "processId": row.get::<Option<i32>, _>("process_id"),
                    "lastError": row.get::<Option<String>, _>("last_error"),
                    "lastUsedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

pub fn tool_access_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/tools/gallery", get(tools_gallery))
        .route("/companies/:company_id/tools/examples", get(tools_examples))
        .route(
            "/companies/:company_id/tools/apps/attention",
            get(tools_attention_apps),
        )
        .route(
            "/companies/:company_id/tools/action-requests",
            get(tools_action_requests),
        )
        .route(
            "/companies/:company_id/tools/applications",
            get(tools_applications).post(create_tool_application),
        )
        .route("/companies/:company_id/tools/profiles", get(tools_profiles))
        .route(
            "/companies/:company_id/tools/runtime-health",
            get(tools_runtime_health),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots",
            get(tools_runtime_slots),
        )
        .route(
            "/companies/:company_id/tools/trust-rules",
            get(tools_trust_rules),
        )
        .route(
            "/companies/:company_id/tools/stdio-templates",
            get(tools_stdio_templates),
        )
        .route(
            "/tool-applications/:id",
            patch(update_tool_application).delete(delete_tool_application),
        )
        .route(
            "/tool-connections/:id",
            get(get_tool_connection)
                .patch(update_tool_connection)
                .delete(delete_tool_connection),
        )
        .route("/tool-connections/:id/grants", get(list_connection_grants))
        .route(
            "/tool-connections/:id/grants/:grant_id",
            delete(delete_connection_grant),
        )
        .route("/tool-connections/:id/usage", get(connection_usage))
        .route("/tool-connections/:id/installs", get(connection_installs))
        .route("/tool-connections/:id/catalog", get(connection_catalog))
        .route("/tool-connections/:id/activity", get(connection_activity))
        .route("/tool-profiles/:id/new-tools", get(profile_new_tools))
        .route(
            "/tool-profiles/:id",
            patch(update_tool_profile).delete(delete_tool_profile),
        )
        .route("/tool-gateway/audit", get(gateway_audit))
        .route("/tool-gateway/runtime-slots", get(gateway_runtime_slots))
        // ---- round 2 ----
        .route(
            "/tool-profile-entries/:id",
            patch(update_tool_profile_entry).delete(delete_tool_profile_entry),
        )
        .route(
            "/tool-connections/:id/test-agents",
            get(connection_test_agents),
        )
        .route(
            "/tool-connections/:id/test-calls/:call_id",
            get(connection_test_call),
        )
        .route(
            "/tool-connections/:id/catalog/refresh",
            post(refresh_connection_catalog),
        )
        .route(
            "/tool-connections/:id/grants/installations",
            post(install_connection_grants),
        )
        .route("/tools/oauth/callback", get(tools_oauth_callback))
        .route(
            "/companies/:company_id/tools/connections",
            post(create_company_tool_connection),
        )
        .route(
            "/companies/:company_id/tools/profiles",
            post(create_company_tool_profile),
        )
        .route(
            "/companies/:company_id/tools/examples/:example_id/install",
            post(install_tool_example),
        )
        .route(
            "/companies/:company_id/tools/examples/:example_id/smoke",
            post(smoke_tool_example),
        )
        .route(
            "/companies/:company_id/tools/apps/connect",
            post(connect_tool_app),
        )
        .route(
            "/companies/:company_id/tools/apps/:app_id/finish",
            post(finish_tool_app),
        )
        .route(
            "/companies/:company_id/tools/mcp/import-json",
            post(import_mcp_json),
        )
        .route(
            "/companies/:company_id/tools/policies/:policy_id",
            patch(update_company_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/policies/:policy_id/duplicate",
            post(duplicate_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/policies/reorder",
            post(reorder_tool_policies),
        )
        .route(
            "/companies/:company_id/tools/policy/test",
            post(test_tool_policy),
        )
        .route(
            "/companies/:company_id/tools/stdio-templates",
            post(create_stdio_template),
        )
        .route(
            "/companies/:company_id/tools/trust-rules/:rule_id/revoke",
            post(revoke_trust_rule),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots/:slot_id/stop",
            post(stop_runtime_slot),
        )
        .route(
            "/companies/:company_id/tools/runtime-slots/:slot_id/restart",
            post(restart_runtime_slot),
        )
        .route(
            "/agents/me/connections/:connection_id/start-authorization",
            post(start_agent_connection_auth),
        )
        .route(
            "/agents/me/connections/:connection_id/token",
            post(agent_connection_token),
        )
        // ---- round 3 ----
        .route(
            "/tool-connections/:id/health-check",
            post(connection_health_check),
        )
        .route(
            "/tool-connections/:id/test-calls",
            post(create_connection_test_call),
        )
        .route("/tool-profiles/:id/duplicate", post(duplicate_tool_profile))
        .route(
            "/tool-profiles/:id/entries",
            post(create_tool_profile_entry),
        )
        .route(
            "/tool-profiles/:id/new-tools/review",
            post(review_profile_new_tools),
        )
        .route("/tools/oauth/:provider/start", post(tools_oauth_start))
        .route(
            "/tool-gateway/runtime-slots/:slot_id/stop",
            post(gateway_slot_stop),
        )
        .route(
            "/tool-gateway/runtime-slots/:slot_id/restart",
            post(gateway_slot_restart),
        )
}

// ================= Round 2 =================

/// PATCH /api/tool-profile-entries/:id
#[derive(Debug, Deserialize)]
struct UpdateProfileEntryRequest {
    enabled: Option<bool>,
    order: Option<i32>,
}
async fn update_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(entry_id): Path<Uuid>,
    Json(request): Json<UpdateProfileEntryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_profile_entries SET effect = CASE WHEN COALESCE($2, effect = 'include') THEN 'include' ELSE 'exclude' END,
         updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(entry_id)
    .bind(request.enabled)
    .bind(request.enabled)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool profile entry: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(json!({
        "id": entry_id,
        "enabled": row.get::<String, _>("effect") == "include",
    })))
}

/// DELETE /api/tool-profile-entries/:id
async fn delete_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(entry_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM tool_profile_entries WHERE id = $1")
        .bind(entry_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile entry: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/test-agents —— 当前公司可用于测试的 agent。
async fn connection_test_agents(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let exists = sqlx::query("SELECT id FROM tool_connections WHERE id = $1 AND company_id = $2")
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let rows = sqlx::query(
        "SELECT id, name, role, status FROM agents WHERE company_id = $1 AND status <> 'terminated' ORDER BY name ASC",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection test agents: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "name": row.get::<String, _>("name"),
                    "role": row.get::<String, _>("role"),
                    "status": row.get::<String, _>("status"),
                })
            })
            .collect(),
    ))
}

/// GET /api/tool-connections/:id/test-calls/:call_id —— 静态空结构。
async fn connection_test_call(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((connection_id, call_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "connectionId": connection_id, "callId": call_id, "status": "noop" }),
    ))
}

/// POST /api/tool-connections/:id/catalog/refresh —— 真实调用 MCP tools/list 并落库。
async fn refresh_connection_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let connection = sqlx::query(
        "SELECT transport, transport_config FROM tool_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let transport: String = connection.get("transport");
    let config: Value = connection.get("transport_config");
    let result = if transport == "mcp_remote" {
        let url = config
            .get("url")
            .or_else(|| config.get("endpoint"))
            .and_then(Value::as_str)
            .ok_or(StatusCode::BAD_REQUEST)?;
        crate::routes::tools::mcp_http_request(url, "tools/list", json!({})).await
    } else {
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .ok_or(StatusCode::BAD_REQUEST)?;
        let args = config
            .get("args")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        crate::routes::tools::mcp_stdio_request(command, &args, "tools/list", json!({})).await
    }
    .map_err(|error| {
        tracing::warn!(connection_id = %connection_id, %error, "MCP catalog refresh failed");
        StatusCode::BAD_GATEWAY
    })?;
    let tools = result
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut refreshed = 0usize;
    for tool in tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .ok_or(StatusCode::BAD_GATEWAY)?;
        let entry_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tool_catalog_entries
                (id, company_id, connection_id, name, tool_name, title, description, input_schema, output_schema, version_hash, last_seen_at, updated_at)
             VALUES ($1,$2,$3,$4,$4,$5,$6,$7,$8,$9,NOW(),NOW())
             ON CONFLICT (connection_id, name) DO UPDATE SET
                title = EXCLUDED.title, description = EXCLUDED.description, input_schema = EXCLUDED.input_schema,
                output_schema = EXCLUDED.output_schema, status = 'active', last_seen_at = NOW(), updated_at = NOW()",
        )
        .bind(entry_id)
        .bind(company_id)
        .bind(connection_id)
        .bind(name)
        .bind(tool.get("title").and_then(Value::as_str))
        .bind(tool.get("description").and_then(Value::as_str))
        .bind(tool.get("inputSchema").cloned().unwrap_or_else(|| json!({})))
        .bind(tool.get("outputSchema").cloned())
        .bind(Uuid::new_v4().to_string())
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to upsert catalog entry: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        refreshed += 1;
    }
    sqlx::query("UPDATE tool_connections SET health_status = 'healthy', health_message = NULL, health_checked_at = NOW(), last_catalog_refresh_at = NOW(), last_healthy_at = NOW(), last_error = NULL, status = 'active', updated_at = NOW() WHERE id = $1 AND company_id = $2")
        .bind(connection_id).bind(company_id).execute(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        json!({ "connectionId": connection_id, "refreshed": true, "toolCount": refreshed }),
    ))
}

/// POST /api/tool-connections/:id/grants/installations —— 按安装批量授权（基础：204 语义）。
async fn install_connection_grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query(
        "INSERT INTO tool_connection_grants (company_id, connection_id, agent_id) \
         SELECT company_id, $1, id FROM agents WHERE company_id = $2 AND status = 'active' \
         ON CONFLICT (connection_id, agent_id) DO NOTHING",
    )
    .bind(connection_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to install connection grants: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tools/oauth/callback —— OAuth callback（返回 code 回显）。
async fn tools_oauth_callback(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "received": true, "status": "ok" })))
}

/// POST /companies/:cid/tools/connections —— 创建连接。
#[derive(Debug, Deserialize)]
struct CreateConnectionRequest {
    #[serde(rename = "toolType")]
    tool_type: String,
    name: Option<String>,
    config: Option<Value>,
}
async fn create_company_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tool_connections (id, company_id, tool_type, name, config, status) \
         VALUES ($1, $2, $3, COALESCE($4, $3), $5, 'unconfigured')",
    )
    .bind(id)
    .bind(company_id)
    .bind(&request.tool_type)
    .bind(request.name.as_deref())
    .bind(request.config.as_ref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.created",
        &actor,
        "tool_connection",
        id,
        json!({ "toolType": request.tool_type }),
    )
    .await;
    let row = sqlx::query("SELECT * FROM tool_connections WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(connection_json(&row))))
}

/// POST /companies/:cid/tools/profiles —— 创建 profile。
#[derive(Debug, Deserialize)]
struct CreateToolProfileRequest {
    name: String,
    description: Option<String>,
}
async fn create_company_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateToolProfileRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tool_profiles (id, company_id, name, description) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(company_id)
    .bind(&request.name)
    .bind(request.description.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool profile: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "name": request.name, "description": request.description })),
    ))
}

/// POST /companies/:cid/tools/examples/:example_id/install|smoke —— mock。
async fn install_tool_example(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _example_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "installed": true })))
}
async fn smoke_tool_example(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _example_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "smoke": "passed" })))
}

/// POST /companies/:cid/tools/apps/connect 与 /apps/:app_id/finish —— mock。
async fn connect_tool_app(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "connected": true })))
}
async fn finish_tool_app(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _app_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "finished": true })))
}

/// POST /companies/:cid/tools/mcp/import-json —— 解析 MCP 配置并返回可审阅草稿。
async fn import_mcp_json(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let raw = body
        .get("mcpJson")
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let parsed = match raw {
        Value::String(text) => {
            serde_json::from_str::<Value>(&text).map_err(|_| StatusCode::BAD_REQUEST)?
        }
        value => value,
    };
    let servers = parsed
        .get("mcpServers")
        .and_then(Value::as_object)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let drafts = servers
        .iter()
        .map(|(name, value)| {
            let server = value.as_object();
            let mut warnings = Vec::new();
            let (transport, config, credential_fields) = if let Some(server) = server {
                if let Some(url) = server
                    .get("url")
                    .or_else(|| server.get("endpoint"))
                    .and_then(Value::as_str)
                {
                    let headers = server
                        .get("headers")
                        .and_then(Value::as_object)
                        .map(|headers| {
                            headers
                                .keys()
                                .map(|key| {
                                    warnings.push(format!(
                                        "Header {key} will be stored as a Paperclip secret before activation."
                                    ));
                                    json!({
                                        "configPath": format!("headers.{key}"),
                                        "label": key,
                                        "placement": "header",
                                        "key": key,
                                        "prefix": Value::Null,
                                        "required": true
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    ("mcp_remote", json!({ "url": url }), headers)
                } else if let Some(command) = server.get("command").and_then(Value::as_str) {
                    warnings.push(
                        "Imported stdio commands stay draft-only unless mapped to an approved Paperclip template."
                            .to_string(),
                    );
                    (
                        "local_stdio",
                        json!({
                            "importedCommand": command,
                            "importedArgs": server.get("args").cloned().unwrap_or_else(|| json!([]))
                        }),
                        Vec::new(),
                    )
                } else {
                    warnings.push("Unsupported MCP server entry.".to_string());
                    ("mcp_remote", json!({}), Vec::new())
                }
            } else {
                warnings.push("Unsupported MCP server entry.".to_string());
                ("mcp_remote", json!({}), Vec::new())
            };

            json!({
                "name": name,
                "transport": transport,
                "status": "draft",
                "config": config,
                "credentialRefs": [],
                "credentialFields": credential_fields,
                "warnings": warnings
            })
        })
        .collect::<Vec<_>>();

    if drafts.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(json!({ "drafts": drafts })))
}

/// PATCH /companies/:cid/tools/policies/:policy_id —— 更新策略。
#[derive(Debug, Deserialize)]
struct UpdateToolPolicyRequest {
    name: Option<String>,
    enabled: Option<bool>,
}
async fn update_company_tool_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, policy_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateToolPolicyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query(
        "UPDATE tool_policies SET name = COALESCE($3, name), enabled = COALESCE($4, enabled), \
         updated_at = NOW() WHERE id = $1 AND company_id = $2",
    )
    .bind(policy_id)
    .bind(company_id)
    .bind(request.name.as_deref())
    .bind(request.enabled)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update tool policy: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        json!({ "id": policy_id, "name": request.name, "enabled": request.enabled }),
    ))
}

/// POST /companies/:cid/tools/policies/:policy_id/duplicate —— mock。
async fn duplicate_tool_policy(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, policy_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "id": Uuid::new_v4(), "duplicatedFrom": policy_id }),
    ))
}

/// POST /companies/:cid/tools/policies/reorder —— mock。
async fn reorder_tool_policies(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "reordered": true })))
}

/// POST /companies/:cid/tools/policy/test —— mock。
async fn test_tool_policy(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "matches": true })))
}

/// POST /companies/:cid/tools/stdio-templates —— mock。
#[derive(Debug, Deserialize)]
struct CreateStdioTemplateRequest {
    name: Option<String>,
}
async fn create_stdio_template(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateStdioTemplateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "id": Uuid::new_v4(), "name": request.name })))
}

/// POST /companies/:cid/tools/trust-rules/:rule_id/revoke —— mock（204）。
async fn revoke_trust_rule(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _rule_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /companies/:cid/tools/runtime-slots/:slot_id/stop|restart —— mock。
async fn stop_runtime_slot(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "stopped": true })))
}
async fn restart_runtime_slot(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "restarted": true })))
}

/// POST /agents/me/connections/:connection_id/start-authorization —— mock。
async fn start_agent_connection_auth(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "connectionId": connection_id, "authorizationUrl": format!("/api/tools/oauth/authorize?connection={}", connection_id) }),
    ))
}

/// POST /agents/me/connections/:connection_id/token —— mock token。
async fn agent_connection_token(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "connectionId": connection_id, "accessToken": format!("mock-{}", Uuid::new_v4()) }),
    ))
}

// ================= Round 3 =================

/// POST /api/tool-connections/:id/health-check —— 对 MCP 做 initialize/tools.list 探测。
async fn connection_health_check(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let started = std::time::Instant::now();
    let refreshed = refresh_connection_catalog(
        State(state.clone()),
        Extension(actor.clone()),
        Path(connection_id),
    )
    .await;
    match refreshed {
        Ok(Json(result)) => Ok(Json(json!({
            "connectionId": connection_id,
            "healthy": true,
            "latencyMs": started.elapsed().as_millis(),
            "catalog": result,
        }))),
        Err(status) => {
            let _ = sqlx::query("UPDATE tool_connections SET health_status = 'unhealthy', health_message = $3, health_checked_at = NOW(), last_error = $3, updated_at = NOW() WHERE id = $1 AND company_id = $2")
                .bind(connection_id).bind(company_id).bind(format!("MCP health check failed ({status})"))
                .execute(&state.pool).await;
            Err(status)
        }
    }
}

/// POST /api/tool-connections/:id/test-calls —— mock。
#[derive(Debug, Deserialize)]
struct CreateTestCallRequest {
    tool: Option<String>,
}
async fn create_connection_test_call(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<CreateTestCallRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({ "id": Uuid::new_v4(), "connectionId": connection_id, "tool": request.tool, "status": "passed" }),
        ),
    ))
}

/// POST /api/tool-profiles/:id/duplicate —— mock。
async fn duplicate_tool_profile(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "id": Uuid::new_v4(), "duplicatedFrom": profile_id }),
    ))
}

/// POST /api/tool-profiles/:id/entries —— 添加 profile entry。
#[derive(Debug, Deserialize)]
struct CreateProfileEntryRequest {
    tool: String,
    enabled: Option<bool>,
}
async fn create_tool_profile_entry(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
    Json(request): Json<CreateProfileEntryRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tool_profile_entries (id, company_id, profile_id, selector_type, effect, tool_name) \
         VALUES ($1, $2, $3, 'tool_name', CASE WHEN COALESCE($4, true) THEN 'include' ELSE 'exclude' END, $5)",
    )
    .bind(id)
    .bind(company_id)
    .bind(profile_id)
    .bind(request.enabled)
    .bind(&request.tool)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool profile entry: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "tool": request.tool, "enabled": request.enabled })),
    ))
}

/// POST /api/tool-profiles/:id/new-tools/review —— mock。
async fn review_profile_new_tools(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "profileId": profile_id, "newTools": [], "reviewed": true }),
    ))
}

/// POST /api/tools/oauth/:provider/start —— mock。
async fn tools_oauth_start(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(
        json!({ "provider": provider, "authorizationUrl": format!("/api/tools/oauth/{}", provider) }),
    ))
}

/// POST /api/tool-gateway/runtime-slots/:slot_id/stop
/// 真实更新 tool_runtime_slots 行状态（数据层与 Paperclip 一致）。
/// 注：实际进程启停（spawn/kill）属运行时组件，不在本迁移范围；此处仅对齐状态机。
async fn gateway_slot_stop(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(slot_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let slot_id: Uuid = slot_id.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    sqlx::query(
        "UPDATE tool_runtime_slots \
         SET status = 'stopped', stopped_at = NOW(), process_id = NULL, last_error = NULL, updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(slot_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "stopped": true })))
}
/// POST /api/tool-gateway/runtime-slots/:slot_id/restart
/// 真实更新 tool_runtime_slots 行状态（数据层与 Paperclip 一致）。
async fn gateway_slot_restart(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(slot_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let slot_id: Uuid = slot_id.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    sqlx::query(
        "UPDATE tool_runtime_slots \
         SET status = 'running', started_at = NOW(), stopped_at = NULL, updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(slot_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "restarted": true })))
}
