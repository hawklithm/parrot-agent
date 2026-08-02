use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::issue_auxiliary::{Attachment, UploadAttachmentInput};
use services::auth::AuthorizationActor;

/// GET /issues/:id/attachments - List issue attachments
async fn list_issue_attachments(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Attachment>>, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state
        .attachment_service
        .list_attachments("issue", id, company_id)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// POST /issues/:id/attachments - Upload attachment
async fn upload_issue_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<UploadAttachmentInput>,
) -> Result<Json<Attachment>, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state
        .attachment_service
        .upload_attachment("issue", id, company_id, input)
        .await
        .map(Json)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// GET /attachments/:id/content - Get attachment content
async fn get_attachment_content(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Vec<u8>, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Attachment not found".to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state
        .attachment_service
        .get_attachment_content(id, company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// DELETE /attachments/:id - Delete attachment
async fn delete_attachment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM attachments WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or(AppError::NotFound("Attachment not found".to_string()))?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|e| AppError::Forbidden(e.to_string()))?;

    state
        .attachment_service
        .delete_attachment(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// Create attachment routes (AppState compatible)
pub fn attachment_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/issues/:id/attachments",
            get(list_issue_attachments).post(upload_issue_attachment),
        )
        .route("/attachments/:id/content", get(get_attachment_content))
        .route("/attachments/:id", delete(delete_attachment))
}
