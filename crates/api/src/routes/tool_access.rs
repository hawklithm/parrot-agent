//! Tool Access 补齐域 —— 对齐 Paperclip `routes/tool-access.ts` 中 Parrot 缺失的
//! company tools 子资源、tool-connections CRUD/子端点、tool-profiles、tool-applications。
//! gallery/examples/trust-rules/stdio-templates/runtime-health 以静态/聚合语义实现。

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use url::Url;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{assert_board, require_company_access, AccessMode};
use services::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService, PermissionKey};
use services::secret_provider::{decrypt_secret_material, encrypt_secret_material, sha256_hex};

// ---------- companies/:cid/tools/* 只读聚合 ----------

fn require_board_company_access(
    actor: &AuthorizationActor,
    company_id: Uuid,
    mode: AccessMode,
) -> Result<(), StatusCode> {
    assert_board(actor)?;
    require_company_access(actor, company_id, mode)
}

/// GET /companies/:cid/tools/gallery —— 内置工具目录（静态，与 tools.rs 内置集一致）。
async fn tools_gallery(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /companies/:cid/tools/action-requests —— 从 tool_action_requests 列表。
async fn tools_action_requests(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT ta.id, ta.company_id, ta.application_key,
                COALESCE(NULLIF(ta.name, ''), NULLIF(tc.name, ''), ta.application_key,
                         'legacy-tool-application') AS name,
                ta.description,
                COALESCE(NULLIF(ta.type, ''), 'custom') AS application_type,
                CASE WHEN ta.status IN ('draft', 'active', 'disabled', 'archived')
                     THEN ta.status ELSE 'active' END AS status,
                ta.plugin_id, ta.owner_agent_id, ta.owner_user_id,
                COALESCE(ta.metadata, '{}'::jsonb) AS metadata,
                ta.archived_at, ta.created_at, ta.updated_at,
                ta.agent_id AS legacy_agent_id,
                ta.connection_id AS legacy_connection_id
           FROM tool_applications ta
           LEFT JOIN tool_connections tc
             ON tc.id = ta.connection_id AND tc.company_id = ta.company_id
          WHERE ta.company_id = $1
          ORDER BY ta.updated_at DESC
          LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list tool applications: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let applications = rows.iter().map(tool_application_json).collect::<Vec<_>>();
    Ok(Json(json!({ "applications": applications })))
}

fn tool_application_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    let mut application = json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "applicationKey": row.get::<Option<String>, _>("application_key"),
        "name": row.get::<String, _>("name"),
        "description": row.get::<Option<String>, _>("description"),
        "type": row.get::<String, _>("application_type"),
        "status": row.get::<String, _>("status"),
        "pluginId": row.get::<Option<Uuid>, _>("plugin_id"),
        "ownerAgentId": row.get::<Option<Uuid>, _>("owner_agent_id"),
        "ownerUserId": row.get::<Option<String>, _>("owner_user_id"),
        "metadata": row.get::<Value, _>("metadata"),
        "archivedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    });
    if let Some(object) = application.as_object_mut() {
        if let Some(agent_id) = row.get::<Option<Uuid>, _>("legacy_agent_id") {
            object.insert("agentId".to_string(), json!(agent_id));
        }
        if let Some(connection_id) = row.get::<Option<Uuid>, _>("legacy_connection_id") {
            object.insert("connectionId".to_string(), json!(connection_id));
        }
    }
    application
}

fn is_safe_tool_application_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
}

fn normalize_tool_application_key(value: &str) -> String {
    let mut normalized = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let normalized = normalized.trim_matches('-');
    if normalized.is_empty() {
        "tool-application".to_string()
    } else {
        normalized.to_string()
    }
}

fn validate_tool_application_type(value: &str) -> bool {
    matches!(value, "mcp_http" | "mcp_stdio" | "paperclip_plugin" | "a2a")
}

fn validate_tool_application_status(value: &str) -> bool {
    matches!(value, "draft" | "active" | "disabled" | "archived")
}

async fn validate_tool_application_references(
    state: &AppState,
    company_id: Uuid,
    plugin_id: Option<Uuid>,
    owner_agent_id: Option<Uuid>,
) -> Result<(), StatusCode> {
    if let Some(plugin_id) = plugin_id {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM plugins WHERE id = $1)",
        )
        .bind(plugin_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate tool application plugin: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !exists {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    if let Some(owner_agent_id) = owner_agent_id {
        let belongs_to_company = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM agents WHERE id = $1 AND company_id = $2
             )",
        )
        .bind(owner_agent_id)
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to validate tool application owner agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if !belongs_to_company {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    Ok(())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.code())
        .as_deref()
        == Some("23505")
}

/// POST /companies/:cid/tools/applications —— 创建应用定义。
#[derive(Debug, Deserialize)]
struct CreateToolApplicationRequest {
    #[serde(rename = "applicationKey")]
    application_key: Option<String>,
    name: String,
    description: Option<String>,
    #[serde(rename = "type")]
    application_type: String,
    status: Option<String>,
    #[serde(rename = "pluginId")]
    plugin_id: Option<Uuid>,
    #[serde(rename = "ownerAgentId")]
    owner_agent_id: Option<Uuid>,
    #[serde(rename = "ownerUserId")]
    owner_user_id: Option<String>,
    metadata: Option<Value>,
}
async fn create_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateToolApplicationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let name = request.name.trim();
    let application_type = request.application_type.trim();
    let status = request.status.as_deref().unwrap_or("active");
    if name.is_empty()
        || name.chars().count() > 160
        || !validate_tool_application_type(application_type)
        || !validate_tool_application_status(status)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let application_key = request
        .application_key
        .as_deref()
        .map(str::trim)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalize_tool_application_key(name));
    if application_key.chars().count() > 160 || !is_safe_tool_application_key(&application_key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_tool_application_references(
        &state,
        company_id,
        request.plugin_id,
        request.owner_agent_id,
    )
    .await?;
    let metadata = request
        .metadata
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let id = Uuid::new_v4();
    let row = sqlx::query(
        "INSERT INTO tool_applications
            (id, company_id, application_key, name, description, type, status,
             plugin_id, owner_agent_id, owner_user_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   NULL::uuid AS legacy_agent_id,
                   NULL::uuid AS legacy_connection_id",
    )
    .bind(id)
    .bind(company_id)
    .bind(application_key)
    .bind(name)
    .bind(request.description)
    .bind(application_type)
    .bind(status)
    .bind(request.plugin_id)
    .bind(request.owner_agent_id)
    .bind(request.owner_user_id)
    .bind(metadata)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
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
        json!({ "name": name, "type": application_type }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(tool_application_json(&row))))
}

/// GET /companies/:cid/tools/profiles —— tool_profiles 列表。
async fn tools_profiles(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
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
    require_board_company_access(&actor, company_id, AccessMode::Read)
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
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    let rows = sqlx::query(
        "SELECT id, company_id, template_key, name, description, status, command, args, env_keys, tools,
                created_by_agent_id, created_by_user_id, disabled_at, created_at, updated_at
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
            .map(stdio_template_json)
            .collect(),
    ))
}

fn stdio_template_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "templateId": row.get::<String, _>("template_key"),
        "templateKey": row.get::<String, _>("template_key"),
        "name": row.get::<String, _>("name"),
        "title": Value::Null,
        "description": row.get::<Option<String>, _>("description"),
        "status": row.get::<String, _>("status"),
        "source": "admin",
        "command": row.get::<String, _>("command"),
        "args": row.get::<Value, _>("args"),
        "envKeys": row.get::<Value, _>("env_keys"),
        "tools": row.get::<Value, _>("tools"),
        "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
        "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
        "disabledAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("disabled_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

// ---------- tool-applications ----------

/// PATCH /api/tool-applications/:id —— 更新应用定义。
#[derive(Debug, Deserialize)]
struct UpdateToolApplicationRequest {
    name: Option<String>,
    description: Option<String>,
    status: Option<String>,
    #[serde(rename = "pluginId")]
    plugin_id: Option<Uuid>,
    #[serde(rename = "ownerAgentId")]
    owner_agent_id: Option<Uuid>,
    #[serde(rename = "ownerUserId")]
    owner_user_id: Option<String>,
    metadata: Option<Value>,
}
async fn update_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
    Json(request): Json<UpdateToolApplicationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor)?;
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM tool_applications WHERE id = $1",
    )
    .bind(application_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.trim().chars().count() > 160)
        || request
            .status
            .as_deref()
            .is_some_and(|status| !validate_tool_application_status(status))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    validate_tool_application_references(
        &state,
        company_id,
        request.plugin_id,
        request.owner_agent_id,
    )
    .await?;
    let metadata = request
        .metadata
        .as_ref()
        .filter(|value| !value.is_null());
    let row = sqlx::query(
        "UPDATE tool_applications
            SET name = COALESCE($2, name),
                description = COALESCE($3, description),
                status = COALESCE($4, status),
                plugin_id = COALESCE($5, plugin_id),
                owner_agent_id = COALESCE($6, owner_agent_id),
                owner_user_id = COALESCE($7, owner_user_id),
                metadata = COALESCE($8, metadata),
                updated_at = NOW()
          WHERE id = $1 AND company_id = $9
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   agent_id AS legacy_agent_id,
                   connection_id AS legacy_connection_id",
    )
    .bind(application_id)
    .bind(request.name.as_deref().map(str::trim))
    .bind(request.description.as_deref())
    .bind(request.status.as_deref())
    .bind(request.plugin_id)
    .bind(request.owner_agent_id)
    .bind(request.owner_user_id.as_deref())
    .bind(metadata)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to update tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.updated",
        &actor,
        "tool_application",
        application_id,
        json!({
            "name": request.name,
            "status": request.status,
        }),
    )
    .await;
    Ok(Json(tool_application_json(&row)))
}

/// DELETE /api/tool-applications/:id
async fn delete_tool_application(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(application_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    assert_board(&actor)?;
    use sqlx::Row;
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM tool_applications WHERE id = $1",
    )
    .bind(application_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load tool application: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let company_id = company_id.ok_or(StatusCode::NOT_FOUND)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!("Failed to start tool application delete transaction: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let has_connections = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections
              WHERE application_id = $1 AND company_id = $2
         )",
    )
    .bind(application_id)
    .bind(company_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to inspect tool application connections: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if has_connections {
        return Err(StatusCode::CONFLICT);
    }
    let row = sqlx::query(
        "DELETE FROM tool_applications
          WHERE id = $1 AND company_id = $2
         RETURNING id, company_id, application_key, name, description,
                   type AS application_type, status, plugin_id, owner_agent_id,
                   owner_user_id, metadata, archived_at, created_at, updated_at,
                   agent_id AS legacy_agent_id,
                   connection_id AS legacy_connection_id",
    )
        .bind(application_id)
        .bind(company_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    transaction.commit().await.map_err(|e| {
        tracing::error!("Failed to commit tool application delete: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_application.deleted",
        &actor,
        "tool_application",
        application_id,
        json!({
            "name": row.get::<String, _>("name"),
            "type": row.get::<String, _>("application_type"),
        }),
    )
    .await;
    Ok(Json(tool_application_json(&row)))
}

// ---------- tool-connections ----------

fn connection_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    let transport = row
        .get::<Option<String>, _>("transport")
        .or_else(|| row.get::<Option<String>, _>("tool_type"))
        .unwrap_or_else(|| "mcp_remote".to_string());
    let status = match row.get::<String, _>("status").as_str() {
        "draft" | "active" | "disabled" | "archived" => row.get::<String, _>("status"),
        _ => "draft".to_string(),
    };
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "applicationId": row.get::<Option<Uuid>, _>("application_id"),
        "name": row.get::<String, _>("name"),
        "uid": row.get::<String, _>("uid"),
        "connectionKind": row.get::<String, _>("connection_kind"),
        "ownership": row.get::<String, _>("ownership"),
        "transport": transport.clone(),
        "toolType": transport,
        "authKind": row.get::<String, _>("auth_kind"),
        "status": status,
        "enabled": row.get::<bool, _>("enabled"),
        "config": row.get::<Option<Value>, _>("config").unwrap_or_else(|| json!({})),
        "transportConfig": row.get::<Option<Value>, _>("transport_config").unwrap_or_else(|| json!({})),
        "credentialRefs": row.get::<Option<Value>, _>("credential_refs").unwrap_or_else(|| json!([])),
        "credentialSecretRefs": row.get::<Option<Value>, _>("credential_secret_refs").unwrap_or_else(|| json!([])),
        "healthStatus": row.get::<String, _>("health_status"),
        "healthMessage": row.get::<Option<String>, _>("health_message"),
        "healthCheckedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("health_checked_at"),
        "lastHealthAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_healthy_at"),
        "lastCatalogRefreshAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_catalog_refresh_at"),
        "lastError": row.get::<Option<String>, _>("last_error"),
        "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
        "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
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
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
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
    status: Option<String>,
    enabled: Option<bool>,
    config: Option<Value>,
    #[serde(rename = "transportConfig")]
    transport_config: Option<Value>,
    #[serde(rename = "credentialRefs")]
    credential_refs: Option<Value>,
    #[serde(rename = "credentialSecretRefs")]
    credential_secret_refs: Option<Value>,
}
async fn update_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<UpdateToolConnectionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty() || name.trim().chars().count() > 160)
        || request.status.as_deref().is_some_and(|status| {
            !matches!(status, "draft" | "active" | "disabled" | "archived")
        })
        || request
            .credential_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
        || request
            .credential_secret_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let row = sqlx::query(
        "UPDATE tool_connections
            SET name = COALESCE($3, name),
                status = COALESCE($4, status),
                enabled = COALESCE($5, enabled),
                config = COALESCE($6, config),
                transport_config = COALESCE($7, transport_config),
                credential_refs = COALESCE($8, credential_refs),
                credential_secret_refs = COALESCE($9, credential_secret_refs),
                updated_at = NOW()
          WHERE id = $1 AND company_id = $2
         RETURNING *",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(request.name.as_deref().map(str::trim))
    .bind(request.status.as_deref())
    .bind(request.enabled)
    .bind(request.config.as_ref())
    .bind(request.transport_config.as_ref())
    .bind(request.credential_refs.as_ref())
    .bind(request.credential_secret_refs.as_ref())
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
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "UPDATE tool_connections
            SET status = 'archived', enabled = false, updated_at = NOW()
          WHERE id = $1 AND company_id = $2
         RETURNING *",
    )
        .bind(connection_id)
        .bind(company_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool connection: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
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
    Ok(Json(connection_json(&row)))
}

