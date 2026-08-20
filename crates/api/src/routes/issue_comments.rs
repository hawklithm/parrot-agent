use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{IssueComment, CommentActorType};
use services::{CommentServiceError, CrossIssueInfluenceKind, CrossIssueInfluenceLimitService,
    DefaultCrossIssueInfluenceLimitService};
use crate::errors::ApiError;
use crate::app_state::AppState;
use repositories::ISSUE_COMMENT_COLUMNS;
use services::auth::AuthorizationActor;

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
    /// Paperclip uses the last comment id as a cursor for loading older pages.
    pub after: Option<Uuid>,
    pub order: Option<String>,
}

/// Comment response
#[derive(Debug, Serialize)]
pub struct CommentResponse {
    pub comment: IssueComment,
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Json(req): Json<AddCommentRequest>,
) -> Result<impl IntoResponse, ApiError> {
    assert_issue_company(&state, &actor, issue_id).await?;
    let service = state.issue_comment_service.clone();

    if let AuthorizationActor::Agent {
        agent_id,
        run_id: Some(run_id),
        company_id: actor_company_id,
        ..
    } = &actor
    {
        if actor.company_id() != Some(*actor_company_id) {
            return Err(ApiError::Forbidden("Missing company scope".into()));
        }
        let guard = DefaultCrossIssueInfluenceLimitService::new().with_pool(state.pool.clone());
        match guard
            .observe_influence(services::ObserveCrossIssueInfluenceInput {
                heartbeat_run_id: *run_id,
                company_id: *actor_company_id,
                agent_id: *agent_id,
                source_issue_id: issue_id,
                target_issue_id: issue_id,
                influence_kind: CrossIssueInfluenceKind::Comment,
                actor_label: None,
                assignee_label: None,
                issue_identifier: None,
            })
            .await
        {
            Ok(_) => {}
            Err(services::InfluenceLimitError::LimitExceeded { current, cap }) => {
                return Err(ApiError::Forbidden(format!(
                    "Cross-issue influence limit exceeded: {current}/{cap}"
                )));
            }
            Err(services::InfluenceLimitError::RunNotFound(_)
            | services::InfluenceLimitError::RunContextRequired) => {
                return Err(ApiError::Forbidden("Run context required for cross-issue comments".into()));
            }
            Err(services::InfluenceLimitError::DatabaseError(error)) => {
                return Err(ApiError::InternalServerError(error));
            }
        }
    }
    
    // Save actor_type before moving req
    let actor_type = req.actor_type;
    let actor_id = req.actor_id;
    
    let comment = service.add_comment(
        issue_id,
        req.body,
        actor_type.clone(),
        actor_id,
        req.actor_run_id,
        req.metadata,
    ).await?;

    // Expire superseded interactions when a user comments
    if actor_type == CommentActorType::User {
        let interaction_service = services::IssueThreadInteractionService::new(state.pool.clone());
        let resolver = services::InteractionResolver {
            resolver_type: "system".to_string(),
            resolver_id: "comment_supersede".to_string(),
        };
        
        let user_id = actor_id.map(|id| id.to_string());
        let _ = interaction_service.expire_request_confirmations_superseded_by_comment(
            issue_id,
            comment.created_at,
            user_id,
            resolver,
        ).await;
        // We don't fail the comment creation if interaction expiration fails
    }

    Ok((StatusCode::CREATED, Json(CommentResponse { comment })))
}

