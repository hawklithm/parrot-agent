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
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "agentId": r.get::<Option<Uuid>, _>("agent_id"),
        "action": r.get::<String, _>("action"),
        "toolName": r.get::<String, _>("tool_name"),
        "status": r.get::<String, _>("status"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
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
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "agentId": r.get::<Option<Uuid>, _>("agent_id"),
        "connectionId": r.get::<Option<Uuid>, _>("connection_id"),
        "status": r.get::<String, _>("status"),
        "justification": r.get::<Option<String>, _>("justification"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
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
    let rows = sqlx::query("SELECT * FROM tool_profiles WHERE company_id = $1 ORDER BY created_at ASC")
        .bind(company_id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list tool profiles: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<Option<String>, _>("description"),
        "status": r.get::<Option<String>, _>("status"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
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
        by_status.insert(
            r.get::<String, _>("status"),
            json!(r.get::<i64, _>("cnt")),
        );
    }
    Ok(Json(json!({ "connectionsByStatus": by_status, "healthy": true })))
}

/// GET /companies/:cid/tools/runtime-slots —— 聚合（静态空，后续接 runtime slot 服务）。
async fn tools_runtime_slots(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /companies/:cid/tools/trust-rules —— 默认信任规则（静态）。
async fn tools_trust_rules(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![json!({
        "id": "default-allow-approved-tools",
        "scope": "approved_tools",
        "decision": "allow",
        "priority": 100,
    })]))
}

/// GET /companies/:cid/tools/stdio-templates —— 静态 stdio 模板。
async fn tools_stdio_templates(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![json!({
        "name": "node-mcp-stdio",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-{name}"],
        "env": {},
    })]))
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
    Ok(Json(json!({ "id": application_id, "status": request.status })))
}

/// DELETE /api/tool-applications/:id
async fn delete_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    use sqlx::Row;
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
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "agentId": r.get::<Uuid, _>("agent_id"),
        "grantType": r.get::<String, _>("grant_type"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
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
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_invocations WHERE connection_id = $1",
    )
    .bind(connection_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    Ok(Json(json!({ "totalInvocations": total })))
}

/// GET /api/tool-connections/:id/installs —— 静态空（安装记录后续接）。
async fn connection_installs(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(_connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /api/tool-connections/:id/catalog —— 静态工具目录。
async fn connection_catalog(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(_connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![
        json!({ "name": "list_files", "description": "List files in workspace" }),
        json!({ "name": "read_file", "description": "Read a file's contents" }),
    ]))
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
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "toolName": r.get::<String, _>("tool_name"),
        "status": r.get::<String, _>("status"),
        "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("occurred_at"),
    })).collect()))
}

// ---------- tool-profiles ----------

/// GET /api/tool-profiles/:id/new-tools —— 静态空。
async fn profile_new_tools(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(_profile_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
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
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "agentId": r.get::<Option<Uuid>, _>("agent_id"),
        "toolName": r.get::<String, _>("tool_name"),
        "status": r.get::<String, _>("status"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
}

/// GET /api/tool-gateway/runtime-slots —— 静态空。
async fn gateway_runtime_slots(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
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
        .route(
            "/companies/:company_id/tools/profiles",
            get(tools_profiles),
        )
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
}