/// GET /api/tool-connections/:id/grants
async fn list_connection_grants(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT * FROM tool_connection_grants
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
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
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;
    sqlx::query(
        "DELETE FROM tool_connection_grants
          WHERE id = $1 AND connection_id = $2 AND company_id = $3",
    )
        .bind(grant_id)
        .bind(connection_id)
        .bind(company_id)
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
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let total: i64 =
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM tool_invocations
              WHERE company_id = $1 AND connection_id = $2",
        )
            .bind(company_id)
            .bind(connection_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    Ok(Json(json!({ "totalInvocations": total })))
}

async fn load_connection_installs(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    connection_id: Uuid,
) -> Result<Vec<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, target_type, target_id, created_by_agent_id, created_by_user_id, created_at
           FROM tool_connection_installs
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to list connection installs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "targetType": row.get::<String, _>("target_type"),
                "targetId": row.get::<String, _>("target_id"),
                "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
                "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect())
}

/// GET /api/tool-connections/:id/installs —— 连接安装记录。
async fn connection_installs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let installs = load_connection_installs(&state.pool, company_id, connection_id).await?;
    Ok(Json(json!({
        "connectionId": connection_id,
        "installs": installs,
    })))
}

#[derive(Debug, Deserialize)]
struct ConnectionInstallRequest {
    #[serde(rename = "targetType", alias = "target_type")]
    target_type: String,
    #[serde(rename = "targetId", alias = "target_id")]
    target_id: String,
}

#[derive(Debug, Deserialize)]
struct PutConnectionInstallsRequest {
    #[serde(default)]
    installs: Vec<ConnectionInstallRequest>,
}

