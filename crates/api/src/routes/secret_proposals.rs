//! Secret Proposals 路由 —— 对齐 Paperclip `server/src/routes/secrets.ts`
//! 的 secret-proposals / agent-secrets 端点。
//!
//! - `POST /agents/me/secret-proposals`：agent 提案（kind=secret|binding）。
//! - `GET  /agents/me/secret-proposals`：agent 查看自己的提案（分页）。
//! - `DELETE /agents/me/secret-proposals/:id`：agent 撤回（withdrawn）。
//! - `GET  /companies/:company_id/secret-proposals`：board 列表（status 过滤）。
//! - `POST /companies/:company_id/secret-proposals/:id/approve`：board 批准
//!   （kind=secret 时创建 company_secret + version）。
//! - `POST /companies/:company_id/secret-proposals/:id/reject`：board 拒绝（需 reason）。
//! - `GET  /agents/me/secrets`：agent 列出已获批准的 secret 元数据。

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use sqlx::Row;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

fn proposal_json(
    row: &sqlx::postgres::PgRow,
) -> serde_json::Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "kind": row.get::<String, _>("kind"),
        "status": row.get::<String, _>("status"),
        "proposedName": row.get::<Option<String>, _>("proposed_name"),
        "proposedKey": row.get::<Option<String>, _>("proposed_key"),
        "proposedDescription": row.get::<Option<String>, _>("proposed_description"),
        "justification": row.get::<String, _>("justification"),
        "valueFingerprintSha256": row.get::<Option<String>, _>("value_fingerprint_sha256"),
        "valueLength": row.get::<Option<i32>, _>("value_length"),
        "secretId": row.get::<Option<Uuid>, _>("secret_id"),
        "targetType": row.get::<Option<String>, _>("target_type"),
        "targetId": row.get::<Option<Uuid>, _>("target_id"),
        "configPath": row.get::<Option<String>, _>("config_path"),
        "proposedByAgentId": row.get::<Uuid, _>("proposed_by_agent_id"),
        "resolvedByUserId": row.get::<Option<String>, _>("resolved_by_user_id"),
        "resolvedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
        "resolutionReason": row.get::<Option<String>, _>("resolution_reason"),
        "createdSecretId": row.get::<Option<Uuid>, _>("created_secret_id"),
        "expiresAt": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

fn agent_context(
    actor: &AuthorizationActor,
) -> Result<(Uuid, Uuid, Option<Uuid>), StatusCode> {
    match actor {
        AuthorizationActor::Agent { agent_id, company_id, run_id, .. } => {
            Ok((*agent_id, *company_id, *run_id))
        }
        _ => Err(StatusCode::FORBIDDEN),
    }
}

#[derive(Debug, Deserialize)]
struct CreateProposalRequest {
    kind: Option<String>,
    #[serde(rename = "name")]
    name: Option<String>,
    #[serde(rename = "key")]
    key: Option<String>,
    description: Option<String>,
    value: Option<Value>,
    justification: Option<String>,
    #[serde(rename = "secretId")]
    secret_id: Option<Uuid>,
    #[serde(rename = "targetAgentId")]
    target_agent_id: Option<Uuid>,
    #[serde(rename = "configPath")]
    config_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProposalsQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RejectProposalRequest {
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ApproveProposalRequest {
    #[serde(default)]
    cascade: bool,
    overrides: Option<ApproveProposalOverrides>,
}

#[derive(Debug, Deserialize)]
struct ApproveProposalOverrides {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "providerConfigId")]
    provider_config_id: Option<Uuid>,
}

