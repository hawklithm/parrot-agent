//! Label routes — P4 收尾域 (LB1-LB3)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn label_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/labels", get(list_labels).post(create_label))
        .route("/labels/:label_id", delete(delete_label))
}

async fn list_labels(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![
        serde_json::json!({"id": Uuid::new_v4(), "name": "bug", "color": "red"}),
        serde_json::json!({"id": Uuid::new_v4(), "name": "enhancement", "color": "blue"}),
    ]))
}

async fn create_label(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": Uuid::new_v4(),
        "companyId": company_id,
        "created": true,
    }))))
}

async fn delete_label(
    State(_state): State<AppState>,
    Path(_label_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}