/// PUT /api/tool-connections/:id/installs —— 以声明式快照同步安装目标。
async fn put_connection_installs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<PutConnectionInstallsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;

    if request.installs.len() > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut requested = Vec::with_capacity(request.installs.len());
    let mut requested_keys = HashSet::with_capacity(request.installs.len());
    for install in request.installs {
        let target_type = install.target_type.trim().to_ascii_lowercase();
        let target_id = install.target_id.trim().to_string();
        if target_id.is_empty() || !matches!(target_type.as_str(), "company" | "agent") {
            return Err(StatusCode::BAD_REQUEST);
        }
        let target_uuid = target_id.parse::<Uuid>().map_err(|_| StatusCode::BAD_REQUEST)?;
        if target_type == "company" && target_uuid != company_id {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let key = (target_type.clone(), target_uuid.to_string());
        if requested_keys.insert(key) {
            requested.push((target_type, target_uuid));
        }
    }

    let created_by_agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    };
    let created_by_user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    };

    let mut tx = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to start connection install sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    for (target_type, target_id) in &requested {
        if target_type == "agent" {
            let valid_agent = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(
                     SELECT 1 FROM agents
                      WHERE id = $1 AND company_id = $2 AND status <> 'terminated'
                 )",
            )
            .bind(target_id)
            .bind(company_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %target_id, "Failed to validate connection install agent");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if !valid_agent {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    }

    use sqlx::Row;
    let existing = sqlx::query(
        "SELECT id, target_type, target_id
           FROM tool_connection_installs
          WHERE company_id = $1 AND connection_id = $2",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to load connection installs for sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let existing_keys: HashSet<(String, String)> = existing
        .iter()
        .map(|row| {
            (
                row.get::<String, _>("target_type"),
                row.get::<String, _>("target_id"),
            )
        })
        .collect();

    for row in &existing {
        let key = (
            row.get::<String, _>("target_type"),
            row.get::<String, _>("target_id"),
        );
        if !requested_keys.contains(&key) {
            sqlx::query(
                "DELETE FROM tool_connection_installs
                  WHERE id = $1 AND company_id = $2 AND connection_id = $3",
            )
            .bind(row.get::<Uuid, _>("id"))
            .bind(company_id)
            .bind(connection_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %connection_id, "Failed to remove connection install");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

    for (target_type, target_id) in &requested {
        if !existing_keys.contains(&(target_type.clone(), target_id.to_string())) {
            sqlx::query(
                "INSERT INTO tool_connection_installs
                    (company_id, connection_id, target_type, target_id, created_by_agent_id, created_by_user_id)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (company_id, connection_id, target_type, target_id) DO NOTHING",
            )
            .bind(company_id)
            .bind(connection_id)
            .bind(target_type)
            .bind(target_id.to_string())
            .bind(created_by_agent_id)
            .bind(created_by_user_id.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(%e, %company_id, %connection_id, "Failed to add connection install");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(%e, %company_id, %connection_id, "Failed to commit connection install sync");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let installs = load_connection_installs(&state.pool, company_id, connection_id).await?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.installs_synced",
        &actor,
        "tool_connection",
        connection_id,
        json!({
            "installs": installs.iter().map(|install| json!({
                "targetType": install.get("targetType").and_then(Value::as_str),
                "targetId": install.get("targetId").and_then(Value::as_str),
            })).collect::<Vec<_>>(),
        }),
    )
    .await;
    Ok(Json(json!({
        "connectionId": connection_id,
        "installs": installs,
    })))
}

/// GET /api/tool-connections/:id/catalog —— MCP 发现得到的工具目录。
async fn connection_catalog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, name, tool_name, title, description, input_schema, output_schema, risk_level, status, last_seen_at
           FROM tool_catalog_entries
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY name ASC",
    )
    .bind(company_id)
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
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, tool_name, status, occurred_at FROM tool_call_events \
         WHERE company_id = $1 AND connection_id = $2
         ORDER BY occurred_at DESC LIMIT 50",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection activity: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let events: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "toolName": r.get::<String, _>("tool_name"),
                "status": r.get::<String, _>("status"),
                "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("occurred_at"),
            })
        })
        .collect();

    // Derive lifecycle events from the connection-scoped activity log, the
    // same way Paperclip's listConnectionLifecycleEvents does (no separate
    // table): map each log row through the canonical lifecycle mapper.
    let log_rows = sqlx::query(
        "SELECT id, action, entity_type, entity_id, resource_type, resource_id, details, actor_type, agent_id, user_id, created_at \
           FROM activity_log
          WHERE company_id = $1
            AND ((entity_type = 'tool_connection' AND entity_id = $2)
              OR (resource_type = 'tool_connection' AND resource_id = $2))
          ORDER BY created_at DESC LIMIT 50",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list connection lifecycle log: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let lifecycle_events: Vec<serde_json::Value> = log_rows
        .iter()
        .filter_map(|r| {
            let action: String = r.get("action");
            let details: serde_json::Value = r.get("details");
            let lifecycle_type =
                services::tool_access_contract::activity_log_action_to_lifecycle_type(
                    &action,
                    Some(&details),
                )?;
            let actor_type: Option<String> = r.try_get("actor_type").unwrap_or(None);
            let agent_id: Option<Uuid> = r.try_get("agent_id").unwrap_or(None);
            let user_id: Option<Uuid> = r.try_get("user_id").unwrap_or(None);
            let actor_display = user_id
                .map(|id| id.to_string())
                .or_else(|| agent_id.map(|id| id.to_string()));
            Some(json!({
                "id": r.get::<Uuid, _>("id"),
                "connectionId": connection_id,
                "type": lifecycle_type,
                "actorType": actor_type,
                "actorId": user_id,
                "agentId": agent_id,
                "actorDisplayName": actor_display,
                "details": details,
                "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            }))
        })
        .collect();

    Ok(Json(json!({
        "connectionId": connection_id,
        "events": events,
        "lifecycleEvents": lifecycle_events,
    })))
}

// ---------- tool-profiles ----------

async fn ensure_tool_profile_scope(
    state: &AppState,
    profile_id: Uuid,
    company_id: Uuid,
) -> Result<(), StatusCode> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM tool_profiles WHERE id = $1 AND company_id = $2)",
    )
    .bind(profile_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check tool profile scope: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    exists.then_some(()).ok_or(StatusCode::NOT_FOUND)
}

/// GET /api/tool-profiles/:id/new-tools —— profile 尚未审核的目录工具。
async fn profile_new_tools(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    ensure_tool_profile_scope(&state, profile_id, company_id).await?;
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let result = sqlx::query("DELETE FROM tool_profiles WHERE id = $1 AND company_id = $2")
        .bind(profile_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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

#[derive(Debug, Deserialize)]
struct GatewayAuditQuery {
    #[serde(rename = "companyId")]
    company_id: Option<String>,
    app: Option<String>,
    agent: Option<String>,
    outcome: Option<String>,
    window: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
    cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayAuditCursor {
    #[serde(rename = "createdAt")]
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn gateway_audit_cursor_encode(cursor: &GatewayAuditCursor) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(cursor).expect("audit cursor is serializable"))
}

fn gateway_audit_cursor_decode(value: &str) -> Option<GatewayAuditCursor> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn gateway_audit_window(window: &str) -> Option<Duration> {
    match window {
        "1h" => Some(Duration::hours(1)),
        "24h" => Some(Duration::hours(24)),
        "7d" => Some(Duration::days(7)),
        "30d" => Some(Duration::days(30)),
        _ => None,
    }
}

#[derive(Debug)]
struct ActiveAgentRunContext {
    agent_id: Uuid,
    company_id: Uuid,
    run_id: Uuid,
    responsible_user_id: Option<String>,
    issue_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

async fn load_active_agent_run_context(
    state: &AppState,
    actor: &AuthorizationActor,
) -> Result<ActiveAgentRunContext, StatusCode> {
    let (agent_id, company_id, run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id,
            company_id,
            run_id: Some(run_id),
            ..
        } => (*agent_id, *company_id, *run_id),
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT responsible_user_id, context_snapshot
           FROM heartbeat_runs
          WHERE id = $1 AND company_id = $2 AND agent_id = $3 AND status = 'running'",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %agent_id, %company_id, %run_id, "Failed to validate agent run context");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::FORBIDDEN);
    };
    let context_snapshot = row.get::<Option<Value>, _>("context_snapshot");
    let issue_id = context_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("issueId").or_else(|| snapshot.get("taskId")))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let project_id = context_snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.get("projectId"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    Ok(ActiveAgentRunContext {
        agent_id,
        company_id,
        run_id,
        responsible_user_id: row.get("responsible_user_id"),
        issue_id,
        project_id,
    })
}

async fn require_active_agent_run(
    state: &AppState,
    actor: &AuthorizationActor,
) -> Result<(Uuid, Uuid, Uuid), StatusCode> {
    let context = load_active_agent_run_context(state, actor).await?;
    Ok((context.agent_id, context.company_id, context.run_id))
}

fn gateway_audit_like_pattern(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn gateway_audit_outcome(event_type: &str, decision: Option<&str>, outcome: &str) -> &'static str {
    if matches!(event_type, "call_completed" | "tool_gateway.call_completed")
        || matches!(decision, Some("allow" | "approved"))
        || matches!(outcome, "success" | "allowed")
    {
        return "allowed";
    }
    if matches!(event_type, "approval_requested" | "tool_gateway.approval_requested")
        || decision == Some("require_approval")
    {
        return "asked_first";
    }
    if matches!(event_type, "call_deferred" | "tool_gateway.call_deferred")
        || decision == Some("defer_runtime")
    {
        return "waiting";
    }
    if matches!(event_type, "call_failed" | "tool_gateway.call_failed")
        || matches!(outcome, "failure" | "failed")
    {
        return "failed";
    }
    if matches!(event_type, "call_denied" | "tool_gateway.call_denied")
        || matches!(decision, Some("deny" | "rate_limited"))
        || matches!(outcome, "denied" | "blocked")
    {
        return "blocked";
    }
    "unknown"
}

/// GET /api/tool-gateway/audit —— Paperclip-compatible filtered activity feed.
async fn gateway_audit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<GatewayAuditQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    crate::routes::assert_board(&actor).map_err(|_| StatusCode::FORBIDDEN)?;
    let company_id = query
        .company_id
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    require_board_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let permission = AuthorizationService::decide(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::from_const(PermissionKey::TOOLS_VIEW_AUDIT),
        },
        Some(company_id),
    )
    .await;
    if !permission.allowed {
        return Err(StatusCode::FORBIDDEN);
    }

    let application_or_connection_id = query
        .app
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let agent_id = query
        .agent
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let window = query.window.as_deref().unwrap_or("24h");
    let window_duration = gateway_audit_window(window).ok_or(StatusCode::BAD_REQUEST)?;
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(gateway_audit_like_pattern);
    let outcome = query
        .outcome
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let cursor = match query.cursor.as_deref() {
        Some(value) => Some(gateway_audit_cursor_decode(value).ok_or(StatusCode::BAD_REQUEST)?),
        None => None,
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 100);
    let window_start = Utc::now() - window_duration;

    let rows = sqlx::query(
        r#"
        SELECT e.id,
               e.company_id,
               e.event_type,
               e.actor_type,
               e.actor_id,
               COALESCE(e.agent_id, i.agent_id) AS agent_id,
               COALESCE(e.run_id, i.run_id) AS run_id,
               COALESCE(e.application_id, i.application_id) AS application_id,
               COALESCE(e.connection_id, i.connection_id) AS connection_id,
               e.invocation_id,
               e.action_request_id,
               e.tool_name,
               e.decision,
               e.outcome,
               e.reason_code,
               e.arguments_summary,
               e.request_summary,
               e.result_summary,
               e.error_code,
               e.error_message,
               e.metadata,
               e.created_at,
               a.name AS agent_name,
               c.name AS connection_name,
               c.application_id AS connection_application_id
          FROM tool_call_events e
          LEFT JOIN tool_invocations i
            ON i.id = e.invocation_id AND i.company_id = e.company_id
          LEFT JOIN agents a
            ON a.id = COALESCE(e.agent_id, i.agent_id)
           AND a.company_id = e.company_id
          LEFT JOIN tool_connections c
            ON c.id = COALESCE(e.connection_id, i.connection_id)
           AND c.company_id = e.company_id
         WHERE e.company_id = $1
           AND e.created_at >= $2
           AND ($3::uuid IS NULL
                OR e.application_id = $3
                OR i.application_id = $3
                OR e.connection_id = $3
                OR i.connection_id = $3
                OR c.application_id = $3)
           AND ($4::uuid IS NULL OR COALESCE(e.agent_id, i.agent_id) = $4)
           AND (
             $5::text IS NULL
             OR e.event_type ILIKE $5 ESCAPE '\'
             OR COALESCE(e.tool_name, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.reason_code, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.error_code, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(a.name, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(c.name, '') ILIKE $5 ESCAPE '\'
             OR COALESCE(e.metadata::text, '') ILIKE $5 ESCAPE '\'
           )
           AND (
             $6::text IS NULL
             OR CASE $6
                  WHEN 'allowed' THEN e.event_type IN ('call_completed', 'tool_gateway.call_completed')
                                      OR e.decision IN ('allow', 'approved')
                                      OR e.outcome IN ('success', 'allowed')
                  WHEN 'blocked' THEN e.event_type IN ('call_denied', 'tool_gateway.call_denied')
                                      OR e.decision IN ('deny', 'rate_limited')
                                      OR e.outcome IN ('denied', 'blocked')
                  WHEN 'asked_first' THEN e.event_type IN ('approval_requested', 'tool_gateway.approval_requested')
                                      OR e.decision = 'require_approval'
                  WHEN 'waiting' THEN e.event_type IN ('call_deferred', 'tool_gateway.call_deferred')
                                      OR e.decision = 'defer_runtime'
                  WHEN 'failed' THEN e.event_type IN ('call_failed', 'tool_gateway.call_failed')
                                      OR e.outcome IN ('failure', 'failed')
                  ELSE TRUE
                END
           )
           AND (
             $7::timestamptz IS NULL
             OR e.created_at < $7
             OR (e.created_at = $7 AND e.id < $8)
           )
         ORDER BY e.created_at DESC, e.id DESC
         LIMIT $9
        "#,
    )
    .bind(company_id)
    .bind(window_start)
    .bind(application_or_connection_id)
    .bind(agent_id)
    .bind(search)
    .bind(outcome)
    .bind(cursor.as_ref().map(|value| value.created_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list gateway audit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    use sqlx::Row;
    let has_more = rows.len() > limit as usize;
    let visible = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
    let events = visible
        .iter()
        .map(|row| {
            let event_type: String = row.get("event_type");
            let actor_type: String = row.get("actor_type");
            let actor_id: Option<String> = row.get("actor_id");
            let agent_id: Option<Uuid> = row.get("agent_id");
            let run_id: Option<Uuid> = row.get("run_id");
            let application_id: Option<Uuid> = row.get("application_id");
            let connection_id: Option<Uuid> = row.get("connection_id");
            let invocation_id: Option<Uuid> = row.get("invocation_id");
            let action_request_id: Option<Uuid> = row.get("action_request_id");
            let tool_name: Option<String> = row.get("tool_name");
            let decision: Option<String> = row.get("decision");
            let outcome: String = row.get("outcome");
            let reason_code: Option<String> = row.get("reason_code");
            let error_code: Option<String> = row.get("error_code");
            let error_message: Option<String> = row.get("error_message");
            let metadata: Option<Value> = row.get("metadata");
            let mut details = match metadata {
                Some(Value::Object(object)) => Value::Object(object),
                Some(value) => json!({"metadata": value}),
                None => json!({}),
            };
            if let Some(value) = decision.as_deref() {
                details["decision"] = Value::String(value.to_string());
            }
            if let Some(value) = reason_code.as_deref() {
                details["reasonCode"] = Value::String(value.to_string());
            }
            if let Some(value) = tool_name.as_deref() {
                details["tool"] = Value::String(value.to_string());
            }
            if let Some(value) = application_id {
                details["applicationId"] = Value::String(value.to_string());
            }
            if let Some(value) = connection_id {
                details["connectionId"] = Value::String(value.to_string());
            }
            if let Some(value) = invocation_id {
                details["invocationId"] = Value::String(value.to_string());
            }
            if let Some(value) = action_request_id {
                details["actionRequestId"] = Value::String(value.to_string());
            }
            if let Some(value) = error_code.as_deref() {
                details["errorCode"] = Value::String(value.to_string());
            }
            if let Some(value) = error_message.as_deref() {
                details["error"] = Value::String(value.to_string());
            }
            if let Some(value) = row.get::<Option<Value>, _>("arguments_summary") {
                details["argumentsSummary"] = value;
            }
            if let Some(value) = row.get::<Option<Value>, _>("request_summary") {
                details["requestSummary"] = value;
            }
            if let Some(value) = row.get::<Option<Value>, _>("result_summary") {
                details["resultSummary"] = value;
            }
            let agent_name: Option<String> = row.get("agent_name");
            let connection_name: Option<String> = row.get("connection_name");
            let effective_application_id = application_id.or_else(|| {
                row.get::<Option<Uuid>, _>("connection_application_id")
            });
            let normalized_outcome = gateway_audit_outcome(
                &event_type,
                decision.as_deref(),
                &outcome,
            );
            json!({
                "id": row.get::<Uuid, _>("id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "action": event_type,
                "actorType": actor_type,
                "actorId": actor_id,
                "entityType": if invocation_id.is_some() { "tool_invocation" } else if action_request_id.is_some() { "tool_action_request" } else { "tool_gateway" },
                "entityId": invocation_id.or(action_request_id),
                "details": details,
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
                "agentId": agent_id,
                "runId": run_id,
                "applicationId": effective_application_id,
                "connectionId": connection_id,
                "agentDisplayName": agent_name,
                "appDisplayName": connection_name,
                "applicationDisplayName": Value::Null,
                "connectionDisplayName": connection_name,
                "toolDisplayName": tool_name,
                "normalizedOutcome": normalized_outcome,
            })
        })
        .collect::<Vec<_>>();
    let next_cursor = if has_more {
        visible.last().map(|row| {
            gateway_audit_cursor_encode(&GatewayAuditCursor {
                created_at: row.get("created_at"),
                id: row.get("id"),
            })
        })
    } else {
        None
    };
    Ok(Json(json!({
        "events": events,
        "nextCursor": next_cursor,
    })))
}

/// GET /api/tool-gateway/runtime-slots —— 网关运行时槽位。
async fn require_gateway_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    permission_key: &'static str,
) -> Result<(), StatusCode> {
    crate::routes::assert_board(actor).map_err(|_| StatusCode::FORBIDDEN)?;
    require_company_access(actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let decision = AuthorizationService::decide(
        &state.pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::from_const(permission_key),
        },
        Some(company_id),
    )
    .await;
    decision
        .allowed
        .then_some(())
        .ok_or(StatusCode::FORBIDDEN)
}

async fn require_gateway_any_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    permission_keys: &[&'static str],
) -> Result<(), StatusCode> {
    crate::routes::assert_board(actor).map_err(|_| StatusCode::FORBIDDEN)?;
    require_company_access(actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    for permission_key in permission_keys {
        let decision = AuthorizationService::decide(
            &state.pool,
            actor,
            &AuthorizationAction::Permission {
                key: PermissionKey::from_const(*permission_key),
            },
            Some(company_id),
        )
        .await;
        if decision.allowed {
            return Ok(());
        }
    }
    Err(StatusCode::FORBIDDEN)
}

async fn require_gateway_runtime_permission(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<(), StatusCode> {
    require_gateway_permission(state, actor, company_id, PermissionKey::TOOLS_MANAGE_RUNTIME).await
}

async fn gateway_runtime_slots(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
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
        .route(
            "/tool-connections/:id/installs",
            get(connection_installs).put(put_connection_installs),
        )
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
            "/companies/:company_id/tools/stdio-templates/:template_id/disable",
            post(disable_stdio_template),
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_profile_entries AS e
            SET effect = CASE WHEN COALESCE($2, e.effect = 'include') THEN 'include' ELSE 'exclude' END,
                updated_at = NOW()
           FROM tool_profiles AS p
          WHERE e.id = $1 AND e.profile_id = p.id AND p.company_id = $3
          RETURNING e.*",
    )
    .bind(entry_id)
    .bind(request.enabled)
    .bind(company_id)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let result = sqlx::query(
        "DELETE FROM tool_profile_entries AS e
           USING tool_profiles AS p
          WHERE e.id = $1 AND e.profile_id = p.id AND p.company_id = $2",
    )
        .bind(entry_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete tool profile entry: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/tool-connections/:id/test-agents —— 当前公司可用于测试的 agent。
async fn connection_test_agents(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((connection_id, call_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
    let connection_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !connection_exists {
        return Err(StatusCode::NOT_FOUND);
    }
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_gateway_permission(
        &state,
        &actor,
        company_id,
        PermissionKey::TOOLS_MANAGE_CONNECTIONS,
    )
    .await?;
    let connection_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !connection_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query(
        "INSERT INTO tool_connection_grants (company_id, connection_id, agent_id) \
         SELECT company_id, $1, id FROM agents WHERE company_id = $2 AND status <> 'terminated' \
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

#[derive(Debug, Deserialize)]
struct OAuthCallbackQuery {
    state: String,
    code: Option<String>,
    error: Option<String>,
    #[serde(rename = "error_description")]
    _error_description: Option<String>,
}

#[derive(Debug)]
struct OAuthTokenExchange {
    access_token: String,
    refresh_token: Option<String>,
    token_type: Option<String>,
    scope: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}

fn validate_oauth_endpoint(value: &str) -> Result<Url, StatusCode> {
    let url = Url::parse(value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    let host = url.host_str().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let local_host = matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1");
    if url.username() != ""
        || url.password().is_some()
        || (url.scheme() != "https" && !(url.scheme() == "http" && local_host))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(url)
}

fn configured_oauth_token_uri(row: &sqlx::postgres::PgRow) -> Option<String> {
    connection_config_string_in_sections(
        row,
        &["oauth", "tokenBroker", "token_broker", "broker"],
        &[
            "tokenUrl",
            "token_url",
            "tokenUri",
            "token_uri",
            "tokenEndpoint",
            "token_endpoint",
        ],
    )
}

fn oauth_callback_actor_matches(
    actor: &AuthorizationActor,
    state_row: &sqlx::postgres::PgRow,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let user_id_string = user_id.to_string();
    let subject_user_id = state_row.get::<Option<String>, _>("subject_user_id");
    if let Some(subject_user_id) = subject_user_id {
        if subject_user_id != user_id_string {
            return Err(StatusCode::FORBIDDEN);
        }
    } else if state_row.get::<Option<String>, _>("created_by_actor_type").as_deref()
        != Some("user")
        || state_row.get::<Option<String>, _>("created_by_actor_id").as_deref()
            != Some(user_id_string.as_str())
        || state_row
            .get::<Option<String>, _>("created_by_session_id")
            .is_some()
    {
        // The current actor model does not carry a session id. Fail closed for
        // a state that requires an unavailable session binding; Agent-started
        // states use subject_user_id and remain fully verifiable here.
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user_id)
}

async fn consume_oauth_state(pool: &sqlx::PgPool, state_token: &str) -> Result<(), StatusCode> {
    let mut transaction = pool.begin().await.map_err(|e| {
        tracing::error!(%e, "Failed to start OAuth state consumption transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let consumed = sqlx::query(
        "DELETE FROM tool_oauth_states
          WHERE state = $1 AND expires_at > NOW()
       RETURNING state",
    )
    .bind(state_token)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to consume OAuth state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if consumed.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, "Failed to commit OAuth state consumption");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(())
}

async fn resolve_oauth_client_secret(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    row: &sqlx::postgres::PgRow,
) -> Result<Option<String>, StatusCode> {
    use sqlx::Row;
    let refs = row
        .get::<Option<Value>, _>("credential_secret_refs")
        .unwrap_or_else(|| json!([]));
    let Some(reference) = refs.as_array().and_then(|values| {
        values.iter().find(|value| {
            let Some(path) = value
                .get("configPath")
                .or_else(|| value.get("config_path"))
                .or_else(|| value.get("path"))
                .and_then(Value::as_str)
            else {
                return false;
            };
            matches!(
                path.to_ascii_lowercase().as_str(),
                "oauth.clientsecret"
                    | "oauth.client_secret"
                    | "clientsecret"
                    | "client_secret"
            )
        })
    }) else {
        return Ok(None);
    };
    let secret_id = reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let version_selector = reference
        .get("versionSelector")
        .or_else(|| reference.get("version_selector"))
        .and_then(Value::as_str)
        .unwrap_or("latest")
        .trim();
    let material: Option<Value> = if version_selector.eq_ignore_ascii_case("latest") {
        sqlx::query_scalar(
            "SELECT v.material
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.status = 'current' AND v.revoked_at IS NULL
              ORDER BY v.version DESC
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(company_id)
        .fetch_optional(pool)
        .await
    } else {
        let version = version_selector
            .parse::<i32>()
            .ok()
            .filter(|version| *version > 0)
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        sqlx::query_scalar(
            "SELECT v.material
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.version = $3 AND v.revoked_at IS NULL
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(company_id)
        .bind(version)
        .fetch_optional(pool)
        .await
    }
    .map_err(|e| {
        tracing::error!(%e, %secret_id, "Failed to load OAuth client secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let material = material.ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    decrypt_secret_material(&material)
        .map(Some)
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to decrypt OAuth client secret");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn exchange_oauth_code(
    token_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
    requested_scope: &[String],
) -> Result<OAuthTokenExchange, StatusCode> {
    let token_uri = validate_oauth_endpoint(token_uri)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("client_id", client_id.to_string()),
    ];
    if let Some(client_secret) = client_secret {
        form.push(("client_secret", client_secret.to_string()));
    }
    let response = client
        .post(token_uri)
        .form(&form)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(%e, "OAuth token endpoint request failed");
            StatusCode::BAD_GATEWAY
        })?;
    let status = response.status();
    let payload = response.json::<Value>().await.map_err(|e| {
        tracing::warn!(%e, "OAuth token endpoint returned invalid JSON");
        StatusCode::BAD_GATEWAY
    })?;
    if !status.is_success() {
        tracing::warn!(%status, "OAuth token endpoint rejected authorization code");
        return Err(StatusCode::BAD_GATEWAY);
    }
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let token_type = payload
        .get("token_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let scope = payload
        .get("scope")
        .map(config_scope_values)
        .filter(|scope| !scope.is_empty())
        .unwrap_or_else(|| requested_scope.to_vec());
    let expires_at = payload
        .get("expires_in")
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|seconds| (1..=31_536_000).contains(seconds))
        .map(|seconds| Utc::now() + Duration::seconds(seconds));
    Ok(OAuthTokenExchange {
        access_token,
        refresh_token,
        token_type,
        scope,
        expires_at,
    })
}

async fn upsert_oauth_secret(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    company_id: Uuid,
    key: &str,
    name: &str,
    value: &str,
    created_by_user_id: Uuid,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let (material, digest) = encrypt_secret_material(value).map_err(|e| {
        tracing::error!(%e, "Failed to encrypt OAuth credential");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let existing = sqlx::query(
        "SELECT id, latest_version
           FROM company_secrets
          WHERE company_id = $1 AND scope = 'company' AND key = $2
            AND deleted_at IS NULL
          FOR UPDATE",
    )
    .bind(company_id)
    .bind(key)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to lock OAuth secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some(existing) = existing {
        let secret_id: Uuid = existing.get("id");
        let next_version = existing.get::<i32, _>("latest_version").max(1) + 1;
        sqlx::query(
            "UPDATE company_secret_versions
                SET status = 'superseded', revoked_at = NOW()
              WHERE secret_id = $1 AND status = 'current'",
        )
        .bind(secret_id)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to retire OAuth secret version");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "INSERT INTO company_secret_versions
                (secret_id, version, material, value_sha256, fingerprint_sha256, status)
             VALUES ($1, $2, $3, $4, $4, 'current')",
        )
        .bind(secret_id)
        .bind(next_version)
        .bind(material)
        .bind(&digest)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to store OAuth secret version");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "UPDATE company_secrets
                SET latest_version = $2, last_rotated_at = NOW(), updated_at = NOW(),
                    status = 'active'
              WHERE id = $1",
        )
        .bind(secret_id)
        .bind(next_version)
        .execute(&mut **transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %secret_id, "Failed to update OAuth secret metadata");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        return Ok(secret_id);
    }

    let secret_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, scope, key, name, provider, status, managed_mode,
             description, created_by_user_id)
         VALUES ($1, $2, 'company', $3, $4, 'local_encrypted', 'active',
                 'paperclip_managed', $5, $6)",
    )
    .bind(secret_id)
    .bind(company_id)
    .bind(key)
    .bind(name)
    .bind("OAuth credential managed by Tool Gateway")
    .bind(created_by_user_id.to_string())
    .execute(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to create OAuth secret");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(secret_id)
    .bind(material)
    .bind(&digest)
    .execute(&mut **transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %secret_id, "Failed to store initial OAuth secret version");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(secret_id)
}

fn oauth_secret_key(connection_id: Uuid, kind: &str, subject_user_id: Option<&str>) -> String {
    let subject_suffix = subject_user_id
        .map(|subject| format!("_{}", &sha256_hex(subject)[..16]))
        .unwrap_or_default();
    format!("tool_connection_{connection_id}_oauth_{kind}{subject_suffix}")
}

fn oauth_secret_ref(secret_id: Uuid, config_path: &str, label: &str) -> Value {
    json!({
        "secretId": secret_id,
        "versionSelector": "latest",
        "configPath": config_path,
        "required": config_path.ends_with("access_token"),
        "label": label,
    })
}

fn credential_ref_path(value: &Value) -> Option<&str> {
    value
        .get("configPath")
        .or_else(|| value.get("config_path"))
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)
}

fn merge_oauth_secret_refs(existing: Value, access_ref: Value, refresh_ref: Option<Value>) -> Value {
    let mut refs = existing.as_array().cloned().unwrap_or_default();
    refs.retain(|reference| {
        !credential_ref_path(reference).is_some_and(|path| {
            matches!(
                path.to_ascii_lowercase().as_str(),
                "oauth.access_token"
                    | "oauth.access-token"
                    | "oauth.refreshtoken"
                    | "oauth.refresh_token"
                    | "oauth.refresh-token"
            )
        })
    });
    refs.push(access_ref);
    if let Some(refresh_ref) = refresh_ref {
        refs.push(refresh_ref);
    }
    Value::Array(refs)
}

fn oauth_connection_config_with_metadata(
    existing: Value,
    token: &OAuthTokenExchange,
) -> Value {
    let mut config = existing.as_object().cloned().unwrap_or_default();
    let mut oauth = config
        .get("oauth")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    oauth.insert("connectedAt".to_string(), json!(Utc::now()));
    if let Some(token_type) = token.token_type.as_deref() {
        oauth.insert("tokenType".to_string(), json!(token_type));
    }
    if !token.scope.is_empty() {
        oauth.insert("scope".to_string(), json!(token.scope.join(" ")));
    }
    if let Some(expires_at) = token.expires_at {
        oauth.insert("expiresAt".to_string(), json!(expires_at));
    }
    config.insert("oauth".to_string(), Value::Object(oauth));
    Value::Object(config)
}

async fn persist_oauth_connection(
    state: &AppState,
    actor: &AuthorizationActor,
    connection_id: Uuid,
    company_id: Uuid,
    subject_user_id: Option<&str>,
    token: &OAuthTokenExchange,
) -> Result<Uuid, StatusCode> {
    use sqlx::Row;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to start OAuth credential transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let connection = sqlx::query(
        "SELECT credential_secret_refs, config
           FROM tool_connections
          WHERE id = $1 AND company_id = $2
          FOR UPDATE",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to lock OAuth connection");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let subject_suffix = subject_user_id
        .map(|subject| format!(" for user {}", &sha256_hex(subject)[..16]))
        .unwrap_or_default();
    let access_id = upsert_oauth_secret(
        &mut transaction,
        company_id,
        &oauth_secret_key(connection_id, "access_token", subject_user_id),
        &format!("OAuth access token{subject_suffix}"),
        &token.access_token,
        user_id,
    )
    .await?;
    let refresh_id = if let Some(refresh_token) = token.refresh_token.as_deref() {
        Some(
            upsert_oauth_secret(
                &mut transaction,
                company_id,
                &oauth_secret_key(connection_id, "refresh_token", subject_user_id),
                &format!("OAuth refresh token{subject_suffix}"),
                refresh_token,
                user_id,
            )
            .await?,
        )
    } else {
        None
    };
    let access_ref = oauth_secret_ref(access_id, "oauth.access_token", "OAuth access token");
    let refresh_ref = refresh_id.map(|id| oauth_secret_ref(id, "oauth.refresh_token", "OAuth refresh token"));
    let secret_refs = merge_oauth_secret_refs(
        connection
            .get::<Option<Value>, _>("credential_secret_refs")
            .unwrap_or_else(|| json!([])),
        access_ref.clone(),
        refresh_ref.clone(),
    );
    let config = oauth_connection_config_with_metadata(
        connection
            .get::<Option<Value>, _>("config")
            .unwrap_or_else(|| json!({})),
        token,
    );
    let grant_id = if let Some(subject_user_id) = subject_user_id {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO connection_grants
                (id, company_id, connection_id, kind, subject_user_id,
                 credential_secret_refs, status, is_default, created_by_user_id)
             VALUES ($1, $2, $3, 'user', $4, $5, 'active', false, $6)
             ON CONFLICT (connection_id, subject_user_id)
             DO UPDATE SET company_id = EXCLUDED.company_id,
                           credential_secret_refs = EXCLUDED.credential_secret_refs,
                           status = 'active', revoked_at = NULL,
                           updated_at = NOW()
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(connection_id)
        .bind(subject_user_id)
        .bind(&secret_refs)
        .bind(user_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to persist user OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query(
            "INSERT INTO connection_grants
                (id, company_id, connection_id, kind, credential_secret_refs,
                 status, is_default, created_by_user_id)
             VALUES ($1, $2, $3, 'workspace', $4, 'active', true, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(connection_id)
        .bind(&secret_refs)
        .bind(user_id.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to ensure workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "UPDATE connection_grants
                SET credential_secret_refs = $3, status = 'active', revoked_at = NULL,
                    updated_at = NOW()
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true",
        )
        .bind(company_id)
        .bind(connection_id)
        .bind(&secret_refs)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to update workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true
              LIMIT 1",
        )
        .bind(company_id)
        .bind(connection_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load workspace OAuth grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };
    if subject_user_id.is_none() {
        sqlx::query(
            "UPDATE tool_connections
                SET credential_secret_refs = $3, config = $4,
                    status = 'active', enabled = true, health_status = 'unchecked',
                    health_message = 'OAuth credentials stored; health refresh pending',
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&secret_refs)
        .bind(&config)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to activate OAuth connection");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    } else {
        sqlx::query(
            "UPDATE tool_connections
                SET config = $3, status = 'active', enabled = true,
                    health_status = 'unchecked',
                    health_message = 'OAuth credentials stored; health refresh pending',
                    last_error = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(connection_id)
        .bind(company_id)
        .bind(&config)
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to activate user OAuth connection");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to commit OAuth credentials");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(grant_id)
}

/// GET /api/tools/oauth/callback —— validate, consume and complete a PKCE OAuth callback.
async fn tools_oauth_callback(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<OAuthCallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let state_token = query.state.trim();
    if state_token.is_empty() || state_token.chars().count() > 512 {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let state_row = sqlx::query(
        "SELECT state, company_id, connection_id, code_verifier,
                created_by_actor_type, created_by_actor_id, created_by_session_id,
                subject_user_id, requested_scopes, expires_at
           FROM tool_oauth_states
          WHERE state = $1",
    )
    .bind(state_token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to load OAuth callback state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::BAD_REQUEST)?;
    let company_id: Uuid = state_row.get("company_id");
    let connection_id: Uuid = state_row.get("connection_id");
    let user_id = oauth_callback_actor_matches(&actor, &state_row)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if state_row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if connection.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    if connection.get::<String, _>("auth_kind") != "oauth" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let client_id = connection_config_string(&connection, &["clientId", "client_id"])
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let redirect_uri = configured_oauth_redirect_uri(&connection)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let token_uri = configured_oauth_token_uri(&connection)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    validate_oauth_endpoint(&token_uri)?;
    let client_secret = resolve_oauth_client_secret(&state.pool, company_id, &connection).await?;
    let provider_error = query
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if provider_error.is_some() {
        consume_oauth_state(&state.pool, state_token).await?;
        tracing::warn!(%connection_id, error = provider_error.unwrap_or("unknown"), "OAuth provider returned an authorization error");
        return Err(StatusCode::BAD_REQUEST);
    }
    let code = query
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= 8192)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let requested_scope = state_row
        .get::<Option<Value>, _>("requested_scopes")
        .map(|value| config_scope_values(&value))
        .unwrap_or_default();
    consume_oauth_state(&state.pool, state_token).await?;
    let token = exchange_oauth_code(
        &token_uri,
        &client_id,
        client_secret.as_deref(),
        &redirect_uri,
        code,
        &state_row.get::<String, _>("code_verifier"),
        &requested_scope,
    )
    .await?;
    let subject_user_id = state_row.get::<Option<String>, _>("subject_user_id");
    let grant_id = persist_oauth_connection(
        &state,
        &actor,
        connection_id,
        company_id,
        subject_user_id.as_deref(),
        &token,
    )
    .await?;
    let connection = get_connection_by_id(&state, company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.oauth_connected",
        &actor,
        "tool_connection",
        connection_id,
        json!({
            "grantId": grant_id,
            "subjectType": if subject_user_id.is_some() { "user" } else { "workspace" },
            "scopeCount": token.scope.len(),
            "hasRefreshToken": token.refresh_token.is_some(),
            "actorUserId": user_id,
        }),
    )
    .await;
    if headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|part| part.trim() == "text/html"))
    {
        let issue_prefix = sqlx::query_scalar::<_, String>(
            "SELECT issue_prefix FROM companies WHERE id = $1",
        )
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %company_id, "Failed to load company issue prefix for OAuth redirect");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let location = format!(
            "/{}/apps/{}/setup?oauth=connected",
            issue_prefix, connection_id
        );
        let mut response = StatusCode::SEE_OTHER.into_response();
        response.headers_mut().insert(
            header::LOCATION,
            HeaderValue::from_str(&location).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        );
        return Ok(response);
    }
    Ok((
        StatusCode::OK,
        Json(json!({
            "connectionId": connection_id,
            "grantId": grant_id,
            "status": "connected",
            "connection": connection_json(&connection),
        })),
    )
        .into_response())
}

/// POST /companies/:cid/tools/connections —— 创建连接。
#[derive(Debug, Deserialize)]
struct CreateConnectionRequest {
    #[serde(rename = "applicationId")]
    application_id: Option<Uuid>,
    #[serde(rename = "applicationName")]
    application_name: Option<String>,
    name: String,
    transport: Option<String>,
    #[serde(rename = "authKind")]
    auth_kind: Option<String>,
    ownership: Option<String>,
    status: Option<String>,
    #[serde(rename = "connectionKind")]
    connection_kind: Option<String>,
    config: Option<Value>,
    #[serde(rename = "transportConfig")]
    transport_config: Option<Value>,
    #[serde(rename = "credentialRefs")]
    credential_refs: Option<Value>,
    #[serde(rename = "credentialSecretRefs")]
    credential_secret_refs: Option<Value>,
    enabled: Option<bool>,
}
async fn create_company_tool_connection(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateConnectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let name = request.name.trim();
    let transport = request.transport.as_deref().unwrap_or("");
    let auth_kind = request.auth_kind.as_deref().unwrap_or("none");
    let ownership = request.ownership.as_deref().unwrap_or("customer");
    let status = request.status.as_deref().unwrap_or("draft");
    let connection_kind = request.connection_kind.as_deref().unwrap_or("managed");
    if name.is_empty()
        || name.chars().count() > 160
        || !matches!(transport, "mcp_remote" | "rest_api" | "local_stdio")
        || !matches!(auth_kind, "oauth" | "api_key" | "none")
        || !matches!(ownership, "platform_shared" | "platform_provisioned" | "customer" | "dcr")
        || !matches!(status, "draft" | "active" | "disabled" | "archived")
        || !matches!(connection_kind, "managed" | "delegated" | "self_hosted")
        || request
            .credential_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
        || request
            .credential_secret_refs
            .as_ref()
            .is_some_and(|refs| !refs.is_array())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let config = request
        .config
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let transport_config = request
        .transport_config
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!({}));
    let credential_refs = request
        .credential_refs
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!([]));
    let credential_secret_refs = request
        .credential_secret_refs
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!([]));
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!("Failed to start tool connection transaction: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let application_id = if let Some(application_id) = request.application_id {
        let application_type = sqlx::query_scalar::<_, Option<String>>(
            "SELECT type FROM tool_applications WHERE id = $1 AND company_id = $2",
        )
        .bind(application_id)
        .bind(company_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load tool connection application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .flatten()
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        if transport == "mcp_remote" && application_type != "mcp_http"
            || transport == "local_stdio" && application_type != "mcp_stdio"
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        application_id
    } else {
        let application_name = request
            .application_name
            .as_deref()
            .unwrap_or(name)
            .trim();
        if application_name.is_empty() || application_name.chars().count() > 160 {
            return Err(StatusCode::BAD_REQUEST);
        }
        let application_id = Uuid::new_v4();
        let application_key = normalize_tool_application_key(application_name);
        sqlx::query(
            "INSERT INTO tool_applications
                (id, company_id, application_key, name, type, status, metadata)
             VALUES ($1, $2, $3, $4, $5, 'active', '{}')",
        )
        .bind(application_id)
        .bind(company_id)
        .bind(application_key)
        .bind(application_name)
        .bind(if transport == "local_stdio" {
            "mcp_stdio"
        } else {
            "mcp_http"
        })
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                return StatusCode::CONFLICT;
            }
            tracing::error!("Failed to create tool connection application: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        application_id
    };
    let id = Uuid::new_v4();
    let uid = format!(
        "{}-{}",
        normalize_tool_application_key(name),
        id.simple()
    );
    let row = sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, tool_type,
             connection_kind, ownership, transport, auth_kind, status, enabled,
             config, transport_config, credential_refs, credential_secret_refs,
             created_by_user_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 $13, $14, $15, $16, $17)
         RETURNING *",
    )
    .bind(id)
    .bind(company_id)
    .bind(application_id)
    .bind(name)
    .bind(uid)
    .bind(transport)
    .bind(connection_kind)
    .bind(ownership)
    .bind(transport)
    .bind(auth_kind)
    .bind(status)
    .bind(request.enabled.unwrap_or(false))
    .bind(config)
    .bind(transport_config)
    .bind(credential_refs)
    .bind(credential_secret_refs)
    .bind(actor.principal_id().map(|id| id.to_string()))
    .fetch_one(&mut *transaction)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            return StatusCode::CONFLICT;
        }
        tracing::error!("Failed to create tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction.commit().await.map_err(|e| {
        tracing::error!("Failed to commit tool connection: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_connection.created",
        &actor,
        "tool_connection",
        id,
        json!({ "transport": transport, "applicationId": application_id }),
    )
    .await;
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "installed": true })))
}
async fn smoke_tool_example(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _example_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "smoke": "passed" })))
}

/// POST /companies/:cid/tools/apps/connect 与 /apps/:app_id/finish —— mock。
async fn connect_tool_app(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "connected": true })))
}
async fn finish_tool_app(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _app_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "reordered": true })))
}