/// POST /agents/me/secret-proposals
async fn create_agent_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<CreateProposalRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let (agent_id, company_id, run_id) = agent_context(&actor)?;
    let kind = match request.kind.as_deref() {
        Some("secret") => "secret",
        Some("binding") => "binding",
        _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
    };
    let justification = request.justification.clone().unwrap_or_default();
    if justification.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if kind == "secret" && (request.name.is_none() || request.key.is_none() || request.value.is_none()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if kind == "binding" && (request.secret_id.is_none() || request.target_agent_id.is_none() || request.config_path.is_none()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let value_ciphertext = request.value.clone();
    let value_fingerprint = value_ciphertext
        .as_ref()
        .map(json_fingerprint);
    let value_length: Option<i32> = value_ciphertext
        .as_ref()
        .and_then(|v| v.to_string().len().try_into().ok());

    let id = Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    sqlx::query(
        "INSERT INTO company_secret_proposals \
         (id, company_id, kind, proposed_name, proposed_key, proposed_description, justification, \
          value_ciphertext, value_fingerprint_sha256, value_length, secret_id, target_type, target_id, \
          config_path, proposed_by_agent_id, origin_run_id, expires_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
    )
    .bind(id)
    .bind(company_id)
    .bind(kind)
    .bind(request.name.as_deref())
    .bind(request.key.as_deref())
    .bind(request.description.as_deref())
    .bind(&justification)
    .bind(&value_ciphertext)
    .bind(&value_fingerprint)
    .bind(value_length)
    .bind(request.secret_id)
    .bind(if kind == "binding" { Some("agent") } else { None })
    .bind(request.target_agent_id)
    .bind(request.config_path.as_deref())
    .bind(agent_id)
    .bind(run_id)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create secret proposal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let row = sqlx::query("SELECT * FROM company_secret_proposals WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload proposal: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(proposal_json(&row))))
}

/// GET /agents/me/secret-proposals
async fn list_agent_proposals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<ListProposalsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (agent_id, company_id, _) = agent_context(&actor)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);

    let rows = sqlx::query(
        "SELECT * FROM company_secret_proposals \
         WHERE company_id = $1 AND proposed_by_agent_id = $2 \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list agent proposals: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let has_more = rows.len() as i64 > limit;
    let visible = if has_more { &rows[..limit as usize] } else { &rows[..] };
    Ok(Json(json!({
        "proposals": visible.iter().map(proposal_json).collect::<Vec<_>>(),
        "nextOffset": if has_more { json!(offset + limit) } else { Value::Null },
    })))
}

/// DELETE /agents/me/secret-proposals/:id
async fn withdraw_agent_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(proposal_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (agent_id, company_id, _) = agent_context(&actor)?;
    let row = sqlx::query(
        "UPDATE company_secret_proposals SET status = 'withdrawn', updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 AND proposed_by_agent_id = $3 AND status = 'pending' \
         RETURNING *",
    )
    .bind(proposal_id)
    .bind(company_id)
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to withdraw proposal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(proposal_json(&row)))
}

/// GET /companies/:company_id/secret-proposals
async fn list_board_proposals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListProposalsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _ = match &actor {
        AuthorizationActor::Board { .. } => Ok(()),
        _ => Err(StatusCode::FORBIDDEN),
    }?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let status = query.status.as_deref();

    let rows = sqlx::query(
        "SELECT * FROM company_secret_proposals \
         WHERE company_id = $1 AND ($2::text IS NULL OR status = $2) \
         ORDER BY created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(company_id)
    .bind(status)
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list board proposals: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let has_more = rows.len() as i64 > limit;
    let visible = if has_more { &rows[..limit as usize] } else { &rows[..] };
    Ok(Json(json!({
        "proposals": visible.iter().map(proposal_json).collect::<Vec<_>>(),
        "nextOffset": if has_more { json!(offset + limit) } else { Value::Null },
    })))
}

/// POST /companies/:company_id/secret-proposals/:id/approve
async fn approve_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, proposal_id)): Path<(Uuid, Uuid)>,
    request: Option<Json<ApproveProposalRequest>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let row = sqlx::query(
        "SELECT * FROM company_secret_proposals WHERE id = $1 AND company_id = $2 AND status = 'pending'",
    )
    .bind(proposal_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load proposal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let kind: String = row.get("kind");
    let proposed_name: Option<String> = row.get("proposed_name");
    let proposed_key: Option<String> = row.get("proposed_key");
    let proposed_description: Option<String> = row.get("proposed_description");
    let value_ciphertext: Option<Value> = row.get("value_ciphertext");
    let fingerprint: Option<String> = row.get("value_fingerprint_sha256");
    let proposed_by: Uuid = row.get("proposed_by_agent_id");
    let request = request.map(|Json(value)| value).unwrap_or_default();
    if request.cascade {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut created_secret_id: Option<Uuid> = None;
    if kind == "secret" {
        let secret_id = Uuid::new_v4();
        let name = request
            .overrides
            .as_ref()
            .and_then(|value| value.name.as_deref())
            .or(proposed_name.as_deref())
            .filter(|value| !value.trim().is_empty())
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        let description = request
            .overrides
            .as_ref()
            .and_then(|value| value.description.clone())
            .or(proposed_description);
        let provider_config_id = request
            .overrides
            .as_ref()
            .and_then(|value| value.provider_config_id);
        sqlx::query(
            "INSERT INTO company_secrets (id, company_id, key, name, provider, provider_config_id, status, managed_mode, description, created_by_agent_id) \
             VALUES ($1,$2,$3,$4,'local_encrypted',$5,'active','paperclip_managed',$6,$7)",
        )
        .bind(secret_id)
        .bind(company_id)
        .bind(proposed_key.unwrap_or_default())
        .bind(name)
        .bind(provider_config_id)
        .bind(description)
        .bind(proposed_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create secret from proposal: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        sqlx::query(
            "INSERT INTO company_secret_versions (secret_id, version, material, value_sha256, status) \
             VALUES ($1, 1, $2, $3, 'current')",
        )
        .bind(secret_id)
        .bind(value_ciphertext.unwrap_or_else(|| json!({})))
        .bind(fingerprint.unwrap_or_default())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create secret version: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        created_secret_id = Some(secret_id);
    } else if kind == "binding" {
        let secret_id: Uuid = row.get("secret_id");
        let target_type: String = row.get("target_type");
        let target_id: Uuid = row.get("target_id");
        let config_path: String = row.get("config_path");
        if target_type != "agent" || config_path.trim().is_empty() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let secret_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM company_secrets WHERE id = $1 AND company_id = $2 AND deleted_at IS NULL)",
        )
        .bind(secret_id)
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let target_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND company_id = $2 AND status <> 'terminated')",
        )
        .bind(target_id)
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if !secret_exists || !target_exists {
            return Err(StatusCode::NOT_FOUND);
        }
        sqlx::query(
            "INSERT INTO company_secret_bindings (company_id, secret_id, target_type, target_id, config_path) VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(company_id)
        .bind(secret_id)
        .bind(target_type)
        .bind(target_id.to_string())
        .bind(config_path)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    sqlx::query(
        "UPDATE company_secret_proposals \
         SET status = 'approved', created_secret_id = $3, resolved_by_user_id = $4, resolved_at = NOW(), updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(proposal_id)
    .bind(company_id)
    .bind(created_secret_id)
    .bind(user_id.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to approve proposal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "secret.proposal_approved",
        &actor,
        "company_secret_proposal",
        proposal_id,
        json!({ "kind": kind, "createdSecretId": created_secret_id }),
    )
    .await;

    let updated = sqlx::query("SELECT * FROM company_secret_proposals WHERE id = $1")
        .bind(proposal_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload proposal: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(proposal_json(&updated)))
}

/// POST /companies/:company_id/secret-proposals/:id/reject
async fn reject_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, proposal_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<RejectProposalRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let reason = request.reason.unwrap_or_default().trim().to_string();
    if reason.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let row = sqlx::query(
        "UPDATE company_secret_proposals \
         SET status = 'rejected', resolution_reason = $3, resolved_by_user_id = $4, resolved_at = NOW(), updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 AND status = 'pending' RETURNING *",
    )
    .bind(proposal_id)
    .bind(company_id)
    .bind(&reason)
    .bind(user_id.to_string())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to reject proposal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "secret.proposal_rejected",
        &actor,
        "company_secret_proposal",
        proposal_id,
        json!({ "reason": reason }),
    )
    .await;

    Ok(Json(proposal_json(&row)))
}

