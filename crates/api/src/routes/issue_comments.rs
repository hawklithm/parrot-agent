use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{IssueComment, CommentActorType, Pagination};
use services::CommentServiceError;
use crate::errors::ApiError;
use crate::app_state::AppState;

/// Add comment request
#[derive(Debug, Deserialize)]
pub struct AddCommentRequest {
    pub body: String,
    pub actor_type: CommentActorType,
    pub actor_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
}

/// Update comment request
#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub body: String,
    pub actor_id: Uuid,
}

/// Delete comment request
#[derive(Debug, Deserialize)]
pub struct DeleteCommentRequest {
    pub actor_id: Uuid,
}

/// Comment pagination query
#[derive(Debug, Deserialize)]
pub struct CommentPaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Comment response
#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub comment: IssueComment,
}

/// Comments list response
#[derive(Debug, Serialize)]
pub struct CommentsListResponse {
    pub comments: Vec<IssueComment>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// Convert service errors to API errors
impl From<CommentServiceError> for ApiError {
    fn from(err: CommentServiceError) -> Self {
        match err {
            CommentServiceError::NotFound(id) => {
                ApiError::NotFound(format!("Comment not found: {}", id))
            }
            CommentServiceError::IssueNotFound(id) => {
                ApiError::NotFound(format!("Issue not found: {}", id))
            }
            CommentServiceError::PermissionDenied(msg) => {
                ApiError::Forbidden(msg)
            }
            CommentServiceError::Validation(msg) => {
                ApiError::BadRequest(msg)
            }
            CommentServiceError::Repository(repo_err) => {
                ApiError::InternalServerError(format!("Database error: {}", repo_err))
            }
        }
    }
}

/// POST /issues/:issue_id/comments - Add a comment
pub async fn add_comment(
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
    Json(req): Json<AddCommentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.issue_comment_service.clone();
    let comment = service.add_comment(
        issue_id,
        req.body,
        req.actor_type,
        req.actor_id,
        req.actor_run_id,
        req.metadata,
    ).await?;

    Ok((StatusCode::CREATED, Json(CommentResponse { comment })))
}

/// GET /issues/:issue_id/comments - List comments for an issue
pub async fn list_comments(
    State(state): State<AppState>,
    Path(issue_id): Path<Uuid>,
    Query(query): Query<CommentPaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.issue_comment_service.clone();
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).min(100),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };

    let comments = service.list_comments(issue_id, &pagination).await?;
    let total = service.count_comments(issue_id).await?;

    Ok(Json(CommentsListResponse {
        comments,
        total,
        limit: pagination.limit,
        offset: pagination.offset,
    }))
}

/// GET /comments/:comment_id - Get a single comment
pub async fn get_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.issue_comment_service.clone();
    let comment = service.get_comment(comment_id).await?;

    Ok(Json(CommentResponse { comment }))
}

/// PUT /comments/:comment_id - Update a comment
pub async fn update_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<Uuid>,
    Json(req): Json<UpdateCommentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.issue_comment_service.clone();
    let comment = service.update_comment(
        comment_id,
        req.body,
        req.actor_id,
    ).await?;

    Ok(Json(CommentResponse { comment }))
}

/// DELETE /comments/:comment_id - Delete a comment
pub async fn delete_comment(
    State(state): State<AppState>,
    Path(comment_id): Path<Uuid>,
    Json(req): Json<DeleteCommentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let service = state.issue_comment_service.clone();
    service.delete_comment(comment_id, req.actor_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Create Issue Comment routes
pub fn issue_comment_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:issue_id/comments", post(add_comment))
        .route("/issues/:issue_id/comments", get(list_comments))
        .route("/comments/:comment_id", get(get_comment))
        .route("/comments/:comment_id", put(update_comment))
        .route("/comments/:comment_id", delete(delete_comment))
}