/// POST /companies/:cid/tools/policy/test —— mock。
async fn test_tool_policy(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "matches": true })))
}

fn empty_json_array() -> Value {
    json!([])
}

fn is_safe_stdio_template_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
}

fn is_safe_env_key(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// POST /companies/:cid/tools/stdio-templates —— 创建管理员 stdio 模板。
#[derive(Debug, Deserialize)]
struct CreateStdioTemplateRequest {
    #[serde(rename = "templateId", alias = "template_key")]
    template_id: String,
    name: String,
    description: Option<String>,
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(rename = "envKeys", alias = "env_keys", default)]
    env_keys: Vec<String>,
    #[serde(default = "empty_json_array")]
    tools: Value,
}

async fn create_stdio_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateStdioTemplateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    let template_id = request.template_id.trim().to_string();
    let name = request.name.trim().to_string();
    let command = request.command.trim().to_string();
    let arg_count = request.args.len();
    let env_key_count = request.env_keys.len();
    let tool_count = request.tools.as_array().map_or(0, Vec::len);
    if template_id.is_empty()
        || template_id.len() > 160
        || !is_safe_stdio_template_key(&template_id)
        || name.is_empty()
        || name.len() > 160
        || command.is_empty()
        || command.len() > 2000
        || request.args.len() > 100
        || request.args.iter().any(|arg| arg.len() > 2000)
        || request.env_keys.len() > 200
        || request
            .env_keys
            .iter()
            .any(|key| key.len() > 160 || !is_safe_env_key(key))
        || !request.tools.is_array()
        || request.tools.as_array().is_some_and(|tools| tools.len() > 500)
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    use sqlx::Row;
    let row = sqlx::query(
        "INSERT INTO tool_stdio_command_templates
            (company_id, template_key, name, description, status, command, args, env_keys, tools,
             created_by_agent_id, created_by_user_id)
         VALUES ($1, $2, $3, $4, 'active', $5, $6, $7, $8, $9, $10)
         RETURNING *",
    )
    .bind(company_id)
    .bind(template_id)
    .bind(name)
    .bind(request.description)
    .bind(command)
    .bind(json!(request.args))
    .bind(json!(request.env_keys))
    .bind(request.tools)
    .bind(match &actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(*agent_id),
        _ => None,
    })
    .bind(match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id.to_string()),
        _ => None,
    })
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to create stdio template");
        if e.as_database_error()
            .and_then(|database_error| database_error.code())
            .as_deref()
            == Some("23505")
        {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    let template = stdio_template_json(&row);
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_stdio_command_template.created",
        &actor,
        "tool_stdio_command_template",
        row.get("id"),
        json!({
            "templateId": template.get("templateId"),
            "command": template.get("command"),
            "argCount": arg_count,
            "envKeyCount": env_key_count,
            "toolCount": tool_count,
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(template)))
}

