//! Sidebar Preferences 路由 —— 对齐 Paperclip `routes/sidebar-preferences.ts`。
//!
//! 注意：`GET/PUT /companies/:company_id/sidebar-preferences/me`（company 级
//! project 顺序）在 Parrot 中已由 `companies.rs`（CM9/CM10，基于 user_preferences
//! 表）占用同一路径，为**预存在的语义分歧**，本模块不重复注册，仅实现顶层
//! `GET/PUT /sidebar-preferences/me`（user 级 company 顺序）。

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use services::auth::AuthorizationActor;

fn board_user_id(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

/// 对齐 Paperclip `upsertSidebarOrderPreferenceSchema`。
#[derive(Debug, Deserialize)]
struct UpsertOrderRequest {
    #[serde(rename = "orderedIds")]
    ordered_ids: Vec<String>,
}

fn ordered_ids_json(ordered_ids: &[String]) -> serde_json::Value {
    json!({ "orderedIds": ordered_ids })
}

/// GET /sidebar-preferences/me
async fn get_user_sidebar_preferences(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = board_user_id(&actor)?;
    let row = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT company_order FROM user_sidebar_preferences WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load sidebar preferences: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let ordered = row
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Ok(Json(ordered_ids_json(
        &ordered
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
    )))
}

/// PUT /sidebar-preferences/me
async fn put_user_sidebar_preferences(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<UpsertOrderRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = board_user_id(&actor)?;
    let ordered: serde_json::Value = serde_json::to_value(&request.ordered_ids)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query(
        "INSERT INTO user_sidebar_preferences (user_id, company_order, updated_at) \
         VALUES ($1, $2, NOW()) \
         ON CONFLICT (user_id) DO UPDATE SET company_order = EXCLUDED.company_order, updated_at = NOW()",
    )
    .bind(user_id)
    .bind(&ordered)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to save sidebar preferences: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ordered_ids_json(&request.ordered_ids)))
}

pub fn sidebar_preference_routes() -> Router<AppState> {
    Router::new().route(
        "/sidebar-preferences/me",
        get(get_user_sidebar_preferences).put(put_user_sidebar_preferences),
    )
}
