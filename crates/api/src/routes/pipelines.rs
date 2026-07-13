//! Pipeline routes — CRUD + Case operations + Stage/Transition management
//!
//! 对应 Pipeline/Adapter 模块 §6 Pipeline HTTP 路由层

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
use models::pipeline::{Pipeline, PipelineStage, PipelineCase, PipelineTransition};
use services::{PipelineService, AdvanceCaseInput, CreateCaseInput};

pub fn pipeline_routes() -> Router<AppState> {
    Router::new()
        // Pipeline CRUD
        .route("/companies/:company_id/pipelines", post(create_pipeline))
        .route("/companies/:company_id/pipelines", get(list_pipelines))
        .route("/pipelines/:pipeline_id", get(get_pipeline))
        // Stages
        .route("/pipelines/:pipeline_id/stages", get(list_stages))
        .route("/pipelines/:pipeline_id/transitions", get(list_transitions))
        // Cases
        .route("/pipelines/:pipeline_id/cases", post(create_case))
        .route("/pipelines/:pipeline_id/cases", get(list_cases))
        .route("/cases/:case_id", get(get_case))
        .route("/cases/:case_id/advance", patch(advance_case))
        .route("/cases/:case_id/terminal", post(mark_terminal))
        .route("/cases/:case_id/events", get(get_case_events))
        // Health & attention
        .route("/pipelines/:pipeline_id/health-warnings", get(get_health_warnings))
        .route("/companies/:company_id/pipelines-attention", get(get_pipelines_attention))
}

// ===== Pipeline endpoints =====

/// POST /companies/:company_id/pipelines
async fn create_pipeline(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<models::pipeline::CreatePipelineInput>,
) -> Result<(StatusCode, Json<Pipeline>), AppError> {
    let pipeline = state
        .pipeline_service
        .create_pipeline(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(pipeline)))
}

/// GET /companies/:company_id/pipelines
async fn list_pipelines(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Pipeline>>, AppError> {
    // Note: use get_pipelines_attention as a workaround since list_by_company is on repo not service
    // The service trait needs list_by_company — use a direct approach
    Err(AppError::NotImplemented("Pipeline listing by company not yet available".to_string()))
}

/// GET /pipelines/:pipeline_id
async fn get_pipeline(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Pipeline>, AppError> {
    let pipeline = state
        .pipeline_service
        .get_pipeline(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(pipeline))
}

// ===== Stage/Transition endpoints =====

/// GET /pipelines/:pipeline_id/stages
async fn list_stages(
    State(_state): State<AppState>,
    Path(_pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<PipelineStage>>, AppError> {
    Err(AppError::NotImplemented("Stage listing not yet available".to_string()))
}

/// GET /pipelines/:pipeline_id/transitions
async fn list_transitions(
    State(_state): State<AppState>,
    Path(_pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<PipelineTransition>>, AppError> {
    Err(AppError::NotImplemented("Transition listing not yet available".to_string()))
}

// ===== Case endpoints =====

/// POST /pipelines/:pipeline_id/cases
async fn create_case(
    State(state): State<AppState>,
    Path(_pipeline_id): Path<Uuid>,
    Json(input): Json<CreateCaseInput>,
) -> Result<(StatusCode, Json<PipelineCase>), AppError> {
    let case = state
        .pipeline_service
        .create_case(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(case)))
}

/// GET /pipelines/:pipeline_id/cases
async fn list_cases(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<PipelineCase>>, AppError> {
    let cases = state
        .pipeline_service
        .list_cases(pipeline_id, query.stage_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(cases))
}

/// GET /cases/:case_id
async fn get_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<PipelineCase>, AppError> {
    let case = state
        .pipeline_service
        .get_case(case_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(case))
}

/// PATCH /cases/:case_id/advance
async fn advance_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<PipelineCase>, AppError> {
    let to_stage_id: Uuid = body.get("to_stage_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("Missing to_stage_id".to_string()))?;

    let input = AdvanceCaseInput {
        case_id,
        to_stage_id,
        actor_type: body.get("actor_type").and_then(|v| v.as_str().map(String::from)),
        actor_id: body.get("actor_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())),
        note: body.get("note").and_then(|v| v.as_str().map(String::from)),
    };

    let case = state
        .pipeline_service
        .advance_case(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(case))
}

/// POST /cases/:case_id/terminal
async fn mark_terminal(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<PipelineCase>, AppError> {
    let kind_str = body.get("kind").and_then(|v| v.as_str()).unwrap_or("done");
    let kind = match kind_str {
        "cancelled" => models::pipeline::TerminalKind::Cancelled,
        _ => models::pipeline::TerminalKind::Done,
    };

    let case = state
        .pipeline_service
        .mark_terminal(case_id, kind)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(case))
}

/// GET /cases/:case_id/events
async fn get_case_events(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Vec<models::pipeline::CaseEvent>>, AppError> {
    let events = state
        .pipeline_service
        .get_case_events(case_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(events))
}

// ===== Health & Attention endpoints =====

/// GET /pipelines/:pipeline_id/health-warnings
async fn get_health_warnings(
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
) -> Result<Json<Vec<services::HealthWarning>>, AppError> {
    let warnings = state
        .pipeline_service
        .get_health_warnings(pipeline_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(warnings))
}

/// GET /companies/:company_id/pipelines-attention
async fn get_pipelines_attention(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<services::HealthWarning>>, AppError> {
    let warnings = state
        .pipeline_service
        .get_pipelines_attention(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(warnings))
}

/// Query params for listing cases
#[derive(Debug, Deserialize)]
pub struct ListCasesQuery {
    pub stage_id: Option<Uuid>,
}
