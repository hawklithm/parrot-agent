//! API routes for the task watchdog subsystem.
//!
//! Provides CRUD operations for issue watchdogs and a manual trigger
//! endpoint to evaluate a watchdog.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::task_watchdog::{IssueWatchdog, IssueWatchdogStatus};
use services::WatchdogService;
use std::sync::Arc;

/// Request to create/upsert a watchdog for an issue.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertWatchdogRequest {
    pub watchdog_agent_id: Uuid,
    pub instructions: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_by_run_id: Option<Uuid>,
}

/// Request to update a watchdog's status.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWatchdogStatusRequest {
    pub status: IssueWatchdogStatus,
}

/// Watchdog info response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogInfo {
    pub watchdog: IssueWatchdog,
}

/// Evaluation result response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResult {
    pub evaluated_count: usize,
    pub message: String,
}

/// POST /companies/:companyId/issues/:issueId/watchdog — Upsert watchdog
async fn upsert_watchdog(
    State(watchdog_service): State<Arc<dyn WatchdogService>>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpsertWatchdogRequest>,
) -> Result<Json<WatchdogInfo>, StatusCode> {
    let watchdog = watchdog_service
        .upsert_watchdog(
            company_id,
            issue_id,
            req.watchdog_agent_id,
            req.instructions,
            req.created_by_agent_id,
            req.created_by_user_id,
            req.created_by_run_id,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WatchdogInfo { watchdog }))
}

/// GET /companies/:companyId/issues/:issueId/watchdog — Get watchdog for issue
async fn get_watchdog(
    State(watchdog_service): State<Arc<dyn WatchdogService>>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<WatchdogInfo>, StatusCode> {
    let watchdog = watchdog_service
        .get_watchdog(company_id, issue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(WatchdogInfo { watchdog }))
}

/// POST /companies/:companyId/watchdogs/evaluate — Evaluate all active watchdogs
async fn evaluate_all_watchdogs(
    State(watchdog_service): State<Arc<dyn WatchdogService>>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<EvaluateResult>, StatusCode> {
    let count = watchdog_service
        .evaluate_all(company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(EvaluateResult {
        evaluated_count: count,
        message: format!("Evaluated {} watchdog(s)", count),
    }))
}

/// POST /companies/:companyId/issues/:issueId/watchdog/evaluate — Evaluate a specific watchdog
async fn evaluate_watchdog(
    State(watchdog_service): State<Arc<dyn WatchdogService>>,
    Path((company_id, issue_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<EvaluateResult>, StatusCode> {
    let count = watchdog_service
        .evaluate_for_issue(company_id, issue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(EvaluateResult {
        evaluated_count: count,
        message: format!("Evaluated {} watchdog(s) for issue", count),
    }))
}

/// PUT /companies/:companyId/watchdogs/:watchdogId/status — Update watchdog status
async fn update_watchdog_status(
    State(watchdog_service): State<Arc<dyn WatchdogService>>,
    Path((_company_id, watchdog_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateWatchdogStatusRequest>,
) -> Result<Json<WatchdogInfo>, StatusCode> {
    let watchdog = watchdog_service
        .update_watchdog_status(watchdog_id, req.status)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(WatchdogInfo { watchdog }))
}

/// Build watchdog routes
pub fn watchdog_routes() -> Router<Arc<dyn WatchdogService>> {
    Router::new()
        .route(
            "/api/companies/:companyId/issues/:issueId/watchdog",
            get(get_watchdog).post(upsert_watchdog),
        )
        .route(
            "/api/companies/:companyId/watchdogs/evaluate",
            post(evaluate_all_watchdogs),
        )
        .route(
            "/api/companies/:companyId/issues/:issueId/watchdog/evaluate",
            post(evaluate_watchdog),
        )
        .route(
            "/api/companies/:companyId/watchdogs/:watchdogId/status",
            put(update_watchdog_status),
        )
}
