use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::models::{AddCommentInput, IssueComment};
use crate::services::CommentService;

/// GET /issues/:id/comments - List issue comments
async fn list_comments(
    State(service): State<Arc<dyn CommentService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IssueComment>>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .list_comments(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/comments - Add comment to issue
async fn add_comment(
    State(service): State<Arc<dyn CommentService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<AddCommentInput>,
) -> Result<Json<IssueComment>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .add_comment(id, company_id, input, None, None)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /issues/:id/comments/:commentId - Delete comment
async fn delete_comment(
    State(service): State<Arc<dyn CommentService>>,
    Path((_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .delete_comment(comment_id, company_id, None, None)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create comment routes
pub fn comment_routes(service: Arc<dyn CommentService>) -> Router {
    Router::new()
        .route("/api/issues/:id/comments", get(list_comments).post(add_comment))
        .route("/api/issues/:id/comments/:commentId", delete(delete_comment))
        .with_state(service)
}
