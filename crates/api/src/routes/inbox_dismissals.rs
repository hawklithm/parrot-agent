//! Inbox Dismissals 路由 —— 对齐 Paperclip `server/src/routes/inbox-dismissals.ts`。
//!
//! 注意：`GET/POST /companies/:company_id/inbox-dismissals` 在 Parrot 中已由
//! `companies.rs`（CM19/CM20，issue inbox 归档语义）占用同一路径，为**预存在
//! 的语义分歧**（与 Paperclip dismiss 契约不同），本模块不重复注册，仅补齐缺失的
//! `DELETE /companies/:company_id/inbox-dismissals/:item_key`。
//!
//! 权限：board 用户 + company access（写操作，viewer 不可写）。

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::delete,
    Json, Router,
};
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
    Router::new().route(
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
