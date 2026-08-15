//! Inbox Dismissals 路由 —— 对齐 Paperclip `server/src/routes/inbox-dismissals.ts`。
//! `companies.rs` 旧 issue-inbox-archive 语义已迁至
//! `/companies/:company_id/issues/inbox-archive`，本模块接管契约路径。

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

/// 对齐 Paperclip `ITEM_KEY_RE = /^(approval|join|run|attention):.+$/`。
fn valid_item_key(key: &str) -> bool {
    ["approval:", "join:", "run:", "attention:"]
        .iter()
        .any(|p| key.starts_with(p) && key.len() > p.len())
}

fn board_user_id(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

/// GET /companies/:company_id/inbox-dismissals —— 列出当前用户 dismiss/snooze。
async fn list_inbox_dismissals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let user_id = board_user_id(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT item_key, kind, dismissed_at, snoozed_until \
         FROM inbox_dismissals WHERE company_id = $1 AND user_id = $2 ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list inbox dismissals: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let items: Vec<serde_json::Value> = rows.iter().map(|r| json!({
        "itemKey": r.get::<String, _>("item_key"),
        "kind": r.get::<String, _>("kind"),
        "dismissedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("dismissed_at"),
        "snoozedUntil": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("snoozed_until"),
    })).collect();
    Ok(Json(items))
}

#[derive(Debug, Deserialize)]
struct CreateInboxDismissalRequest {
    #[serde(rename = "itemKey")]
    item_key: String,
    #[serde(default = "default_kind")]
    kind: String,
    #[serde(rename = "snoozedUntil")]
    snoozed_until: Option<String>,
}
fn default_kind() -> String {
    "dismiss".to_string()
}

/// POST /companies/:company_id/inbox-dismissals —— 创建 dismiss/snooze（含校验 + 审计）。
async fn create_inbox_dismissal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateInboxDismissalRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let user_id = board_user_id(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let item_key = request.item_key.trim().to_string();
    if item_key.is_empty() || !valid_item_key(&item_key) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let kind = match request.kind.as_str() {
        "dismiss" => "dismiss",
        "snooze" => "snooze",
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let snoozed_until = match kind {
        "dismiss" => {
            if request.snoozed_until.is_some() {
                return Err(StatusCode::BAD_REQUEST);
            }
            None
        }
        _ => {
            let raw = request.snoozed_until.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
            let parsed = chrono::DateTime::parse_from_rfc3339(raw)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .with_timezone(&chrono::Utc);
            if parsed <= chrono::Utc::now() {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(parsed)
        }
    };
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO inbox_dismissals (company_id, user_id, item_key, kind, dismissed_at, snoozed_until) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (company_id, user_id, item_key) DO UPDATE \
         SET kind = EXCLUDED.kind, dismissed_at = EXCLUDED.dismissed_at, snoozed_until = EXCLUDED.snoozed_until",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(&item_key)
    .bind(kind)
    .bind(now)
    .bind(snoozed_until)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create inbox dismissal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        if kind == "snooze" { "inbox.snoozed" } else { "inbox.dismissed" },
        &actor,
        "company",
        company_id,
        json!({ "userId": user_id, "itemKey": item_key, "kind": kind }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "itemKey": item_key,
            "kind": kind,
            "dismissedAt": now,
            "snoozedUntil": snoozed_until,
        })),
    ))
}

/// DELETE /companies/:company_id/inbox-dismissals/:item_key
/// 恢复（取消 dismiss），写 `inbox.restored` 审计；对齐 Paperclip。
async fn delete_inbox_dismissal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, item_key)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = board_user_id(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    if !valid_item_key(&item_key) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let deleted = sqlx::query(
        "DELETE FROM inbox_dismissals WHERE company_id = $1 AND user_id = $2 AND item_key = $3 \
         RETURNING item_key, kind",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(&item_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to restore inbox dismissal: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(row) = deleted {
        use sqlx::Row;
        crate::routes::log_activity(
            &state.pool,
            company_id,
            "inbox.restored",
            &actor,
            "company",
            company_id,
            json!({
                "userId": user_id,
                "itemKey": row.get::<String, _>("item_key"),
                "kind": row.get::<String, _>("kind"),
            }),
        )
        .await;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub fn inbox_dismissal_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/inbox-dismissals",
            get(list_inbox_dismissals).post(create_inbox_dismissal),
        )
        .route(
            "/companies/:company_id/inbox-dismissals/:item_key",
            delete(delete_inbox_dismissal),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_key_validation_matches_paperclip_regex() {
        assert!(valid_item_key("approval:123"));
        assert!(valid_item_key("join:abc"));
        assert!(valid_item_key("run:x"));
        assert!(valid_item_key("attention:1"));
        assert!(!valid_item_key("approval:"));
        assert!(!valid_item_key("other:123"));
        assert!(!valid_item_key(""));
    }
}
