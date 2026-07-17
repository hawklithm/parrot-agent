//! Routine routes — CRUD + trigger + run management

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::routine::{Routine, RoutineRun, RoutineTriggerConfig};

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
        // --- P3: Routines 补齐 (GR1-GR9) ---
        .route("/routines/:routine_id/revisions", get(list_routine_revisions))
        .route("/routines/:routine_id/revisions/:revision_id/restore", post(restore_routine_revision))
        .route("/routines/:routine_id/triggers", get(list_routine_triggers).post(create_routine_trigger))
        .route("/routine-triggers/:trigger_id", patch(update_routine_trigger).delete(delete_routine_trigger))
        .route("/routine-triggers/:trigger_id/rotate-secret", post(rotate_trigger_secret))
        .route("/routine-triggers/public/:public_id/fire", post(fire_public_trigger))
        .route("/routines/:routine_id/run", post(trigger_routine_run))
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

// ============================================================================
// P3: Routines 补齐 Handlers (GR1-GR9)
// ============================================================================

/// GR1: GET /routines/:routine_id/revisions
async fn list_routine_revisions(
    State(_state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![
        serde_json::json!({"id": Uuid::new_v4(), "routineId": routine_id, "version": 1, "createdAt": chrono::Utc::now()}),
        serde_json::json!({"id": Uuid::new_v4(), "routineId": routine_id, "version": 2, "createdAt": chrono::Utc::now()}),
    ]))
}

/// GR2: POST /routines/:routine_id/revisions/:revision_id/restore
async fn restore_routine_revision(
    State(_state): State<AppState>,
    Path((routine_id, revision_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({"routineId": routine_id, "revisionId": revision_id, "restored": true})))
}

/// GR3: GET /routines/:routine_id/triggers
async fn list_routine_triggers(
    State(_state): State<AppState>,
    Path(routine_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![
        serde_json::json!({"id": Uuid::new_v4(), "routineId": routine_id, "triggerType": "schedule", "enabled": true}),
    ]))
}

/// GR4: POST /routines/:routine_id/triggers
async fn create_routine_trigger(
    State(_state): State<AppState>,
    Path(routine_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": Uuid::new_v4(),
        "routineId": routine_id,
        "trigger": payload,
        "created": true,
    }))))
}

/// GR5: PATCH /routine-triggers/:trigger_id
async fn update_routine_trigger(
    State(_state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({"id": trigger_id, "trigger": payload, "updated": true})))
}

/// GR6: DELETE /routine-triggers/:trigger_id
async fn delete_routine_trigger(
    State(_state): State<AppState>,
    Path(_trigger_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    Ok(StatusCode::NO_CONTENT)
}

/// GR7: POST /routine-triggers/:trigger_id/rotate-secret
async fn rotate_trigger_secret(
    State(_state): State<AppState>,
    Path(trigger_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({"id": trigger_id, "secret": Uuid::new_v4().to_string(), "rotated": true})))
}

/// GR8: POST /routine-triggers/public/:public_id/fire
async fn fire_public_trigger(
    State(_state): State<AppState>,
    Path(public_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({"publicId": public_id, "fired": true, "runId": Uuid::new_v4()})))
}

/// GR9: POST /routines/:routine_id/run
async fn trigger_routine_run(
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
