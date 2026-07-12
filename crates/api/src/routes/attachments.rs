use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use models::{Attachment, UploadAttachmentInput};
use services::AttachmentService;

/// GET /issues/:id/attachments - List issue attachments
async fn list_issue_attachments(
    State(service): State<Arc<dyn AttachmentService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Attachment>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_attachments("issue", id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/attachments - Upload attachment
async fn upload_issue_attachment(
    State(service): State<Arc<dyn AttachmentService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UploadAttachmentInput>,
) -> Result<Json<Attachment>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .upload_attachment("issue", id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /attachments/:id/content - Get attachment content
async fn get_attachment_content(
    State(service): State<Arc<dyn AttachmentService>>,
    Path(id): Path<Uuid>,
) -> Result<Vec<u8>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get_attachment_content(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /attachments/:id - Delete attachment
async fn delete_attachment(
    State(service): State<Arc<dyn AttachmentService>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .delete_attachment(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create attachment routes
pub fn attachment_routes(service: Arc<dyn AttachmentService>) -> Router {
    Router::new()
        .route("/api/issues/:id/attachments", get(list_issue_attachments).post(upload_issue_attachment))
        .route("/api/attachments/:id/content", get(get_attachment_content))
        .route("/api/attachments/:id", delete(delete_attachment))
        .with_state(service)
}
