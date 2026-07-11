use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{Issue, IssueStatus, IssuePriority, IssueWorkMode, Pagination};
use services::IssueServiceError;
use crate::{errors::ApiError, app_state::AppState};

/// Create Issue request
#[derive(Debug, Deserialize)]
pub struct CreateIssueRequest {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub work_mode: Option<IssueWorkMode>,
    pub parent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
}

/// Update Issue request
#[derive(Debug, Deserialize)]
pub struct UpdateIssueRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub work_mode: Option<IssueWorkMode>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
}

/// List Issues query parameters
#[derive(Debug, Deserialize)]
pub struct ListIssuesQuery {
    pub company_id: Uuid,
    pub status: Option<String>, // Comma-separated statuses
    pub priority: Option<String>, // Comma-separated priorities
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Search Issues query parameters
#[derive(Debug, Deserialize)]
pub struct SearchIssuesQuery {
    pub company_id: Uuid,
    pub q: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Issue response
#[derive(Debug, Serialize)]
pub struct IssueResponse {
    pub issue: Issue,
}

/// Issues list response
#[derive(Debug, Serialize)]
pub struct IssuesListResponse {
    pub issues: Vec<Issue>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// Convert service errors to API errors
impl From<IssueServiceError> for ApiError {
    fn from(err: IssueServiceError) -> Self {
        match err {
            IssueServiceError::NotFound(id) => {
                ApiError::NotFound(format!("Issue not found: {}", id))
            }
            IssueServiceError::InvalidStateTransition { from, to } => {
                ApiError::BadRequest(format!("Invalid state transition from {:?} to {:?}", from, to))
            }
            IssueServiceError::CircularReference => {
                ApiError::BadRequest("Circular reference detected in issue tree".to_string())
            }
            IssueServiceError::DepthLimitExceeded { max } => {
                ApiError::BadRequest(format!("Issue tree depth limit exceeded (max: {})", max))
            }
            IssueServiceError::CrossCompanyParent => {
                ApiError::BadRequest("Parent issue belongs to different company".to_string())
            }
            IssueServiceError::Validation(msg) => {
                ApiError::BadRequest(msg)
            }
            IssueServiceError::Repository(repo_err) => {
                ApiError::InternalServerError(format!("Database error: {}", repo_err))
            }
        }
    }
}

/// POST /issues - Create a new issue
pub async fn create_issue(
    State(state): State<AppState>,
    Json(req): Json<CreateIssueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = models::CreateIssueInput {
        company_id: req.company_id,
        project_id: req.project_id,
        project_workspace_id: None,
        goal_id: req.goal_id,
        parent_id: req.parent_id,
        title: req.title,
        description: req.description,
        status: req.status.unwrap_or(IssueStatus::Backlog),
        priority: req.priority.unwrap_or(IssuePriority::Medium),
        work_mode: req.work_mode.unwrap_or(IssueWorkMode::Standard),
        assignee_agent_id: None,
        assignee_user_id: None,
        created_by_agent_id: None,
        created_by_user_id: None,
        responsible_user_id: None,
        origin_kind: None,
        origin_id: None,
        origin_run_id: None,
        request_depth: None,
        billing_code: None,
        assignee_adapter_overrides: None,
        execution_workspace_id: None,
        execution_workspace_preference: None,
    };

    let issue = state.issue_service.create(input).await?;

    Ok((StatusCode::CREATED, Json(IssueResponse { issue })))
}

/// GET /issues/:id - Get issue by ID
pub async fn get_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let issue = state.issue_service.get(id).await?;

    Ok(Json(IssueResponse { issue }))
}

/// GET /issues - List issues with filtering and pagination
pub async fn list_issues(
    State(state): State<AppState>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Parse status filter
    let status_filter = if let Some(status_str) = query.status {
        let statuses: Result<Vec<IssueStatus>, _> = status_str
            .split(',')
            .map(|s| serde_json::from_value(serde_json::Value::String(s.to_string())))
            .collect();
        Some(statuses.map_err(|_| ApiError::BadRequest("Invalid status values".to_string()))?)
    } else {
        None
    };

    // Parse priority filter
    let priority_filter = if let Some(priority_str) = query.priority {
        let priorities: Result<Vec<IssuePriority>, _> = priority_str
            .split(',')
            .map(|s| serde_json::from_value(serde_json::Value::String(s.to_string())))
            .collect();
        Some(priorities.map_err(|_| ApiError::BadRequest("Invalid priority values".to_string()))?)
    } else {
        None
    };

    let filter = models::IssueQueryFilter {
        status: status_filter,
        priority: priority_filter,
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        goal_id: query.goal_id,
        parent_id: query.parent_id,
        work_mode: None,
    };

    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).min(100),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };

    let issues = state.issue_service.list(query.company_id, &filter, &pagination).await?;
    let total = state.issue_service.count(query.company_id, &filter).await?;

    Ok(Json(IssuesListResponse {
        issues,
        total,
        limit: pagination.limit,
        offset: pagination.offset,
    }))
}

/// PUT /issues/:id - Update an issue
pub async fn update_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIssueRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = models::UpdateIssueInput {
        title: req.title,
        description: req.description,
        status: req.status,
        priority: req.priority,
        work_mode: req.work_mode,
        assignee_agent_id: req.assignee_agent_id,
        assignee_user_id: req.assignee_user_id,
        responsible_user_id: None,
        execution_policy: None,
        execution_state: None,
        monitor_notes: None,
        monitor_scheduled_by: None,
        execution_workspace_preference: None,
        execution_workspace_settings: None,
        hidden_at: None,
        source_trust: None,
    };

    let issue = state.issue_service.update(id, input).await?;

    Ok(Json(IssueResponse { issue }))
}

/// DELETE /issues/:id - Delete an issue
pub async fn delete_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.issue_service.delete(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /issues/search - Search issues by text
pub async fn search_issues(
    State(state): State<AppState>,
    Query(query): Query<SearchIssuesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).min(100),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };

    let issues = state.issue_service.search(query.company_id, &query.q, &pagination).await?;

    // Get total count for search results (approximate)
    let total = issues.len() as i64;

    Ok(Json(IssuesListResponse {
        issues,
        total,
        limit: pagination.limit,
        offset: pagination.offset,
    }))
}

/// GET /issues/identifier/:identifier - Get issue by identifier
pub async fn get_issue_by_identifier(
    State(state): State<AppState>,
    Path(identifier): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let issue = state.issue_service.get_by_identifier(&identifier).await?;

    Ok(Json(IssueResponse { issue }))
}

/// GET /issues/:id/children - List child issues
pub async fn list_child_issues(
    State(state): State<AppState>,
    Path(parent_id): Path<Uuid>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).min(100),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };

    let issues = state.issue_service.list_children(parent_id, &pagination).await?;
    let total = issues.len() as i64; // Approximate

    Ok(Json(IssuesListResponse {
        issues,
        total,
        limit: pagination.limit,
        offset: pagination.offset,
    }))
}

/// Create Issue routes
pub fn issue_routes() -> Router<AppState> {
    Router::new()
        .route("/issues", post(create_issue))
        .route("/issues/:id", get(get_issue))
        .route("/issues", get(list_issues))
        .route("/issues/:id", put(update_issue))
        .route("/issues/:id", delete(delete_issue))
        .route("/issues/search", get(search_issues))
        .route("/issues/identifier/:identifier", get(get_issue_by_identifier))
        .route("/issues/:parent_id/children", get(list_child_issues))
}
