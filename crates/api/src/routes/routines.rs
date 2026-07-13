//! Routine routes — CRUD + trigger + run management

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::routine::{Routine, RoutineRun, RoutineTriggerConfig};
use services::RoutineService;

pub fn routine_routes() -> Router<AppState> {
    Router::new()
        // Routine CRUD
        .route("/companies/:company_id/routines", get(list_routines).post(create_routine))
        .route("/routines/:routine_id", get(get_routine).patch(update_routine).delete(delete_routine))
        .route("/routines/:routine_id/pause", post(pause_routine))
        .route("/routines/:routine_id/resume", post(resume_routine))
        .route("/routines/:routine_id/trigger", post(trigger_routine))
        // Runs
        .route("/routines/:routine_id/runs", get(list_runs))
        .route("/runs/:run_id", get(get_run))
}

/// POST /companies/:company_id/routines
async fn create_routine(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<Routine>), AppError> {
    let agent_id: Uuid = body.get("agent_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());
    let name = body.get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let description = body.get("description").and_then(|v| v.as_str().map(String::from));
    let trigger_config: RoutineTriggerConfig = serde_json::from_value(
        body.get("trigger_config").cloned().unwrap_or(serde_json::json!({}))
    ).map_err(|e| AppError::BadRequest(format!("Invalid trigger_config: {}", e)))?;

    let routine = state
        .routine_service
        .create_routine(company_id, agent_id, name, description, trigger_config, Uuid::nil())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(routine)))
}

/// GET /companies/:company_id/routines
async fn list_routines(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<Routine>>, AppError> {
    let routines = state
        .routine_service
        .list_routines(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routines))
}

/// GET /routines/:routine_id
async fn get_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .get_by_id(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// PATCH /routines/:routine_id
async fn update_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Routine>, AppError> {
    let name = body.get("name").and_then(|v| v.as_str().map(String::from));
    let description = body.get("description").and_then(|v| v.as_str().map(String::from));
    let routine = state
        .routine_service
        .update_routine(routine_id, name, description)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// DELETE /routines/:routine_id
async fn delete_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .routine_service
        .delete_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /routines/:routine_id/pause
async fn pause_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .pause_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// POST /routines/:routine_id/resume
async fn resume_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Routine>, AppError> {
    let routine = state
        .routine_service
        .resume_routine(routine_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(routine))
}

/// POST /routines/:routine_id/trigger
async fn trigger_routine(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<RoutineRun>, AppError> {
    let run = state
        .routine_service
        .trigger_routine(routine_id, "manual".to_string())
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(run))
}

/// GET /routines/:routine_id/runs
async fn list_runs(
    State(state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<RoutineRun>>, AppError> {
    let runs = state
        .routine_service
        .list_runs(routine_id, 50)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(runs))
}

/// GET /runs/:run_id
async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<RoutineRun>, AppError> {
    let run = state
        .routine_service
        .get_run(run_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Run {} not found", run_id)))?;
    Ok(Json(run))
}
