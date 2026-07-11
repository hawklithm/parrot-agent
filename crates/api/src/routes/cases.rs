use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{Case, CaseStatus, CaseDetail, Pagination};
use services::CaseServiceError;
use crate::{errors::ApiError, app_state::AppState};

/// Create Case request
#[derive(Debug, Deserialize)]
pub struct CreateCaseRequest {
    pub company_id: Uuid,
    pub external_key: String,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<CaseStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub source: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Upsert Case request
#[derive(Debug, Deserialize)]
pub struct UpsertCaseRequest {
    pub company_id: Uuid,
    pub external_key: String,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<CaseStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub source: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Update Case request
#[derive(Debug, Deserialize)]
pub struct UpdateCaseRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<CaseStatus>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}

/// List Cases query parameters
#[derive(Debug, Deserialize)]
pub struct ListCasesQuery {
    pub company_id: Uuid,
    pub status: Option<String>, // Comma-separated statuses
    pub source: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Case response
#[derive(Debug, Serialize)]
pub struct CaseResponse {
    pub case: Case,
}

/// Case detail response
#[derive(Debug, Serialize)]
pub struct CaseDetailResponse {
    pub detail: CaseDetail,
}

/// Upsert response
#[derive(Debug, Serialize)]
pub struct UpsertCaseResponse {
    pub case: Case,
    pub created: bool,
}

/// Cases list response
#[derive(Debug, Serialize)]
pub struct CasesListResponse {
    pub cases: Vec<Case>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// Convert service errors to API errors
impl From<CaseServiceError> for ApiError {
    fn from(err: CaseServiceError) -> Self {
        match err {
            CaseServiceError::NotFound(id) => {
                ApiError::NotFound(format!("Case not found: {}", id))
            }
            CaseServiceError::InvalidStatusTransition { from, to } => {
                ApiError::BadRequest(format!("Invalid status transition from {:?} to {:?}", from, to))
            }
            CaseServiceError::Validation(msg) => {
                ApiError::BadRequest(msg)
            }
            CaseServiceError::Repository(repo_err) => {
                ApiError::InternalServerError(format!("Database error: {}", repo_err))
            }
        }
    }
}

/// POST /cases - Create a new case
pub async fn create_case(
    State(state): State<AppState>,
    Json(req): Json<CreateCaseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = models::CreateCaseInput {
        company_id: req.company_id,
        external_key: req.external_key,
        title: req.title,
        description: req.description,
        status: req.status.unwrap_or(CaseStatus::Draft),
        priority: req.priority.unwrap_or(3),
        tags: req.tags.unwrap_or_default(),
        source: req.source,
        metadata: req.metadata,
    };

    let case = state.case_service.create(input).await?;

    Ok((StatusCode::CREATED, Json(CaseResponse { case })))
}

/// POST /cases/upsert - Upsert a case by external_key
pub async fn upsert_case(
    State(state): State<AppState>,
    Json(req): Json<UpsertCaseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = models::UpsertCaseInput {
        company_id: req.company_id,
        external_key: req.external_key,
        title: req.title,
        description: req.description,
        status: req.status,
        priority: req.priority,
        tags: req.tags,
        source: req.source,
        metadata: req.metadata,
    };

    let (case, created) = state.case_service.upsert(input).await?;

    let status_code = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((status_code, Json(UpsertCaseResponse { case, created })))
}

/// GET /cases/:id - Get case by ID
pub async fn get_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let case = state.case_service.get(id).await?;

    Ok(Json(CaseResponse { case }))
}

/// GET /cases/:id/detail - Load full case detail with events, links, attachments
pub async fn get_case_detail<CS: CaseService>(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let detail = state.case_service.load_detail(id).await?;

    Ok(Json(CaseDetailResponse { detail }))
}

/// GET /cases - List cases with filtering and pagination
pub async fn list_cases<CS: CaseService>(
    State(state): State<AppState>,
    Query(query): Query<ListCasesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    // Parse status filter
    let status_filter = if let Some(status_str) = query.status {
        let statuses: Result<Vec<CaseStatus>, _> = status_str
            .split(',')
            .map(|s| serde_json::from_value(serde_json::Value::String(s.to_string())))
            .collect();
        Some(statuses.map_err(|_| ApiError::BadRequest("Invalid status values".to_string()))?)
    } else {
        None
    };

    let filter = models::CaseQueryFilter {
        status: status_filter,
        source: query.source,
        tags: None,
    };

    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).min(100),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };

    let cases = state.case_service.list(query.company_id, &filter, &pagination).await?;
    let total = state.case_service.count(query.company_id, &filter).await?;

    Ok(Json(CasesListResponse {
        cases,
        total,
        limit: pagination.limit,
        offset: pagination.offset,
    }))
}

/// PUT /cases/:id - Update a case
pub async fn update_case<CS: CaseService>(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateCaseRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let input = models::UpdateCaseInput {
        title: req.title,
        description: req.description,
        status: req.status,
        priority: req.priority,
        tags: req.tags,
        metadata: req.metadata,
    };

    let case = state.case_service.update(id, input).await?;

    Ok(Json(CaseResponse { case }))
}

/// DELETE /cases/:id - Delete a case
pub async fn delete_case<CS: CaseService>(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    state.case_service.delete(id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /cases/external/:external_key - Get case by external key
pub async fn get_case_by_external_key<CS: CaseService>(
    State(state): State<AppState>,
    Path((company_id, external_key)): Path<(Uuid, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let case = state.case_service.get_by_external_key(company_id, &external_key).await?;

    Ok(Json(CaseResponse { case }))
}

/// Create Case routes
pub fn case_routes() -> Router<AppState> {
    Router::new()
        .route("/cases", post(create_case))
        .route("/cases/upsert", post(upsert_case))
        .route("/cases/:id", get(get_case))
        .route("/cases/:id/detail", get(get_case_detail))
        .route("/cases", get(list_cases))
        .route("/cases/:id", put(update_case))
        .route("/cases/:id", delete(delete_case))
        .route("/cases/external/:company_id/:external_key", get(get_case_by_external_key))
}
