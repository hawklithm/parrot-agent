//! Background-task scheduler observability (PAPERCLIP_MIGRATION_PLAN §4B.3 line 357).
//!
//! Exposes the JobScheduler's runtime state over HTTP: registered job metadata
//! (name, schedule), recent persisted executions, persisted leases, and manual
//! trigger. The scheduler is optional in AppState (None in test/embedded
//! contexts) — routes then return 404.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn scheduler_routes() -> Router<AppState> {
    Router::new()
        .route("/scheduler/jobs", get(list_jobs))
        .route("/scheduler/jobs/:name/executions", get(job_executions))
        .route("/scheduler/leases", get(list_leases))
        .route("/scheduler/jobs/:name/trigger", post(trigger_job))
}

fn scheduler(
    state: &AppState,
) -> Result<&std::sync::Arc<services::JobScheduler>, StatusCode> {
    state.scheduler.as_ref().ok_or(StatusCode::NOT_FOUND)
}

/// GET /scheduler/jobs — registered jobs with schedule.
async fn list_jobs(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let sched = scheduler(&state)?;
    let metadata = sched.list_job_metadata().await;
    Ok(Json(
        metadata
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "jobName": m.job_name,
                    "schedule": format!("{:?}", m.schedule),
                })
            })
            .collect::<Vec<_>>(),
    ))
}

/// GET /scheduler/jobs/:name/executions — recent persisted executions for a job.
async fn job_executions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sched = scheduler(&state)?;
    let recent = sched.get_recent_executions(50).await;
    let filtered = recent
        .into_iter()
        .filter(|e| e.job_name == name)
        .map(|e| {
            serde_json::json!({
                "jobName": e.job_name,
                "status": format!("{:?}", e.status),
                "startedAt": e.started_at,
                "completedAt": e.completed_at,
                "error": e.error_message,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "jobName": name, "executions": filtered })))
}

/// GET /scheduler/leases — persisted cross-process leases.
async fn list_leases(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let sched = scheduler(&state)?;
    let leases = sched
        .load_persisted_leases()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "failed to load scheduler leases");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({
        "leases": leases.iter().map(|l| serde_json::json!({
            "jobName": l.job_name,
            "ownerId": l.owner_id,
            "leasedUntil": l.leased_until,
            "heartbeatAt": l.heartbeat_at,
        })).collect::<Vec<_>>(),
        "count": leases.len(),
    })))
}

/// POST /scheduler/jobs/:name/trigger — manual trigger (respects running guard
/// and lease; returns existing-run outcome when already active).
async fn trigger_job(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sched = scheduler(&state)?;
    let outcome = sched.trigger_job(&name).await.map_err(|e| {
        tracing::warn!(job = %name, error = %e, "manual scheduler trigger failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({
        "jobName": name,
        "triggerId": Uuid::new_v4(),
        "outcome": outcome,
    })))
}
