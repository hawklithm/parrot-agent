//! Instance Settings routes — P4 收尾域 (IS1-IS9)

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

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

async fn get_instance_settings(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"instanceName": "Parrot Agent", "version": "0.1.0"})))
}

async fn update_instance_settings(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"updated": true})))
}

async fn get_general_settings(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"timezone": "UTC", "language": "en"})))
}

async fn update_general_settings(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"updated": true})))
}

async fn get_experimental_settings(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueGraphLivenessAutoRecovery": false})))
}

async fn update_experimental_settings(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"updated": true})))
}

async fn preview_auto_recovery(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"affectedIssues": 0, "previewComplete": true})))
}

async fn run_auto_recovery(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"recoveredIssues": 0, "recoveryComplete": true})))
}

async fn create_database_backup(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"backupId": Uuid::new_v4(), "status": "started"})))
}
