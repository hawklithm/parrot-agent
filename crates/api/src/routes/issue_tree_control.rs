use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{app_state::AppState, errors::ApiError, routes::{require_company_access, AccessMode}};
use models::{
    ActiveIssueTreePauseHoldGate, CreateIssueTreeHoldInput, IssueTreeControlMode, IssueTreeHold,
    IssueTreeHoldMember, IssueTreeHoldReleasePolicy, IssueTreeHoldReleasePolicyStrategy,
};
use services::TreeControlServiceError;
use services::auth::AuthorizationActor;

async fn authorized_issue_company(
    state: &AppState,
    actor: &AuthorizationActor,
    issue_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, ApiError> {
    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM issues WHERE id = $1",
    )
    .bind(issue_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| ApiError::InternalServerError(error.to_string()))?
    .ok_or_else(|| ApiError::NotFound(format!("Issue not found: {}", issue_id)))?;
    require_company_access(actor, company_id, mode)
        .map_err(|_| ApiError::Forbidden("Issue tree control access denied".to_string()))?;
    Ok(company_id)
}

/// Preview tree control request
#[derive(Debug, Deserialize)]
pub struct PreviewTreeControlRequest {
    pub mode: IssueTreeControlMode,
}

/// Create tree hold request
#[derive(Debug, Deserialize)]
pub struct CreateTreeHoldRequest {
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: Option<IssueTreeHoldReleasePolicy>,
    pub metadata: Option<serde_json::Value>,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
}

/// Release tree hold request
#[derive(Debug, Deserialize)]
pub struct ReleaseTreeHoldRequest {
    pub released_by_type: Option<String>,
    pub released_by_id: Option<Uuid>,
}

/// Tree hold response
#[derive(Debug, Serialize)]
pub struct TreeHoldResponse {
    pub hold: IssueTreeHold,
}

/// Tree holds list response
#[derive(Debug, Serialize)]
pub struct TreeHoldsListResponse {
    pub holds: Vec<IssueTreeHold>,
}

/// Tree hold members response
#[derive(Debug, Serialize)]
pub struct TreeHoldMembersResponse {
    pub members: Vec<IssueTreeHoldMember>,
}

/// Pause state response
#[derive(Debug, Serialize)]
pub struct PauseStateResponse {
    pub paused: bool,
    pub gate: Option<ActiveIssueTreePauseHoldGate>,
}

// Convert service errors to API errors
impl From<TreeControlServiceError> for ApiError {
    fn from(err: TreeControlServiceError) -> Self {
        match err {
            TreeControlServiceError::HoldNotFound(id) => {
                ApiError::NotFound(format!("Tree hold not found: {}", id))
            }
            TreeControlServiceError::IssueNotFound(id) => {
                ApiError::NotFound(format!("Issue not found: {}", id))
            }
            TreeControlServiceError::HoldAlreadyReleased => {
                ApiError::Conflict("Hold already released".to_string())
            }
            TreeControlServiceError::InvalidOperation(msg) => ApiError::BadRequest(msg),
            TreeControlServiceError::Validation(msg) => ApiError::BadRequest(msg),
            TreeControlServiceError::Repository(repo_err) => {
                ApiError::InternalServerError(format!("Database error: {}", repo_err))
            }
        }
    }
}

/// POST /issues/:id/tree-control/preview - Preview tree control effect
pub async fn preview_tree_control(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Json(req): Json<PreviewTreeControlRequest>,
) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Read).await?;
    let preview = state
        .issue_tree_control_service
        .preview_tree_hold(issue_id, req.mode)
        .await?;

    Ok(Json(preview))
}

/// POST /issues/:id/tree-holds - Create a tree hold
pub async fn create_tree_hold(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Json(req): Json<CreateTreeHoldRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Get issue to determine company_id
    let company_id = authorized_issue_company(&state, &actor, issue_id, AccessMode::Write).await?;

    let input = CreateIssueTreeHoldInput {
        mode: req.mode,
        reason: req.reason,
        release_policy: req
            .release_policy
            .unwrap_or_else(|| IssueTreeHoldReleasePolicy {
                strategy: IssueTreeHoldReleasePolicyStrategy::Manual,
                note: None,
            }),
        metadata: req.metadata,
    };

    let hold = state
        .issue_tree_control_service
        .create_tree_hold(
            company_id,
            issue_id,
            input,
            Some(actor.actor_type().to_string()),
            actor.principal_id(),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(TreeHoldResponse { hold })))
}

/// GET /issues/:id/tree-holds - List tree holds for an issue
pub async fn list_tree_holds(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Read).await?;
    let holds = state
        .issue_tree_control_service
        .list_tree_holds(issue_id)
        .await?;

    Ok(Json(TreeHoldsListResponse { holds }))
}

/// GET /issues/:id/tree-holds/:hold_id - Get a tree hold
pub async fn get_tree_hold(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, hold_id)): Path<(Uuid, Uuid)>,
    ) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Read).await?;
    let hold = state
        .issue_tree_control_service
        .get_tree_hold(hold_id)
        .await?;

    Ok(Json(TreeHoldResponse { hold }))
}

/// POST /issues/:id/tree-holds/:hold_id/release - Release a tree hold
pub async fn release_tree_hold(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, hold_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Write).await?;
    let hold = state
        .issue_tree_control_service
        .release_tree_hold(
            hold_id,
            Some(actor.actor_type().to_string()),
            actor.principal_id(),
        )
        .await?;

    Ok(Json(TreeHoldResponse { hold }))
}

/// GET /issues/:id/tree-control/state - Get current pause state
pub async fn get_pause_state(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Read).await?;
    let gate = state
        .issue_tree_control_service
        .get_pause_state(issue_id)
        .await?;

    let paused = gate.is_some();

    Ok(Json(PauseStateResponse { paused, gate }))
}

/// GET /issues/:id/tree-holds/:hold_id/members - Get hold members
pub async fn get_hold_members(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, hold_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, ApiError> {
    authorized_issue_company(&state, &actor, issue_id, AccessMode::Read).await?;
    let members = state
        .issue_tree_control_service
        .get_hold_members(hold_id)
        .await?;

    Ok(Json(TreeHoldMembersResponse { members }))
}

/// Create Issue Tree Control routes
pub fn issue_tree_control_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/issues/:id/tree-control/preview",
            post(preview_tree_control),
        )
        .route("/issues/:id/tree-control/state", get(get_pause_state))
        .route("/issues/:id/tree-holds", post(create_tree_hold))
        .route("/issues/:id/tree-holds", get(list_tree_holds))
        .route("/issues/:id/tree-holds/:hold_id", get(get_tree_hold))
        .route(
            "/issues/:id/tree-holds/:hold_id/release",
            post(release_tree_hold),
        )
        .route(
            "/issues/:id/tree-holds/:hold_id/members",
            get(get_hold_members),
        )
}
