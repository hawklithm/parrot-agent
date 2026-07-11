use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::{CreateIssueInput, Issue, UpdateIssueInput};
use crate::services::{
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
    State(service): State<Arc<dyn IssueService>>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
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
    State(service): State<Arc<dyn IssueService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Issue>, StatusCode> {
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
    State(service): State<Arc<dyn IssueService>>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    service
        .create(input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /issues/:id - Update issue
async fn update_issue(
    State(service): State<Arc<dyn IssueService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /issues/:id - Delete issue
async fn delete_issue(
    State(service): State<Arc<dyn IssueService>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .delete(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/issues/count - Count issues
async fn count_issues(
    State(_service): State<Arc<dyn IssueService>>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"count": 0})))
}

/// GET /companies/:companyId/issues/search - Search issues
async fn search_issues(
    State(service): State<Arc<dyn IssueService>>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
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
    State(service): State<Arc<dyn IssueService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<CheckoutInput>,
) -> Result<Json<Issue>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .checkout(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/release - Release issue
async fn release_issue(
    State(service): State<Arc<dyn IssueService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .release(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Cr issue routes
pub fn issue_routes(service: Arc<dyn IssueService>) -> Router {
    Router::new()
        .route("/api/issues", get(list_issues))
        .route("/api/issues/:id", get(get_issue).patch(update_issue).delete(delete_issue))
        .route("/api/companies/:companyId/issues", post(create_issue))
        .route("/api/companies/:companyId/issues/count", get(count_issues))
        .route("/api/companies/:companyId/issues/search", get(search_issues))
        .route("/api/issues/:id/checkout", post(checkout_issue))
        .route("/api/issues/:id/release", post(release_issue))
        .with_state(service)
}
