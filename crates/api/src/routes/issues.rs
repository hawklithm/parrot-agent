use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use crate::app_state::AppState;
use uuid::Uuid;

use models::{CreateIssueInput, Issue, IssueStatus, UpdateIssueInput};
use services::{
    CheckoutInput, IssueQueryFilter, IssueService, Pagination, ReleaseInput,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListIssuesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    status: Option<String>,
    priority: Option<String>,
    assignee_agent_id: Option<Uuid>,
    assignee_user_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

/// GET /issues - List all issues
async fn list_issues(
    State(state): State<AppState>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();
    
    let filter = IssueQueryFilter {
        status: None,
        priority: None,
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: None,
    };
    
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };
    
    service
        .list(company_id, &filter, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/:id - Get issue by ID
async fn get_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// POST /companies/:companyId/issues - Create issue
async fn create_issue(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    service
        .create(input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /issues/:id - Update issue
async fn update_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /issues/:id - Delete issue
async fn delete_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .delete(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/issues/count - Count issues
async fn count_issues(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"count": 0})))
}

/// GET /companies/:companyId/issues/search - Search issues
async fn search_issues(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let filter = IssueQueryFilter::default();
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: 0,
        cursor: None,
    };
    
    service
        .search(company_id, &query.q, &filter, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/checkout - Checkout issue
async fn checkout_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<CheckoutInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .checkout(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/release - Release issue
async fn release_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .release(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/admin/force-release - Force release issue (admin only)
async fn force_release_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<services::ForceReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    // Validate force release schema
    let schema = crate::validation::ForceReleaseSchema {
        admin_user_id: input.admin_user_id,
        reason: input.reason.clone(),
        release_lease: Some(input.release_lease),
    };
    if let Err(e) = schema.validate() {
        return Err(StatusCode::BAD_REQUEST);
    }

    service
        .force_release(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /companies/:companyId/issues/batch-update - Batch update issues
async fn batch_update_issues(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<crate::validation::BatchIssueUpdateSchema>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();

    // Validate batch update schema
    if let Err(e) = input.validate() {
        return Err(StatusCode::BAD_REQUEST);
    }

    service
        .batch_update(company_id, input.issue_ids, input.status, input.priority, input.assignee_agent_id, input.assignee_user_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/heartbeat-context - Get heartbeat context for issue
async fn get_heartbeat_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .get_heartbeat_context(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create issue routes
pub fn issue_routes() -> Router<AppState> {
    Router::new()
        .route("/api/issues", get(list_issues))
        .route("/api/issues/:id", get(get_issue).patch(update_issue).delete(delete_issue))
        .route("/api/companies/:companyId/issues", post(create_issue))
        .route("/api/companies/:companyId/issues/count", get(count_issues))
        .route("/api/companies/:companyId/issues/search", get(search_issues))
        .route("/api/issues/:id/checkout", post(checkout_issue))
        .route("/api/issues/:id/release", post(release_issue))
        .route("/api/issues/:id/admin/force-release", post(force_release_issue))
        .route("/api/companies/:companyId/issues/batch-update", post(batch_update_issues))
        .route("/api/issues/:id/heartbeat-context", get(get_heartbeat_context))
}
