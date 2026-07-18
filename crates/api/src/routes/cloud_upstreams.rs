//! Cloud Upstream routes — P4 收尾域 (CU1-CU8)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn cloud_upstream_routes() -> Router<AppState> {
    Router::new()
        .route("/cloud-upstreams", get(list_cloud_upstreams))
        .route("/cloud-upstreams/connect/start", post(start_cloud_connect))
        .route("/cloud-upstreams/connect/finish", post(finish_cloud_connect))
        .route("/cloud-upstreams/:connection_id/push-runs/preview", post(preview_push_run))
        .route("/cloud-upstreams/:connection_id/push-runs", post(execute_push_run))
        .route("/cloud-upstreams/:connection_id/push-runs/:run_id", get(get_push_run))
        .route("/cloud-upstreams/:connection_id/push-runs/:run_id/cancel", post(cancel_push_run))
        .route("/cloud-upstreams/:connection_id/push-runs/:run_id/activation", post(activate_push_run))
}

async fn list_cloud_upstreams(
    State(_state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

async fn start_cloud_connect(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": Uuid::new_v4(), "status": "started"})))
}

async fn finish_cloud_connect(
    State(_state): State<AppState>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"status": "connected"})))
}

async fn preview_push_run(
    State(_state): State<AppState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": connection_id, "preview": {}})))
}

async fn execute_push_run(
    State(_state): State<AppState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": connection_id, "runId": Uuid::new_v4(), "started": true})))
}

async fn get_push_run(
    State(_state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": connection_id, "runId": run_id, "status": "completed"})))
}

async fn cancel_push_run(
    State(_state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": connection_id, "runId": run_id, "cancelled": true})))
}

async fn activate_push_run(
    State(_state): State<AppState>,
    Path((connection_id, run_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"connectionId": connection_id, "runId": run_id, "activated": true})))
}