#[derive(Debug, Deserialize, Default)]
struct DisableStdioTemplateRequest {
    reason: Option<String>,
}

/// POST /companies/:cid/tools/stdio-templates/:template_id/disable —— 禁用管理员模板。
async fn disable_stdio_template(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, template_id)): Path<(Uuid, String)>,
    Json(request): Json<DisableStdioTemplateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_permission(&state, &actor, company_id, PermissionKey::TOOLS_ADMIN).await?;
    if request.reason.as_deref().is_some_and(|reason| reason.len() > 1000) {
        return Err(StatusCode::BAD_REQUEST);
    }
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE tool_stdio_command_templates
            SET status = 'disabled', disabled_at = COALESCE(disabled_at, NOW()), updated_at = NOW()
          WHERE company_id = $1 AND template_key = $2
         RETURNING *",
    )
    .bind(company_id)
    .bind(template_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, %company_id, "Failed to disable stdio template");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;
    let template = stdio_template_json(&row);
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "tool_stdio_command_template.disabled",
        &actor,
        "tool_stdio_command_template",
        row.get("id"),
        json!({
            "templateId": template.get("templateId"),
            "reason": request.reason,
        }),
    )
    .await;
    Ok(Json(template))
}

/// POST /companies/:cid/tools/trust-rules/:rule_id/revoke —— mock（204）。
async fn revoke_trust_rule(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _rule_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /companies/:cid/tools/runtime-slots/:slot_id/stop|restart —— mock。
async fn stop_runtime_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(json!({ "stopped": true })))
}
async fn restart_runtime_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _slot_id)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
    Ok(Json(json!({ "restarted": true })))
}

