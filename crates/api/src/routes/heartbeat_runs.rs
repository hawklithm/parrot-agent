//! Heartbeat-run routes — 补齐 X1-X11, X14.
//!
//! 对应 FEATURE_GAP_TASKS.md §3.3 Executions/Runs (X1-X11, X14) 及
//! API_GAP_TASKS.md §3.5 heartbeat-run 路由。
//!
//! 路由路径与 Paperclip `server/src/routes/agents.ts` (heartbeat-run block)
//! 及 `server/src/routes/activity.ts` 对齐：
//!   GET    /companies/:company_id/heartbeat-runs        (X1)
//!   GET    /companies/:company_id/live-runs             (X2)
//!   GET    /heartbeat-runs/:run_id                      (X3)
//!   POST   /heartbeat-runs/:run_id/cancel               (X4)
//!   GET    /heartbeat-runs/:run_id/events               (X5)
//!   GET    /heartbeat-runs/:run_id/log                  (X6)
//!   GET    /heartbeat-runs/:run_id/issues               (X7)
//!   GET    /heartbeat-runs/:run_id/watchdog-decisions   (X8)
//!   POST   /heartbeat-runs/:run_id/watchdog-decisions   (X9)
//!   GET    /heartbeat-runs/:run_id/workspace-operations (X10)
//!   GET    /workspace-operations/:operation_id/log      (X11)
//!   GET    /issues/:id/runs                             (X14)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;

pub fn heartbeat_run_routes() -> Router<AppState> {
    Router::new()
        // X1: company heartbeat-run list
        .route(
            "/companies/:company_id/heartbeat-runs",
            get(list_company_heartbeat_runs),
        )
        // X2: company live-runs
        .route(
            "/companies/:company_id/live-runs",
            get(list_company_live_runs),
        )
        // X3/X4: run detail + cancel
        .route(
            "/heartbeat-runs/:run_id",
            get(get_heartbeat_run),
        )
        .route(
            "/heartbeat-runs/:run_id/cancel",
            post(cancel_heartbeat_run),
        )
        // X5-X10: run sub-resources
        .route(
            "/heartbeat-runs/:run_id/events",
            get(list_run_events),
        )
        .route(
            "/heartbeat-runs/:run_id/log",
            get(get_run_log),
        )
        .route(
            "/heartbeat-runs/:run_id/issues",
            get(list_run_issues),
        )
        .route(
            "/heartbeat-runs/:run_id/watchdog-decisions",
            get(list_watchdog_decisions).post(submit_watchdog_decision),
        )
        .route(
            "/heartbeat-runs/:run_id/workspace-operations",
            get(list_run_workspace_operations),
        )
        // X11: workspace-operation log
        .route(
            "/workspace-operations/:operation_id/log",
            get(get_workspace_operation_log),
        )
        // X14: issue run history
        .route("/issues/:id/runs", get(list_issue_runs))
}

/// Query params for the company heartbeat-run list (Paperclip).
#[derive(Debug, Default, Deserialize)]
pub struct HeartbeatRunListQuery {
    pub agent_id: Option<Uuid>,
    pub limit: Option<i64>,
    /// `"true"`/`"1"` requests the summary projection.
    pub summary: Option<String>,
}

/// Query params for live-runs (Paperclip).
#[derive(Debug, Default, Deserialize)]
pub struct LiveRunsQuery {
    pub min_count: Option<i64>,
    pub limit: Option<i64>,
}

/// Run log query (Paperclip).
#[derive(Debug, Default, Deserialize)]
pub struct RunLogQuery {
    pub offset: Option<i64>,
    pub limit_bytes: Option<i64>,
}

/// Run events query (Paperclip).
#[derive(Debug, Default, Deserialize)]
pub struct RunEventsQuery {
    pub after_seq: Option<i64>,
    pub limit: Option<i64>,
}

/// Watchdog decision submission body (Paperclip).
#[derive(Debug, Deserialize)]
pub struct WatchdogDecisionInput {
    pub decision: String,
    pub evaluation_issue_id: Option<Uuid>,
    pub reason: Option<String>,
    pub snoozed_until: Option<String>,
}

