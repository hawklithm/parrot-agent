//! Company routes — CRUD + stats + branding + archive
//!
//! 对应 Company/Org 模块任务 §1.1 ~ §1.3 + §10 API 路由层

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::{
    Company, CompanyStats, CreateCompanyInput, UpdateCompanyInput,
    CompanyStatus,
};
use services::CompanyService;

pub fn company_routes() -> Router<AppState> {
    Router::new()
        // Company list + create
        .route("/companies", get(list_companies).post(create_company))
        // Company stats
        .route("/companies/stats", get(get_company_stats))
        // Single company operations
        .route("/companies/:company_id", get(get_company).patch(update_company).delete(delete_company))
        // Company branding
        .route("/companies/:company_id/branding", patch(update_company_branding))
        // Company archive
        .route("/companies/:company_id/archive", post(archive_company))
}

#[derive(Debug, Deserialize)]
pub struct ListCompaniesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /companies
async fn list_companies(
    State(state): State<AppState>,
    Query(query): Query<ListCompaniesQuery>,
) -> Result<Json<Vec<Company>>, AppError> {
    let companies = state
        .company_service
        .list(query.limit.unwrap_or(50), query.offset.unwrap_or(0))
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(companies))
}

/// POST /companies
async fn create_company(
    State(state): State<AppState>,
    Json(input): Json<CreateCompanyInput>,
) -> Result<(StatusCode, Json<Company>), AppError> {
    // TODO: Extract creator_user_id from auth context
    let creator_user_id = Uuid::nil();
    let company = state
        .company_service
        .create(input, creator_user_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(company)))
}

/// GET /companies/stats
async fn get_company_stats(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Implement global company stats
    Ok(Json(serde_json::json!({
        "total_companies": 0,
        "active_companies": 0,
    })))
}

/// GET /companies/:company_id
async fn get_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Company>, AppError> {
    let company = state
        .company_service
        .get_by_id(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    Ok(Json(company))
}

/// PATCH /companies/:company_id
async fn update_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<UpdateCompanyInput>,
) -> Result<Json<Company>, AppError> {
    let company = state
        .company_service
        .update(company_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}

/// DELETE /companies/:company_id
async fn delete_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .company_service
        .delete(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /companies/:company_id/branding
async fn update_company_branding(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<Company>, AppError> {
    let brand_color = input.get("brand_color").and_then(|v| v.as_str().map(String::from));
    let logo_asset_id = input.get("logo_asset_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()));
    let company = state
        .company_service
        .update_branding(company_id, brand_color, logo_asset_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}

/// POST /companies/:company_id/archive
async fn archive_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Company>, AppError> {
    let company = state
        .company_service
        .archive(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}