fn connection_config_roots(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
) -> Vec<Value> {
    use sqlx::Row;
    let configs = [
        row.get::<Option<Value>, _>("config")
            .unwrap_or_else(|| json!({})),
        row.get::<Option<Value>, _>("transport_config")
            .unwrap_or_else(|| json!({})),
    ];
    let mut roots = Vec::with_capacity(configs.len() * (sections.len() + 1));
    for config in &configs {
        for section in sections {
            if let Some(value) = config.get(*section) {
                roots.push(value.clone());
            }
        }
        roots.push(config.clone());
    }
    roots
}

fn connection_config_string_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<String> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                root.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
        })
}

fn connection_config_string(row: &sqlx::postgres::PgRow, keys: &[&str]) -> Option<String> {
    connection_config_string_in_sections(row, &["oauth"], keys)
}

fn config_scope_values(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        Value::String(value) => value
            .split_whitespace()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn connection_config_scope_strings(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Vec<String> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                let values = root.get(*key).map(config_scope_values)?;
                (!values.is_empty()).then_some(values)
            })
        })
        .unwrap_or_default()
}

fn connection_config_strings(row: &sqlx::postgres::PgRow, keys: &[&str]) -> Vec<String> {
    connection_config_scope_strings(row, &["oauth"], keys)
}

fn connection_config_bool_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<bool> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| keys.iter().find_map(|key| root.get(*key).and_then(Value::as_bool)))
}

fn connection_config_i64_in_sections(
    row: &sqlx::postgres::PgRow,
    sections: &[&str],
    keys: &[&str],
) -> Option<i64> {
    connection_config_roots(row, sections)
        .iter()
        .find_map(|root| {
            keys.iter().find_map(|key| {
                root.get(*key).and_then(|value| match value {
                    Value::Number(value) => value.as_i64(),
                    Value::String(value) => value.trim().parse().ok(),
                    _ => None,
                })
            })
        })
}

fn normalize_token_scopes(input: Option<AgentConnectionTokenScope>) -> Result<Vec<String>, StatusCode> {
    let values = match input {
        None => Vec::new(),
        Some(AgentConnectionTokenScope::String(value)) => value
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect(),
        Some(AgentConnectionTokenScope::Array(values)) => values,
    };
    if values.len() > 100
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.trim().chars().count() > 240)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut unique = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let value = value.trim().to_string();
        if seen.insert(value.clone()) {
            unique.push(value);
        }
    }
    Ok(unique)
}

fn connection_token_path(row: &sqlx::postgres::PgRow) -> &'static str {
    let sections = &["tokenBroker", "token_broker", "broker"];
    if let Some(path) = connection_config_string_in_sections(
        row,
        sections,
        &["path", "tokenPath", "token_path"],
    ) {
        return match path.as_str() {
            "exchange" => "exchange",
            "oauth_access" | "oauthAccess" => "oauth_access",
            "static" => "static",
            _ => "static",
        };
    }
    if connection_config_string_in_sections(row, sections, &["tokenUrl", "token_url"])
        .is_some()
        || connection_config_string_in_sections(row, &[], &["tokenExchangeUrl", "token_exchange_url"])
            .is_some()
    {
        "exchange"
    } else {
        "static"
    }
}

fn connection_token_parent_scopes(row: &sqlx::postgres::PgRow) -> Vec<String> {
    connection_config_scope_strings(
        row,
        &["tokenBroker", "token_broker", "broker", "oauth"],
        &["parentScopes", "parent_scopes", "scopes", "scope"],
    )
}

fn connection_token_default_scopes(row: &sqlx::postgres::PgRow) -> Vec<String> {
    connection_config_scope_strings(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["defaultScopes", "default_scopes"],
    )
}

fn token_scopes_subset(requested: &[String], parent: &[String]) -> bool {
    requested.is_empty() || (!parent.is_empty() && requested.iter().all(|scope| parent.contains(scope)))
}

fn bounded_token_ttl(row: &sqlx::postgres::PgRow, requested: Option<i64>) -> Result<i32, StatusCode> {
    if requested.is_some_and(|value| !(1..=86_400).contains(&value)) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let configured = connection_config_i64_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["defaultTtlSeconds", "default_ttl_seconds", "ttlSeconds", "ttl_seconds"],
    )
    .unwrap_or(900);
    Ok(requested
        .unwrap_or(configured)
        .clamp(1, 900) as i32)
}

#[derive(Debug)]
struct ConnectionTokenExchangeError {
    status: StatusCode,
    outcome: &'static str,
    code: &'static str,
    metadata: Value,
}

impl ConnectionTokenExchangeError {
    fn configuration(code: &'static str) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            outcome: "failure",
            code,
            metadata: json!({}),
        }
    }

    fn credential(code: &'static str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            outcome: "denied",
            code,
            metadata: json!({}),
        }
    }

    fn internal(code: &'static str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            outcome: "failure",
            code,
            metadata: json!({}),
        }
    }

    fn upstream(status: StatusCode, code: &'static str, metadata: Value) -> Self {
        Self {
            status,
            outcome: "upstream_error",
            code,
            metadata,
        }
    }
}

#[derive(Debug)]
struct BrokerTokenExchange {
    token: String,
    token_type: String,
    expires_at: DateTime<Utc>,
    scope: Vec<String>,
}

