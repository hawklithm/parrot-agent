//! Instance Settings routes — 实例级设置管理 (IS1-IS9)

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::app_state::AppState;

pub fn instance_settings_routes() -> Router<AppState> {
    Router::new()
        .route("/instance/settings", get(get_instance_settings).patch(update_instance_settings))
        .route("/instance/settings/general", get(get_general_settings).patch(update_general_settings))
        .route("/instance/settings/experimental", get(get_experimental_settings).patch(update_experimental_settings))
        .route("/instance/settings/experimental/issue-graph-liveness-auto-recovery/preview", post(preview_auto_recovery))
        .route("/instance/settings/experimental/issue-graph-liveness-auto-recovery/run", post(run_auto_recovery))
        .route("/instance/database-backups", post(create_database_backup))
}

/// IS1: GET /instance/settings
async fn get_instance_settings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.get_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS2: PATCH /instance/settings
async fn update_instance_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.update_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS3: GET /instance/settings/general
async fn get_general_settings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.get_general_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS4: PATCH /instance/settings/general
async fn update_general_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.update_general_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS5: GET /instance/settings/experimental
async fn get_experimental_settings(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.get_experimental_settings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS6: PATCH /instance/settings/experimental
async fn update_experimental_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let settings = state.instance_settings_service.update_experimental_settings(body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(settings).unwrap_or_default()))
}

/// IS7: POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/preview
async fn preview_auto_recovery(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = state.instance_settings_service.preview_auto_recovery()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// IS8: POST /instance/settings/experimental/issue-graph-liveness-auto-recovery/run
async fn run_auto_recovery(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = state.instance_settings_service.run_auto_recovery()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// IS9: POST /instance/database-backups
async fn create_database_backup(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let result = state.instance_settings_service.create_database_backup()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}
