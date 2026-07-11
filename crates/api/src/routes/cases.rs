use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::{Case, CaseDetail, CaseEvent, CreateCaseInput, UpdateCaseInput};
use crate::services::{CaseQueryFilter, CaseService, Pagination};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListCasesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    status: Option<String>,
    case_type: Option<String>,
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCaseQuery {
    #[serde(default)]
    upsert: bool,
}

/// POST /companies/:companyId/cases - Create case
async fn create_case(
    State(service): State<Arc<dyn CaseService>>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<CreateCaseQuery>,
    Json(input): Json<CreateCaseInput>,
) -> Result<Json<Case>, StatusCode> {
    service
        .create(input, query.upsert)
        .await
        .map(|result| Json(result.case))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/cases - List cases
async fn list_cases(
    State(service): State<Arc<dyn CaseService>>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<Case>>, StatusCode> {
    let filter = CaseQueryFilter {
        status: None,
        case_type: query.case_type,
        project_id: query.project_id,
        parent_case_id: None,
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

/// GET /cases/:id - Get case by ID
async fn get_case(
    State(service): State<Arc<dyn CaseService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Case>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /cases/:id/detail - Get case detail with related data
async fn get_case_detail(
    State(service): State<Arc<dyn CaseService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<CaseDetail>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .get_detail(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// PATCH /cases/:id - Update case
async fn update_case(
    State(service): State<Arc<dyn CaseService>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCaseInput>,
) -> Result<Json<Case>, StatusCode> {
    let company_id = Uuid::nil();
    
    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.case))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /cases/:id/events - List case events
async fn list_case_events(
    State(service): State<Arc<dyn CaseService>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<CaseEvent>>, StatusCode> {
    let company_id = Uuid::nil();
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };
    
    service
        .list_events(id, company_id, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create case routes
pub fn case_routes(service: Arc<dyn CaseService>) -> Router {
    Router::new()
        .route("/api/companies/:companyId/cases", post(create_case).get(list_cases))
        .route("/api/cases/:id", get(get_case).patch(update_case))
        .route("/api/cases/:id/detail", get(get_case_detail))
        .route("/api/cases/:id/events", get(list_case_events))
        .with_state(service)
}