fn broker_reference_secret_id(reference: &Value) -> Option<Uuid> {
    reference
        .get("secretId")
        .or_else(|| reference.get("secret_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn broker_reference_version(reference: &Value) -> String {
    reference
        .get("versionSelector")
        .or_else(|| reference.get("version_selector"))
        .or_else(|| reference.get("version"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest")
        .to_string()
}

fn broker_reference_name(reference: &Value) -> Option<&str> {
    reference
        .get("name")
        .or_else(|| reference.get("key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_oauth_credential_path(path: &str) -> bool {
    matches!(
        path.to_ascii_lowercase().as_str(),
        "oauth.access_token"
            | "oauth.access-token"
            | "oauth.accesstoken"
            | "oauth.refresh_token"
            | "oauth.refresh-token"
            | "oauth.refreshtoken"
            | "oauth.client_secret"
            | "oauth.client-secret"
            | "oauth.clientsecret"
    )
}

fn is_broker_secret_reference(reference: &Value) -> bool {
    broker_reference_secret_id(reference).is_some()
        && !credential_ref_path(reference).is_some_and(is_oauth_credential_path)
}

fn broker_parent_reference(
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
) -> Option<Value> {
    use sqlx::Row;
    let grant_refs = grant_secret_refs.as_array().cloned().unwrap_or_default();
    let secret_refs = if !grant_refs.is_empty() {
        grant_refs
    } else if subject_user_id.is_none() {
        row.get::<Option<Value>, _>("credential_secret_refs")
            .unwrap_or_else(|| json!([]))
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let configured_path = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["parentCredentialConfigPath", "credentialConfigPath", "secretConfigPath"],
    );
    if let Some(configured_path) = configured_path.as_deref() {
        if let Some(reference) = secret_refs.iter().find(|reference| {
            is_broker_secret_reference(reference)
                && credential_ref_path(reference) == Some(configured_path)
        }) {
            return Some(reference.clone());
        }
    } else {
        for preferred_path in ["credentials.deploy_token", "pages.deploy_token"] {
            if let Some(reference) = secret_refs.iter().find(|reference| {
                is_broker_secret_reference(reference)
                    && credential_ref_path(reference) == Some(preferred_path)
            }) {
                return Some(reference.clone());
            }
        }
        if let Some(reference) = secret_refs
            .iter()
            .find(|reference| is_broker_secret_reference(reference))
        {
            return Some(reference.clone());
        }
    }

    if subject_user_id.is_some() {
        return None;
    }
    let configured_name = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["parentCredentialName", "credentialName"],
    );
    let credential_refs = row
        .get::<Option<Value>, _>("credential_refs")
        .unwrap_or_else(|| json!([]));
    let credential_refs = credential_refs.as_array()?;
    if let Some(configured_name) = configured_name.as_deref() {
        credential_refs
            .iter()
            .find(|reference| {
                broker_reference_secret_id(reference).is_some()
                    && broker_reference_name(reference) == Some(configured_name)
            })
            .cloned()
    } else {
        credential_refs
            .iter()
            .find(|reference| broker_reference_secret_id(reference).is_some())
            .cloned()
    }
}

async fn resolve_broker_parent_credential(
    pool: &sqlx::PgPool,
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
) -> Result<String, ConnectionTokenExchangeError> {
    use sqlx::Row;
    let reference = broker_parent_reference(row, grant_secret_refs, subject_user_id)
        .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_missing"))?;
    let secret_id = broker_reference_secret_id(&reference)
        .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_invalid"))?;
    let version = broker_reference_version(&reference);
    let secret_row = if version.eq_ignore_ascii_case("latest") {
        sqlx::query(
            "SELECT v.material, s.provider
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.status = 'current' AND v.revoked_at IS NULL
              ORDER BY v.version DESC
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(row.get::<Uuid, _>("company_id"))
        .fetch_optional(pool)
        .await
    } else {
        let version_number = version
            .parse::<i32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ConnectionTokenExchangeError::configuration("parent_credential_version_invalid"))?;
        sqlx::query(
            "SELECT v.material, s.provider
               FROM company_secret_versions v
               JOIN company_secrets s ON s.id = v.secret_id
              WHERE s.id = $1 AND s.company_id = $2
                AND s.status = 'active' AND s.deleted_at IS NULL
                AND v.version = $3 AND v.revoked_at IS NULL
              LIMIT 1",
        )
        .bind(secret_id)
        .bind(row.get::<Uuid, _>("company_id"))
        .bind(version_number)
        .fetch_optional(pool)
        .await
    }
    .map_err(|error| {
        tracing::error!(%error, %secret_id, "Failed to resolve broker parent credential");
        ConnectionTokenExchangeError::internal("parent_credential_resolution_failed")
    })?;
    let Some(secret_row) = secret_row else {
        return Err(ConnectionTokenExchangeError::credential("credential_revoked"));
    };
    let provider: String = secret_row.get("provider");
    if provider != "local_encrypted" && provider != "local" {
        return Err(ConnectionTokenExchangeError::configuration(
            "parent_credential_provider_unsupported",
        ));
    }
    let material: Value = secret_row.get("material");
    let value = decrypt_secret_material(&material).map_err(|error| {
        tracing::warn!(%error, %secret_id, "Failed to decrypt broker parent credential");
        ConnectionTokenExchangeError::credential("credential_revoked")
    })?;
    if value.trim().is_empty() {
        return Err(ConnectionTokenExchangeError::credential("credential_revoked"));
    }
    Ok(value)
}

fn broker_exchange_url(row: &sqlx::postgres::PgRow) -> Result<Url, ConnectionTokenExchangeError> {
    let configured = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["tokenUrl", "token_url", "exchangeTokenUrl", "exchange_token_url"],
    )
    .or_else(|| {
        connection_config_string_in_sections(
            row,
            &[],
            &["tokenExchangeUrl", "token_exchange_url", "pagesTokenExchangeUrl"],
        )
    })
    .ok_or_else(|| ConnectionTokenExchangeError::configuration("exchange_url_missing"))?;
    validate_oauth_endpoint(&configured)
        .map_err(|_| ConnectionTokenExchangeError::configuration("exchange_url_invalid"))
}

fn broker_response_expires_at(
    payload: &Value,
    now: DateTime<Utc>,
    ttl_seconds: i32,
) -> Result<DateTime<Utc>, ConnectionTokenExchangeError> {
    let configured = payload
        .get("expiresAt")
        .or_else(|| payload.get("expires_at"))
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let expires_in = payload
        .get("expiresIn")
        .or_else(|| payload.get("expires_in"))
        .and_then(|value| match value {
            Value::Number(value) => value.as_i64(),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
        .filter(|value| *value > 0)
        .map(|value| now + Duration::seconds(value));
    let maximum = now + Duration::seconds(i64::from(ttl_seconds));
    let candidate = configured.or(expires_in).unwrap_or(maximum);
    if candidate <= now {
        return Err(ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_token_invalid",
            json!({ "reason": "token_expired" }),
        ));
    }
    let bounded = std::cmp::min(candidate, maximum);
    Ok(std::cmp::max(bounded, now + Duration::seconds(1)))
}

async fn exchange_connection_token(
    row: &sqlx::postgres::PgRow,
    grant_secret_refs: &Value,
    subject_user_id: Option<&str>,
    context: &ActiveAgentRunContext,
    issued_scope: &[String],
    ttl_seconds: i32,
    pool: &sqlx::PgPool,
) -> Result<BrokerTokenExchange, ConnectionTokenExchangeError> {
    use sqlx::Row;
    let parent_token = resolve_broker_parent_credential(
        pool,
        row,
        grant_secret_refs,
        subject_user_id,
    )
    .await?;
    let endpoint = broker_exchange_url(row)?;
    let protocol = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["protocol", "exchangeProtocol", "exchange_protocol"],
    )
    .unwrap_or_else(|| "generic".to_string())
    .to_ascii_lowercase();
    if !matches!(protocol.as_str(), "generic" | "json" | "pages" | "rfc8693" | "rfc_8693") {
        return Err(ConnectionTokenExchangeError::configuration(
            "exchange_protocol_unsupported",
        ));
    }
    let audience = connection_config_string_in_sections(
        row,
        &["tokenBroker", "token_broker", "broker"],
        &["audience"],
    );
    let actor = json!({
        "type": "agent",
        "id": context.agent_id,
        "runId": context.run_id,
        "onBehalfOf": context.responsible_user_id.as_deref().map(|id| format!("user:{id}")),
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| {
            tracing::error!(%error, "Failed to build connection token exchange client");
            ConnectionTokenExchangeError::internal("exchange_client_unavailable")
        })?;
    let response = if matches!(protocol.as_str(), "rfc8693" | "rfc_8693") {
        let actor_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&actor).map_err(|_| {
                ConnectionTokenExchangeError::internal("exchange_actor_serialization_failed")
            })?,
        );
        let mut form = vec![
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange".to_string(),
            ),
            ("subject_token", parent_token.clone()),
            (
                "subject_token_type",
                connection_config_string_in_sections(
                    row,
                    &["tokenBroker", "token_broker", "broker"],
                    &["subjectTokenType", "subject_token_type"],
                )
                .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:access_token".to_string()),
            ),
            ("scope", issued_scope.join(" ")),
            ("requested_token_type", connection_config_string_in_sections(
                row,
                &["tokenBroker", "token_broker", "broker"],
                &["requestedTokenType", "requested_token_type"],
            )
            .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:access_token".to_string())),
            ("actor_token", actor_token),
            ("actor_token_type", connection_config_string_in_sections(
                row,
                &["tokenBroker", "token_broker", "broker"],
                &["actorTokenType", "actor_token_type"],
            )
            .unwrap_or_else(|| "urn:ietf:params:oauth:token-type:jwt".to_string())),
        ];
        if let Some(audience) = audience.as_deref() {
            form.push(("audience", audience.to_string()));
        }
        client.post(endpoint).form(&form).send().await
    } else {
        let mut body = if protocol == "pages" {
            let namespace = issued_scope
                .first()
                .and_then(|scope| scope.strip_prefix("pages:publish:ns/"));
            if let Some(namespace) = namespace {
                json!({
                    "namespace": namespace,
                    "ttlSeconds": ttl_seconds,
                    "actions": ["publish"],
                    "actor": actor,
                })
            } else {
                json!({
                    "scope": issued_scope,
                    "ttlSeconds": ttl_seconds,
                    "actor": actor,
                })
            }
        } else {
            json!({
                "scope": issued_scope,
                "ttlSeconds": ttl_seconds,
                "actor": actor,
            })
        };
        if let Some(audience) = audience {
            body["audience"] = json!(audience);
        }
        client
            .post(endpoint)
            .bearer_auth(parent_token)
            .json(&body)
            .send()
            .await
    }
    .map_err(|error| {
        tracing::warn!(%error, connection_id = %row.get::<Uuid, _>("id"), "Connection token exchange request failed");
        ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            json!({}),
        )
    })?;
    let upstream_status = response.status();
    let payload = response.json::<Value>().await.map_err(|error| {
        tracing::warn!(%error, %upstream_status, "Connection token exchange returned invalid JSON");
        ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            json!({ "upstreamStatus": upstream_status.as_u16() }),
        )
    })?;
    if !upstream_status.is_success() {
        let upstream_code = payload
            .get("code")
            .or_else(|| payload.get("error"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(128).collect::<String>());
        let credential_revoked = matches!(upstream_status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
            || upstream_code.as_deref() == Some("parent_revoked");
        return Err(ConnectionTokenExchangeError::upstream(
            if credential_revoked {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_GATEWAY
            },
            if credential_revoked {
                "credential_revoked"
            } else {
                "upstream_error"
            },
            json!({
                "upstreamStatus": upstream_status.as_u16(),
                "upstreamCode": upstream_code,
            }),
        ));
    }
    let token = payload
        .get("token")
        .or_else(|| payload.get("access_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 16_384)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ConnectionTokenExchangeError::upstream(
                StatusCode::BAD_GATEWAY,
                "upstream_token_missing",
                json!({ "upstreamStatus": upstream_status.as_u16() }),
            )
        })?;
    let token_type = payload
        .get("tokenType")
        .or_else(|| payload.get("token_type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 128)
        .unwrap_or("Bearer")
        .to_string();
    let response_scope = payload
        .get("scope")
        .map(config_scope_values)
        .unwrap_or_default();
    if !response_scope.is_empty() && !token_scopes_subset(&response_scope, issued_scope) {
        return Err(ConnectionTokenExchangeError::upstream(
            StatusCode::BAD_GATEWAY,
            "upstream_scope_exceeds_requested",
            json!({ "scopeCount": response_scope.len() }),
        ));
    }
    let scope = if response_scope.is_empty() {
        issued_scope.to_vec()
    } else {
        response_scope
    };
    let now = Utc::now();
    let expires_at = broker_response_expires_at(&payload, now, ttl_seconds)?;
    Ok(BrokerTokenExchange {
        token,
        token_type,
        expires_at,
        scope,
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AgentConnectionTokenScope {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AgentConnectionTokenSubject {
    #[serde(rename = "app")]
    App,
    #[serde(rename = "user")]
    User {
        #[serde(rename = "userId")]
        user_id: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentConnectionTokenRequest {
    subject: Option<AgentConnectionTokenSubject>,
    scope: Option<AgentConnectionTokenScope>,
    #[serde(rename = "requestedTtlSeconds")]
    requested_ttl_seconds: Option<i64>,
    #[serde(rename = "grantId")]
    grant_id: Option<Uuid>,
}

async fn record_connection_token_issuance(
    state: &AppState,
    context: &ActiveAgentRunContext,
    row: &sqlx::postgres::PgRow,
    path: &str,
    requested_scope: &[String],
    issued_scope: &[String],
    ttl_seconds: Option<i32>,
    expires_at: Option<DateTime<Utc>>,
    token_hash: Option<&str>,
    outcome: &str,
    error_code: Option<&str>,
    metadata: Value,
) -> Result<(), StatusCode> {
    use sqlx::Row;
    sqlx::query(
        "INSERT INTO connection_token_issuances
            (company_id, application_id, connection_id, agent_id, run_id,
             issue_id, project_id, responsible_user_id, path, requested_scope,
             issued_scope, ttl_seconds, expires_at, token_hash, outcome,
             error_code, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                 $14, $15, $16, $17)",
    )
    .bind(context.company_id)
    .bind(row.get::<Option<Uuid>, _>("application_id"))
    .bind(row.get::<Uuid, _>("id"))
    .bind(context.agent_id)
    .bind(context.run_id)
    .bind(context.issue_id)
    .bind(context.project_id)
    .bind(context.responsible_user_id.as_deref())
    .bind(path)
    .bind(json!(requested_scope))
    .bind(json!(issued_scope))
    .bind(ttl_seconds)
    .bind(expires_at)
    .bind(token_hash)
    .bind(outcome)
    .bind(error_code)
    .bind(metadata)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(%e, "Failed to persist connection token issuance");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(())
}
fn configured_oauth_redirect_uri(row: &sqlx::postgres::PgRow) -> Option<String> {
    if let Some(value) = connection_config_string(
        row,
        &["redirectUri", "redirect_uri", "clientRedirectUri", "client_redirect_uri"],
    ) {
        return Some(value);
    }
    let configured = [
        "PAPERCLIP_PUBLIC_URL",
        "PAPERCLIP_AUTH_PUBLIC_BASE_URL",
        "BETTER_AUTH_URL",
        "BETTER_AUTH_BASE_URL",
    ]
    .into_iter()
    .find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })?;
    let mut url = Url::parse(&configured).ok()?;
    url.set_path("/api/tools/oauth/callback");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

fn random_oauth_token(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(code_verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartAgentConnectionAuthorizationRequest {
    #[serde(rename = "subjectUserId")]
    subject_user_id: String,
    scopes: Option<Vec<String>>,
    #[serde(rename = "returnTo")]
    return_to: Option<String>,
}

/// POST /agents/me/connections/:connection_id/start-authorization —— 创建 OAuth/PKCE 状态。
async fn start_agent_connection_auth(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    request: Option<Json<StartAgentConnectionAuthorizationRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let context = load_active_agent_run_context(&state, &actor).await?;
    let Json(request) = request.ok_or(StatusCode::BAD_REQUEST)?;
    let subject_user_id = request.subject_user_id.trim().to_string();
    if subject_user_id.is_empty() || subject_user_id.chars().count() > 256 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if context.responsible_user_id.as_deref() != Some(subject_user_id.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = get_connection_by_id(&state, context.company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    if row.get::<String, _>("status") == "archived" {
        return Err(StatusCode::CONFLICT);
    }
    if row.get::<String, _>("auth_kind") != "oauth" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scopes = request
        .scopes
        .unwrap_or_else(|| connection_config_strings(&row, &["scopes", "scope"]));
    if scopes.len() > 100 || scopes.iter().any(|scope| scope.chars().count() > 200) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let return_to = request
        .return_to
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if return_to
        .as_deref()
        .is_some_and(|value| value.chars().count() > 2000)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let authorization_endpoint = connection_config_string(
        &row,
        &["authorizationUrl", "authorization_url", "authorizeUrl", "authorize_url"],
    )
    .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let client_id = connection_config_string(&row, &["clientId", "client_id"])
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let redirect_uri = configured_oauth_redirect_uri(&row)
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

    let state_token = random_oauth_token(32);
    let code_verifier = random_oauth_token(48);
    let mut authorization_url =
        Url::parse(&authorization_endpoint).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    authorization_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &pkce_challenge(&code_verifier))
        .append_pair("code_challenge_method", "S256");
    if !scopes.is_empty() {
        authorization_url
            .query_pairs_mut()
            .append_pair("scope", &scopes.join(" "));
    }
    let expires_at = Utc::now() + Duration::minutes(10);
    let requested_scopes = (!scopes.is_empty()).then(|| json!(scopes));
    let mut transaction = state.pool.begin().await.map_err(|e| {
        tracing::error!(%e, "Failed to start OAuth authorization transaction");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query("DELETE FROM tool_oauth_states WHERE expires_at <= NOW()")
        .execute(&mut *transaction)
        .await
        .map_err(|e| {
            tracing::error!(%e, "Failed to clean expired OAuth authorization states");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    sqlx::query(
        "INSERT INTO tool_oauth_states
            (state, company_id, connection_id, code_verifier,
             created_by_actor_type, created_by_actor_id, subject_user_id,
             requested_scopes, return_to, issue_id, expires_at)
         VALUES ($1, $2, $3, $4, 'agent', $5, $6, $7, $8, $9, $10)",
    )
    .bind(&state_token)
    .bind(context.company_id)
    .bind(connection_id)
    .bind(&code_verifier)
    .bind(context.agent_id.to_string())
    .bind(&subject_user_id)
    .bind(requested_scopes)
    .bind(return_to.as_deref())
    .bind(context.issue_id)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await
    .map_err(|e| {
        tracing::error!(%e, %connection_id, "Failed to persist OAuth authorization state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction.commit().await.map_err(|e| {
        tracing::error!(%e, "Failed to commit OAuth authorization state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({ "url": authorization_url.to_string() })))
}

/// POST /agents/me/connections/:connection_id/token —— Agent token broker。
async fn agent_connection_token(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    headers: HeaderMap,
    request: Option<Json<AgentConnectionTokenRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let context = load_active_agent_run_context(&state, &actor).await?;
    if let Some(header_run_id) = headers
        .get("x-paperclip-run-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if header_run_id != context.run_id.to_string() {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let Json(request) = request.ok_or(StatusCode::BAD_REQUEST)?;
    let row = get_connection_by_id(&state, context.company_id, connection_id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let requested_scope = normalize_token_scopes(request.scope)?;
    let subject_user_id = match request.subject {
        None | Some(AgentConnectionTokenSubject::App) => None,
        Some(AgentConnectionTokenSubject::User { user_id }) => {
            let user_id = user_id.trim().to_string();
            if user_id.is_empty() || user_id.chars().count() > 500 {
                return Err(StatusCode::BAD_REQUEST);
            }
            if context.responsible_user_id.as_deref() != Some(user_id.as_str()) {
                return Err(StatusCode::FORBIDDEN);
            }
            Some(user_id)
        }
    };
    let path = connection_token_path(&row);
    let ttl_seconds = bounded_token_ttl(&row, request.requested_ttl_seconds)?;
    let parent_scope = connection_token_parent_scopes(&row);
    let issued_scope = if requested_scope.is_empty() {
        let defaults = connection_token_default_scopes(&row);
        if defaults.is_empty() {
            parent_scope.clone()
        } else {
            defaults
        }
    } else {
        requested_scope.clone()
    };
    if !token_scopes_subset(&issued_scope, &parent_scope) {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            Some(ttl_seconds),
            None,
            None,
            "denied",
            Some("scope_exceeds_parent"),
            json!({ "parentScopeCount": parent_scope.len() }),
        )
        .await?;
        return Err(StatusCode::FORBIDDEN);
    }
    if row.get::<String, _>("status") != "active" || !row.get::<bool, _>("enabled") {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("connection_not_active"),
            json!({
                "connectionStatus": row.get::<String, _>("status"),
                "enabled": row.get::<bool, _>("enabled")
            }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }
    let health_status = row.get::<String, _>("health_status");
    // Paperclip: TOOL_CONNECTION_ATTENTION_HEALTH_STATUSES is the single source
    // of truth for "this app needs the user's attention". The previous ad-hoc
    // list omitted `degraded` and included the non-canonical `unhealthy`.
    if services::tool_access_contract::is_tool_connection_attention_health(&health_status) {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("credential_revoked"),
            json!({ "healthStatus": health_status }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }
    if !connection_config_bool_in_sections(
        &row,
        &["tokenBroker", "token_broker", "broker"],
        &["enabled"],
    )
    .unwrap_or(false)
    {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some("broker_not_enabled"),
            json!({}),
        )
        .await?;
        return Err(StatusCode::FORBIDDEN);
    }

    let grant = if let Some(grant_id) = request.grant_id {
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE id = $1 AND company_id = $2 AND connection_id = $3
                AND kind = $4
                AND ($5::text IS NULL OR subject_user_id = $5)",
        )
        .bind(grant_id)
        .bind(context.company_id)
        .bind(connection_id)
        .bind(if subject_user_id.is_some() { "user" } else { "workspace" })
        .bind(subject_user_id.as_deref())
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else if let Some(subject_user_id) = subject_user_id.as_deref() {
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'user' AND subject_user_id = $3
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .bind(subject_user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load user connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    } else {
        sqlx::query(
            "INSERT INTO connection_grants
                (company_id, connection_id, kind, status, is_default,
                 created_by_agent_id, credential_secret_refs)
             VALUES ($1, $2, 'workspace', 'active', true, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .bind(context.agent_id)
        .bind(
            row.get::<Option<Value>, _>("credential_secret_refs")
                .unwrap_or_else(|| json!([])),
        )
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to ensure workspace connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "SELECT id, status, credential_secret_refs
               FROM connection_grants
              WHERE company_id = $1 AND connection_id = $2
                AND kind = 'workspace' AND is_default = true
              LIMIT 1",
        )
        .bind(context.company_id)
        .bind(connection_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(%e, %connection_id, "Failed to load default connection token grant");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };
    let Some(grant) = grant else {
        let error_code = if subject_user_id.is_some() {
            "user_authorization_required"
        } else {
            "installation_required"
        };
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some(error_code),
            json!({}),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    };
    let grant_id: Uuid = grant.get("id");
    let grant_status: String = grant.get("status");
    if grant_status != "active" {
        let error_code = if grant_status == "needs_reauthorization" {
            "needs_reauthorization"
        } else {
            "grant_revoked"
        };
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            None,
            None,
            None,
            "denied",
            Some(error_code),
            json!({ "grantId": grant_id }),
        )
        .await?;
        return Err(StatusCode::CONFLICT);
    }

    if path == "static" {
        record_connection_token_issuance(
            &state,
            &context,
            &row,
            path,
            &requested_scope,
            &issued_scope,
            Some(ttl_seconds),
            None,
            None,
            "use_env_lease",
            Some("use_env_lease"),
            json!({ "grantId": grant_id }),
        )
        .await?;
        return Ok((StatusCode::CONFLICT, Json(json!({
            "status": "use_env_lease",
            "code": "use_env_lease",
            "connectionId": connection_id,
            "connection": {
                "id": connection_id,
                "uid": row.get::<String, _>("uid")
            },
            "grantId": grant_id,
            "path": "static",
            "message": "This connection uses static credentials. Use an audited environment lease projection instead.",
            "scope": issued_scope,
            "attribution": {
                "agentId": context.agent_id,
                "runId": context.run_id,
                "issueId": context.issue_id,
                "projectId": context.project_id,
                "responsibleUserId": context.responsible_user_id
            }
        }))));
    }

    let grant_secret_refs = grant
        .get::<Option<Value>, _>("credential_secret_refs")
        .unwrap_or_else(|| json!([]));
    if path == "exchange" {
        match exchange_connection_token(
            &row,
            &grant_secret_refs,
            subject_user_id.as_deref(),
            &context,
            &issued_scope,
            ttl_seconds,
            &state.pool,
        )
        .await
        {
            Ok(token) => {
                let effective_ttl = (token.expires_at - Utc::now())
                    .num_seconds()
                    .clamp(1, 900) as i32;
                let token_hash = sha256_hex(&token.token);
                record_connection_token_issuance(
                    &state,
                    &context,
                    &row,
                    path,
                    &requested_scope,
                    &token.scope,
                    Some(effective_ttl),
                    Some(token.expires_at),
                    Some(&token_hash),
                    "success",
                    None,
                    json!({
                        "grantId": grant_id,
                        "tokenType": token.token_type,
                    }),
                )
                .await?;
                sqlx::query(
                    "UPDATE connection_grants
                        SET last_used_at = NOW(), updated_at = NOW()
                      WHERE id = $1 AND company_id = $2",
                )
                .bind(grant_id)
                .bind(context.company_id)
                .execute(&state.pool)
                .await
                .map_err(|error| {
                    tracing::error!(%error, %grant_id, "Failed to update connection grant usage");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                return Ok((
                    StatusCode::OK,
                    Json(json!({
                        "status": "minted",
                        "connectionId": connection_id,
                        "connection": {
                            "id": connection_id,
                            "uid": row.get::<String, _>("uid")
                        },
                        "grantId": grant_id,
                        "path": "exchange",
                        "token": token.token,
                        "tokenType": token.token_type,
                        "expiresAt": token.expires_at,
                        "ttlSeconds": effective_ttl,
                        "scope": token.scope,
                        "attribution": {
                            "agentId": context.agent_id,
                            "runId": context.run_id,
                            "issueId": context.issue_id,
                            "projectId": context.project_id,
                            "responsibleUserId": context.responsible_user_id
                        }
                    })),
                ));
            }
            Err(error) => {
                let ConnectionTokenExchangeError {
                    status,
                    outcome,
                    code,
                    metadata,
                } = error;
                let metadata = match metadata {
                    Value::Object(mut metadata) => {
                        metadata.insert("grantId".to_string(), json!(grant_id));
                        Value::Object(metadata)
                    }
                    _ => json!({ "grantId": grant_id }),
                };
                record_connection_token_issuance(
                    &state,
                    &context,
                    &row,
                    path,
                    &requested_scope,
                    &issued_scope,
                    Some(ttl_seconds),
                    None,
                    None,
                    outcome,
                    Some(code),
                    metadata,
                )
                .await?;
                return Err(status);
            }
        }
    }

    let error_code = if path == "oauth_access" {
        "oauth_access_projection_disabled"
    } else {
        "exchange_not_implemented"
    };
    record_connection_token_issuance(
        &state,
        &context,
        &row,
        path,
        &requested_scope,
        &issued_scope,
        Some(ttl_seconds),
        None,
        None,
        "denied",
        Some(error_code),
        json!({ "grantId": grant_id }),
    )
    .await?;
    Err(StatusCode::UNPROCESSABLE_ENTITY)
}

// ================= Round 3 =================

/// POST /api/tool-connections/:id/health-check —— 对 MCP 做 initialize/tools.list 探测。
async fn connection_health_check(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
            let _ = sqlx::query("UPDATE tool_connections SET health_status = 'failed', health_message = $3, health_checked_at = NOW(), last_error = $3, updated_at = NOW() WHERE id = $1 AND company_id = $2")
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(connection_id): Path<Uuid>,
    Json(request): Json<CreateTestCallRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let company_id = actor_company(&actor)?;
    require_gateway_any_permission(
        &state,
        &actor,
        company_id,
        &[PermissionKey::TOOLS_USE, PermissionKey::TOOLS_MANAGE_CONNECTIONS],
    )
    .await?;
    let connection_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM tool_connections WHERE id = $1 AND company_id = $2
         )",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !connection_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok((
        StatusCode::CREATED,
        Json(
            json!({ "id": Uuid::new_v4(), "connectionId": connection_id, "tool": request.tool, "status": "passed" }),
        ),
    ))
}

/// POST /api/tool-profiles/:id/duplicate —— mock。
async fn duplicate_tool_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    ensure_tool_profile_scope(&state, profile_id, company_id).await?;
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let id = Uuid::new_v4();
    let inserted_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect, tool_name)
         SELECT $1, $2, p.id, 'tool_name',
                CASE WHEN COALESCE($4, true) THEN 'include' ELSE 'exclude' END,
                $5
           FROM tool_profiles AS p
          WHERE p.id = $3 AND p.company_id = $2
         RETURNING id",
    )
    .bind(id)
    .bind(company_id)
    .bind(profile_id)
    .bind(request.enabled)
    .bind(&request.tool)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create tool profile entry: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(id) = inserted_id else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "tool": request.tool, "enabled": request.enabled })),
    ))
}

/// POST /api/tool-profiles/:id/new-tools/review —— mock。
async fn review_profile_new_tools(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_board_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    ensure_tool_profile_scope(&state, profile_id, company_id).await?;
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
    require_board_company_access(&actor, company_id, AccessMode::Write)
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
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
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
    require_gateway_runtime_permission(&state, &actor, company_id).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_audit_cursor_round_trips_without_padding() {
        let cursor = GatewayAuditCursor {
            created_at: Utc::now(),
            id: Uuid::new_v4(),
        };
        let encoded = gateway_audit_cursor_encode(&cursor);
        assert!(!encoded.contains('='));
        assert_eq!(gateway_audit_cursor_decode(&encoded), Some(cursor));
        assert!(gateway_audit_cursor_decode("not-a-cursor").is_none());
    }

    #[test]
    fn gateway_audit_query_helpers_match_paperclip_filters() {
        assert_eq!(gateway_audit_window("1h"), Some(Duration::hours(1)));
        assert_eq!(gateway_audit_window("24h"), Some(Duration::hours(24)));
        assert!(gateway_audit_window("90d").is_none());
        assert_eq!(
            gateway_audit_like_pattern(r"100%_ready\now"),
            r"%100\%\_ready\\now%"
        );
        assert_eq!(
            gateway_audit_outcome("call_completed", Some("allow"), "success"),
            "allowed"
        );
        assert_eq!(
            gateway_audit_outcome("approval_requested", Some("require_approval"), "pending"),
            "asked_first"
        );
        assert_eq!(
            gateway_audit_outcome("call_failed", Some("allow"), "failure"),
            "allowed"
        );
        assert_eq!(
            gateway_audit_outcome("call_denied", Some("deny"), "denied"),
            "blocked"
        );
    }
}