/// GET /issues/:issue_id/comments - List comments for an issue
pub async fn list_comments(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Query(query): Query<CommentPaginationQuery>,
) -> Result<impl IntoResponse, ApiError> {
    assert_issue_company(&state, &actor, issue_id).await?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let order = query.order.as_deref().unwrap_or("desc").to_ascii_lowercase();

    // The Paperclip UI paginates comments with an opaque-looking, but in
    // practice stable, comment-id cursor.  The generic repository API only
    // supports offset pagination, so use the shared pool here to preserve the
    // ordering/cursor contract and avoid returning the same page repeatedly.
    let comments = if let Some(after_id) = query.after {
        let anchor: Option<(Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT id, created_at FROM issue_comments WHERE issue_id = $1 AND id = $2",
        )
        .bind(issue_id)
        .bind(after_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

        let Some((anchor_id, anchor_created_at)) = anchor else {
            return Ok(Json(Vec::<models::IssueComment>::new()));
        };

        if order == "asc" {
            sqlx::query_as::<_, models::IssueComment>(&format!(
                "SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE issue_id = $1 AND (created_at > $2 OR (created_at = $2 AND id > $3)) ORDER BY created_at ASC, id ASC LIMIT $4"
            ))
            .bind(issue_id)
            .bind(anchor_created_at)
            .bind(anchor_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await
        } else {
            sqlx::query_as::<_, models::IssueComment>(&format!(
                "SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE issue_id = $1 AND (created_at < $2 OR (created_at = $2 AND id < $3)) ORDER BY created_at DESC, id DESC LIMIT $4"
            ))
            .bind(issue_id)
            .bind(anchor_created_at)
            .bind(anchor_id)
            .bind(limit)
            .fetch_all(&state.pool)
            .await
        }
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
    } else if order == "asc" {
        sqlx::query_as::<_, models::IssueComment>(&format!(
            "SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE issue_id = $1 ORDER BY created_at ASC, id ASC LIMIT $2 OFFSET $3"
        ))
        .bind(issue_id)
        .bind(limit)
        .bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
    } else {
        sqlx::query_as::<_, models::IssueComment>(&format!(
            "SELECT {ISSUE_COMMENT_COLUMNS} FROM issue_comments WHERE issue_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"
        ))
        .bind(issue_id)
        .bind(limit)
        .bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
    };
    // Paperclip's client consumes this endpoint as `IssueComment[]`.  The
    // previous envelope (`{ comments, total, ... }`) was passed directly into
    // the chat message builder, producing messages without `content` and
    // causing both the normal renderer and its fallback to crash.
    Ok(Json(comments))
}

/// GET /comments/:comment_id - Get a single comment
pub async fn get_comment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(comment_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let company_id = actor.company_id().ok_or_else(|| ApiError::Forbidden("Missing company scope".into()))?;
    let visible = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
           SELECT 1 FROM issue_comments c
           JOIN issues i ON i.id = c.issue_id
          WHERE c.id = $1 AND i.company_id = $2
        )",
    )
    .bind(comment_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| ApiError::InternalServerError(error.to_string()))?;
    if !visible {
        return Err(ApiError::NotFound(format!("Comment not found: {comment_id}")));
    }
    let service = state.issue_comment_service.clone();
    let comment = service.get_comment(comment_id).await?;

    Ok(Json(CommentResponse { comment }))
}

/// GET /issues/:issue_id/comments/:comment_id — Paperclip's issue-scoped
/// comment contract. Keep the legacy /comments/:comment_id route for existing
/// UI callers, but require the issue path for migrated MCP calls.
pub async fn get_issue_comment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    assert_issue_company(&state, &actor, issue_id).await?;
    let belongs_to_issue = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM issue_comments WHERE id = $1 AND issue_id = $2)",
    )
    .bind(comment_id)
    .bind(issue_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| ApiError::InternalServerError(error.to_string()))?;
    if !belongs_to_issue {
        return Err(ApiError::NotFound(format!("Comment not found: {comment_id}")));
    }
    let comment = state.issue_comment_service.get_comment(comment_id).await?;
    Ok(Json(CommentResponse { comment }))
}

async fn assert_issue_company(
    state: &AppState,
    actor: &AuthorizationActor,
    issue_id: Uuid,
) -> Result<(), ApiError> {
    // Match Paperclip: load the issue first, then authorize using its owning
    // company. A local-trusted Board actor intentionally has no company
    // scope, so comparing actor.company_id() to issue.company_id is invalid.
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|error| ApiError::InternalServerError(error.to_string()))?;
    let Some(company_id) = company_id else {
        return Err(ApiError::NotFound(format!("Issue not found: {issue_id}")));
    };
    crate::routes::assert_company_access(actor, company_id, true)
        .map_err(|_| ApiError::Forbidden("Issue is outside the actor's company scope".into()))
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
        .route("/issues/:issue_id/comments/:comment_id", get(get_issue_comment))
        .route("/comments/:comment_id", get(get_comment))
        .route("/comments/:comment_id", put(update_comment))
        .route("/comments/:comment_id", delete(delete_comment))
}
