//! Label routes — Paperclip 一比一迁移
//!
//! 对应 Paperclip: Labels are embedded in the issue model.
//! 提供标签的 CRUD 端点。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;

/// 创建标签请求体
#[derive(Debug, Deserialize)]
pub struct CreateLabelRequest {
    pub name: String,
    pub color: Option<String>,
}

pub fn label_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/labels", get(list_labels).post(create_label))
        .route("/labels/:label_id", delete(delete_label))
}

/// GET /companies/:company_id/labels
/// 列出公司标签。
async fn list_labels(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let labels = state
        .label_service
        .list_by_company(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    let result: Vec<serde_json::Value> = labels
        .into_iter()
        .map(|l| {
            serde_json::json!({
                "id": l.id,
                "companyId": l.company_id,
                "name": l.name,
                "color": l.color,
                "createdAt": l.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

/// POST /companies/:company_id/labels
/// 创建标签。
async fn create_label(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateLabelRequest>,
) -> Result<impl IntoResponse, AppError> {
    let label = state
        .label_service
        .create(company_id, body.name, body.color)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": label.id,
            "companyId": label.company_id,
            "name": label.name,
            "color": label.color,
            "createdAt": label.created_at,
            "created": true,
        })),
    ))
}

/// DELETE /labels/:label_id
/// 删除标签。
async fn delete_label(
    State(state): State<AppState>,
    Path(label_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .label_service
        .delete(label_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