/// Serialize a `heartbeat_runs` row to the Paperclip-shaped JSON projection.
fn run_to_json(r: &sqlx::postgres::PgRow) -> Value {
    let context_snapshot: Option<Value> = r.try_get("context_snapshot").unwrap_or(None);
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "agentId": r.get::<Uuid, _>("agent_id"),
        "invocationSource": r.get::<String, _>("invocation_source"),
        "status": r.get::<String, _>("status"),
        "responsibleUserId": r.get::<Option<String>, _>("responsible_user_id"),
        "startedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
        "finishedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
        "error": r.get::<Option<String>, _>("error"),
        "exitCode": r.get::<Option<i32>, _>("exit_code"),
        "contextSnapshot": context_snapshot.clone(),
        "issueId": context_snapshot.as_ref().and_then(|c| c.get("issueId")).cloned(),
        "taskId": context_snapshot.as_ref().and_then(|c| c.get("taskId")).cloned(),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

const RUN_SELECT: &str = r#"SELECT id, company_id, agent_id, invocation_source, status::text,
       responsible_user_id, started_at, finished_at, error, exit_code,
       context_snapshot, created_at, updated_at
  FROM heartbeat_runs"#;

/// X1: GET /companies/:company_id/heartbeat-runs
async fn list_company_heartbeat_runs(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<HeartbeatRunListQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);

    let rows = if let Some(agent_id) = q.agent_id {
        sqlx::query(&format!(
            "{} WHERE company_id = $1 AND agent_id = $2 ORDER BY created_at DESC LIMIT $3",
            RUN_SELECT
        ))
        .bind(company_id)
        .bind(agent_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(&format!(
            "{} WHERE company_id = $1 ORDER BY created_at DESC LIMIT $2",
            RUN_SELECT
        ))
        .bind(company_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let summary = matches!(q.summary.as_deref(), Some("true") | Some("1"));
    let runs: Vec<Value> = rows
        .iter()
        .map(|r| {
            if summary {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "agentId": r.get::<Uuid, _>("agent_id"),
                    "status": r.get::<String, _>("status"),
                    "startedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
                    "finishedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
                })
            } else {
                run_to_json(r)
            }
        })
        .collect();

    Ok(Json(Value::Array(runs)))
}

/// X2: GET /companies/:company_id/live-runs
///
/// Returns live (queued|running) runs, optionally padding with recent terminal
/// runs up to `min_count` (Paperclip dashboard semantics).
async fn list_company_live_runs(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<LiveRunsQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let limit = q.limit.unwrap_or(50).clamp(1, 1000);
    let min_count = q.min_count.unwrap_or(0).max(0).min(limit);

    let live_rows = sqlx::query(&format!(
        "{} WHERE company_id = $1 AND status IN ('queued','running') ORDER BY created_at DESC LIMIT $2",
        RUN_SELECT
    ))
    .bind(company_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let mut runs: Vec<Value> = live_rows.iter().map(run_to_json).collect();

    if min_count > 0 && (runs.len() as i64) < min_count {
        let active_ids: Vec<String> = runs
            .iter()
            .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        let need = min_count - runs.len() as i64;
        let recent = sqlx::query(&format!(
            "{} WHERE company_id = $1 AND status NOT IN ('queued','running') \
             AND ($2::text[] IS NULL OR NOT (id::text = ANY($2::text[]))) \
             ORDER BY created_at DESC LIMIT $3",
            RUN_SELECT
        ))
        .bind(company_id)
        .bind(&active_ids[..])
        .bind(need)
        .fetch_all(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
        runs.extend(recent.iter().map(run_to_json));
    }

    // Decorate with the Paperclip `outputSilence` placeholder.
    let decorated: Vec<Value> = runs
        .into_iter()
        .map(|mut r| {
            if let Some(obj) = r.as_object_mut() {
                obj.insert("outputSilence".to_string(), json!(null));
            }
            r
        })
        .collect();

    Ok(Json(Value::Array(decorated)))
}

/// X3: GET /heartbeat-runs/:run_id
async fn get_heartbeat_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let row = sqlx::query(&format!("{} WHERE id = $1", RUN_SELECT))
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    match row {
        Some(r) => {
            let mut run = run_to_json(&r);
            run["retryExhaustedReason"] = json!(null);
            run["outputSilence"] = json!(null);
            Ok(Json(run))
        }
        None => Err(HeartbeatRunError::NotFound(run_id)),
    }
}

/// X4: POST /heartbeat-runs/:run_id/cancel
async fn cancel_heartbeat_run(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let row = sqlx::query(
        r#"UPDATE heartbeat_runs
              SET status = 'cancelled',
                  finished_at = COALESCE(finished_at, NOW()),
                  updated_at = NOW()
            WHERE id = $1 AND status IN ('queued','running')
          RETURNING id, company_id, agent_id, invocation_source, status::text,
                    responsible_user_id, started_at, finished_at, error, exit_code,
                    context_snapshot, created_at, updated_at"#,
    )
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    match row {
        Some(r) => Ok(Json(run_to_json(&r))),
        // Either not found, or already terminal — fetch current state for idempotency.
        None => {
            let existing = sqlx::query(&format!("{} WHERE id = $1", RUN_SELECT))
                .bind(run_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
            match existing {
                Some(r) => Ok(Json(run_to_json(&r))),
                None => Err(HeartbeatRunError::NotFound(run_id)),
            }
        }
    }
}

/// X5: GET /heartbeat-runs/:run_id/events
///
/// Parrot Agent does not yet persist a `heartbeat_run_events` table; returns
/// an empty list consistent with the stub-handler convention.
async fn list_run_events(
    State(_state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Query(q): Query<RunEventsQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let after_seq = q.after_seq.unwrap_or(0);
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    Ok(Json(json!({
        "runId": run_id,
        "afterSeq": after_seq,
        "limit": limit,
        "events": [],
    })))
}

/// X6: GET /heartbeat-runs/:run_id/log
async fn get_run_log(
    State(_state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Query(q): Query<RunLogQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let offset = q.offset.unwrap_or(0).max(0);
    let limit_bytes = q.limit_bytes.unwrap_or(256 * 1024).clamp(1, 16 * 1024 * 1024);
    // Parrot Agent does not yet persist run log bytes; return an empty log
    // envelope matching Paperclip's `readLog` shape.
    Ok(Json(json!({
        "runId": run_id,
        "offset": offset,
        "limitBytes": limit_bytes,
        "bytes": "",
        "eof": true,
    })))
}

/// X7: GET /heartbeat-runs/:run_id/issues
///
/// Returns the issues associated with this run — the issue referenced by the
/// run's `context_snapshot.issueId`, plus any issue whose `execution_run_id`
/// points at this run.
async fn list_run_issues(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;

    // Fetch the run to resolve company_id + context issueId.
    let run = sqlx::query(&format!("{} WHERE id = $1", RUN_SELECT))
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let run = match run {
        Some(r) => r,
        None => return Err(HeartbeatRunError::NotFound(run_id)),
    };
    let company_id: Uuid = run.get("company_id");
    let context: Option<Value> = run.try_get("context_snapshot").unwrap_or(None);
    let context_issue_id = context
        .as_ref()
        .and_then(|c| c.get("issueId"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    // Issues whose execution_run_id = run_id.
    let rows = sqlx::query(
        r#"SELECT id, company_id, identifier, title, status::text, parent_id,
                  assignee_agent_id, assignee_user_id, execution_run_id, created_at, updated_at
             FROM issues
            WHERE company_id = $1
              AND (execution_run_id = $2 OR id = $3)
            ORDER BY created_at DESC"#,
    )
    .bind(company_id)
    .bind(run_id)
    .bind(context_issue_id)
    .fetch_all(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let issues: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "companyId": r.get::<Uuid, _>("company_id"),
                "identifier": r.get::<Option<String>, _>("identifier"),
                "title": r.get::<String, _>("title"),
                "status": r.get::<String, _>("status"),
                "parentId": r.get::<Option<Uuid>, _>("parent_id"),
                "assigneeAgentId": r.get::<Option<Uuid>, _>("assignee_agent_id"),
                "assigneeUserId": r.get::<Option<String>, _>("assignee_user_id"),
                "executionRunId": r.get::<Option<Uuid>, _>("execution_run_id"),
                "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect();

    Ok(Json(Value::Array(issues)))
}

/// X8: GET /heartbeat-runs/:run_id/watchdog-decisions
async fn list_watchdog_decisions(
    State(_state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    // No dedicated watchdog-decisions table yet.
    Ok(Json(json!({
        "runId": run_id,
        "decisions": [],
    })))
}

/// X9: POST /heartbeat-runs/:run_id/watchdog-decisions
async fn submit_watchdog_decision(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    Json(body): Json<WatchdogDecisionInput>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    // Verify the run exists.
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
    if exists.is_none() {
        return Err(HeartbeatRunError::NotFound(run_id));
    }

    if !matches!(
        body.decision.as_str(),
        "snooze" | "continue" | "dismissed_false_positive"
    ) {
        return Err(HeartbeatRunError::BadRequest(
            "Unsupported watchdog decision".to_string(),
        ));
    }

    let reason = body.reason.as_deref().map(|s| s.chars().take(4000).collect::<String>());

    Ok(Json(json!({
        "runId": run_id,
        "decision": body.decision,
        "evaluationIssueId": body.evaluation_issue_id,
        "reason": reason,
        "snoozedUntil": body.snoozed_until,
        "createdAt": chrono::Utc::now(),
    })))
}

/// X10: GET /heartbeat-runs/:run_id/workspace-operations
async fn list_run_workspace_operations(
    State(_state): State<AppState>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    Ok(Json(json!({
        "runId": run_id,
        "operations": [],
    })))
}

/// X11: GET /workspace-operations/:operation_id/log
async fn get_workspace_operation_log(
    State(_state): State<AppState>,
    Path(operation_id): Path<Uuid>,
    Query(q): Query<RunLogQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let offset = q.offset.unwrap_or(0).max(0);
    let limit_bytes = q.limit_bytes.unwrap_or(256 * 1024).clamp(1, 16 * 1024 * 1024);
    Ok(Json(json!({
        "operationId": operation_id,
        "offset": offset,
        "limitBytes": limit_bytes,
        "bytes": "",
        "eof": true,
    })))
}

/// X14: GET /issues/:id/runs
///
/// Returns the run history for an issue: runs whose `context_snapshot.issueId`
/// references the issue, plus the run pointed to by the issue's
/// `execution_run_id`.
async fn list_issue_runs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    // Keep one projection for both discovery paths.  The previous UNION used
    // `hr.*` for the second branch, which made the enum `status` disagree with
    // the `status::text` projection in RUN_SELECT and caused PostgreSQL to
    // return 500 for issues with execution runs.
    let rows = sqlx::query(&format!(
        "{} hr WHERE EXISTS (SELECT 1 FROM issues i WHERE i.id = $1 AND i.company_id = hr.company_id) \
          AND (hr.context_snapshot->>'issueId' = $1::text \
            OR EXISTS (SELECT 1 FROM activity_logs al WHERE al.company_id = hr.company_id \
                      AND al.resource_type = 'issue' AND al.resource_id = $1 \
                      AND al.run_id = hr.id) \
            OR EXISTS (SELECT 1 FROM issues i WHERE i.id = $1 AND i.execution_run_id = hr.id)) \
          ORDER BY hr.created_at DESC",
        RUN_SELECT
    ))
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let runs: Vec<Value> = rows.iter().map(run_to_json).collect();
    Ok(Json(Value::Array(runs)))
}

#[derive(Debug)]
pub enum HeartbeatRunError {
    NotFound(Uuid),
    Database(String),
    BadRequest(String),
}

impl IntoResponse for HeartbeatRunError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            HeartbeatRunError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Heartbeat run not found: {}", id),
            ),
            HeartbeatRunError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            HeartbeatRunError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the router must not panic. This catches intra-module route
    /// overlaps (axum panics at construction on duplicate paths).
    #[test]
    fn heartbeat_run_router_constructs() {
        let _ = heartbeat_run_routes();
    }
}