/// GET /agents/me/secrets
/// 简化对齐：返回该 agent 已获批 secret 提案的元数据（不含值）。
async fn list_agent_secrets(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (agent_id, company_id, _) = agent_context(&actor)?;
    let rows = sqlx::query(
        "SELECT p.id, p.proposed_name, p.proposed_key, p.proposed_description, p.created_secret_id, p.created_at \
         FROM company_secret_proposals p \
         WHERE p.company_id = $1 AND p.proposed_by_agent_id = $2 AND p.kind = 'secret' AND p.status = 'approved' \
         ORDER BY p.created_at DESC",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list agent secrets: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let secrets: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.get::<Option<String>, _>("proposed_name"),
                "key": r.get::<Option<String>, _>("proposed_key"),
                "description": r.get::<Option<String>, _>("proposed_description"),
                "secretId": r.get::<Option<Uuid>, _>("created_secret_id"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "secret.access.listed",
        &actor,
        "agent",
        agent_id,
        json!({ "count": secrets.len() }),
    )
    .await;

    Ok(Json(json!({ "secrets": secrets })))
}

fn json_fingerprint(v: &Value) -> String {
    Sha256::digest(v.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposal_fingerprint_is_deterministic_and_changes_with_value() {
        let value = json!({"token": "redacted"});
        assert_eq!(json_fingerprint(&value), json_fingerprint(&value));
        assert_ne!(json_fingerprint(&value), json_fingerprint(&json!({"token": "other"})));
        assert_eq!(json_fingerprint(&value).len(), 64);
    }
}

pub fn secret_proposal_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/agents/me/secret-proposals",
            post(create_agent_proposal).get(list_agent_proposals),
        )
        .route(
            "/agents/me/secret-proposals/:id",
            delete(withdraw_agent_proposal),
        )
        .route("/agents/me/secrets", get(list_agent_secrets))
        .route(
            "/companies/:company_id/secret-proposals",
            get(list_board_proposals),
        )
        .route(
            "/companies/:company_id/secret-proposals/:id/approve",
            post(approve_proposal),
        )
        .route(
            "/companies/:company_id/secret-proposals/:id",
            get(get_board_proposal),
        )
        .route(
            "/companies/:company_id/secret-proposals/:id/reject",
            post(reject_proposal),
        )
}

async fn get_board_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, proposal_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !matches!(actor, AuthorizationActor::Board { .. }) {
        return Err(StatusCode::FORBIDDEN);
    }
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "SELECT * FROM company_secret_proposals WHERE id = $1 AND company_id = $2",
    )
    .bind(proposal_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(proposal_json(&row)))
}
