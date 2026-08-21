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
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;
use sqlx::Row;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

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
fn deserialize_nullable_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    match value.as_deref().map(str::trim) {
        None | Some("") | Some("null") => Ok(None),
        Some(value) => value.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RunLogQuery {
    #[serde(default, deserialize_with = "deserialize_nullable_i64")]
    pub offset: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_nullable_i64")]
    pub limit_bytes: Option<i64>,
}

/// Run events query (Paperclip).
#[derive(Debug, Default, Deserialize)]
pub struct RunEventsQuery {
    pub after_seq: Option<i64>,
    pub limit: Option<i64>,
}

async fn authorize_run_access(
    state: &AppState,
    actor: &AuthorizationActor,
    run_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, HeartbeatRunError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?
        .ok_or(HeartbeatRunError::NotFound(run_id))?;
    require_company_access(actor, company_id, mode)
        .map_err(|_| HeartbeatRunError::NotFound(run_id))?;
    Ok(company_id)
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
    let output: Option<String> = r.try_get("output").unwrap_or(None);
    let result_json: Option<Value> = r.try_get("result_json").unwrap_or(None);
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "agentId": r.get::<Uuid, _>("agent_id"),
        "agentName": r.try_get::<Option<String>, _>("agent_name").unwrap_or(None),
        "adapterType": r.try_get::<Option<String>, _>("adapter_type").unwrap_or(None),
        "invocationSource": r.get::<String, _>("invocation_source"),
        "triggerDetail": Value::Null,
        "status": r.get::<String, _>("status"),
        "responsibleUserId": r.get::<Option<String>, _>("responsible_user_id"),
        "startedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
        "finishedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
        "error": r.get::<Option<String>, _>("error"),
        "exitCode": r.get::<Option<i32>, _>("exit_code"),
        "stdoutExcerpt": output,
        "stderrExcerpt": Value::Null,
        "resultJson": result_json,
        "scheduledRetryAt": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("scheduled_retry_at").unwrap_or(None),
        "scheduledRetryAttempt": r.try_get::<Option<i32>, _>("scheduled_retry_attempt").unwrap_or(None),
        "scheduledRetryReason": r.try_get::<Option<String>, _>("scheduled_retry_reason").unwrap_or(None),
        "usageJson": Value::Null,
        "errorCode": Value::Null,
        "logStore": Value::Null,
        "logRef": Value::Null,
        "logBytes": Value::Null,
        "contextSnapshot": context_snapshot.clone(),
        "issueId": context_snapshot.as_ref().and_then(|c| c.get("issueId")).cloned(),
        "taskId": context_snapshot.as_ref().and_then(|c| c.get("taskId")).cloned(),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

const RUN_SELECT: &str = r#"SELECT id, company_id, agent_id, invocation_source, status::text,
       responsible_user_id, started_at, finished_at, error, exit_code,
       context_snapshot, output, result_json, scheduled_retry_at, scheduled_retry_attempt,
       scheduled_retry_reason, created_at, updated_at,
       (SELECT name FROM agents WHERE agents.id = agent_id) AS agent_name,
       (SELECT adapter_type FROM agents WHERE agents.id = agent_id) AS adapter_type
  FROM heartbeat_runs"#;

/// X1: GET /companies/:company_id/heartbeat-runs
async fn list_company_heartbeat_runs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<HeartbeatRunListQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::Forbidden("Heartbeat run access denied".to_string()))?;
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<LiveRunsQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::Forbidden("Heartbeat run access denied".to_string()))?;
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    authorize_run_access(&state, &actor, run_id, AccessMode::Read).await?;
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    authorize_run_access(&state, &actor, run_id, AccessMode::Write).await?;
    let pool = &state.pool;
    if let Some(row) = sqlx::query("SELECT company_id, agent_id, context_snapshot FROM heartbeat_runs WHERE id = $1")
        .bind(run_id).fetch_optional(pool).await.map_err(|e| HeartbeatRunError::Database(e.to_string()))? {
        let company_id: Uuid = row.try_get("company_id").map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
        let agent_id: Uuid = row.try_get("agent_id").map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
        let context: Option<Value> = row.try_get("context_snapshot").unwrap_or(None);
        if let Some(issue_id) = context.and_then(|v| v.get("issueId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok())) {
            state.heartbeat_service.cancel_run(agent_id, issue_id, company_id, "cancelled by API").await
                .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
        }
    }
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

/// X5: GET /heartbeat-runs/:run_id/events.  Tool calls are the durable
/// execution events available in Parrot's schema, so expose them in the same
/// cursor-shaped projection consumed by Paperclip's run detail page.
async fn list_run_events(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
    Query(q): Query<RunEventsQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    authorize_run_access(&state, &actor, run_id, AccessMode::Read).await?;
    let after_seq = q.after_seq.unwrap_or(0);
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let rows = sqlx::query(
        "SELECT id, event_type, actor_type, actor_id, tool_name, decision, outcome, metadata, created_at
         FROM tool_call_events WHERE run_id = $1 ORDER BY created_at ASC, id ASC OFFSET $2 LIMIT $3")
        .bind(run_id).bind(after_seq).bind(limit as i64).fetch_all(&state.pool).await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
    let events: Vec<Value> = rows.into_iter().enumerate().map(|(i, r)| json!({
        "seq": after_seq + i as i64 + 1,
        "id": r.get::<Uuid, _>("id"),
        "type": r.get::<String, _>("event_type"),
        "eventType": r.get::<String, _>("event_type"),
        "actorType": r.get::<String, _>("actor_type"),
        "actorId": r.get::<Option<String>, _>("actor_id"),
        "toolName": r.get::<Option<String>, _>("tool_name"),
        "decision": r.get::<Option<String>, _>("decision"),
        "outcome": r.get::<String, _>("outcome"),
        "metadata": r.get::<Option<Value>, _>("metadata"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
    })).collect();
    Ok(Json(Value::Array(events)))
}

/// X6: GET /heartbeat-runs/:run_id/log
async fn get_run_log(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
    Query(q): Query<RunLogQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    authorize_run_access(&state, &actor, run_id, AccessMode::Read).await?;
    let offset = q.offset.unwrap_or(0).max(0) as usize;
    let limit = q.limit_bytes.unwrap_or(256 * 1024).clamp(1, 16 * 1024 * 1024) as usize;
    // A run can exist before its adapter has emitted any output.  The
    // database column is nullable in that state; treat it as an empty log
    // instead of letting sqlx decode NULL into String and return a 500.
    let output: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(output, '') FROM heartbeat_runs WHERE id = $1",
    )
        .bind(run_id).fetch_optional(&state.pool).await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
    let output = output.ok_or(HeartbeatRunError::NotFound(run_id))?;
    let bytes = output.as_bytes();
    let start = offset.min(bytes.len());
    let end = (start + limit).min(bytes.len());
    let content = String::from_utf8_lossy(&bytes[start..end]).to_string();
    Ok(Json(json!({
        "runId": run_id,
        "store": "database",
        "logRef": "heartbeat_runs.output",
        "content": content,
        "nextOffset": if end < bytes.len() { json!(end) } else { Value::Null },
    })))
}

/// X7: GET /heartbeat-runs/:run_id/issues
///
/// Returns the issues associated with this run — the issue referenced by the
/// run's `context_snapshot.issueId`, plus any issue whose `execution_run_id`
/// points at this run.
async fn list_run_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
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
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::NotFound(run_id))?;
    let context: Option<Value> = run.try_get("context_snapshot").unwrap_or(None);
    let context_issue_id = context
        .as_ref()
        .and_then(|c| c.get("issueId"))
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok());

    // Issues whose execution_run_id = run_id.
    let rows = sqlx::query(
        r#"SELECT id, company_id, identifier, title, status::text, priority::text, parent_id,
                  assignee_agent_id, assignee_user_id, execution_run_id, created_at, updated_at
             FROM issues
            WHERE company_id = $1
              AND (
                    execution_run_id = $2
                 OR id = $3
                 OR EXISTS (
                      SELECT 1 FROM activity_logs al
                       WHERE al.company_id = issues.company_id
                         AND al.run_id = $2
                         AND al.resource_type = 'issue'
                         AND al.resource_id = issues.id
                    )
              )
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
                "issueId": r.get::<Uuid, _>("id"),
                "id": r.get::<Uuid, _>("id"),
                "companyId": r.get::<Uuid, _>("company_id"),
                "identifier": r.get::<Option<String>, _>("identifier"),
                "title": r.get::<String, _>("title"),
                "status": r.get::<String, _>("status"),
                "priority": r.get::<String, _>("priority"),
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

/// Serialize a `heartbeat_run_watchdog_decisions` row to the Paperclip-shaped JSON.
fn watchdog_decision_to_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "runId": r.get::<Uuid, _>("run_id"),
        "decision": r.get::<String, _>("decision"),
        "evaluationIssueId": r.get::<Option<Uuid>, _>("evaluation_issue_id"),
        "reason": r.get::<Option<String>, _>("reason"),
        "snoozedUntil": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("snoozed_until"),
        "createdByType": r.get::<Option<String>, _>("created_by_type"),
        "createdById": r.get::<Option<Uuid>, _>("created_by_id"),
        "createdByRunId": r.get::<Option<Uuid>, _>("created_by_run_id"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })
}

/// X8: GET /heartbeat-runs/:run_id/watchdog-decisions
async fn list_watchdog_decisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?
        .ok_or(HeartbeatRunError::NotFound(run_id))?;
    // Paperclip's getAccessibleResource returns 404 for inaccessible runs.
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::NotFound(run_id))?;

    let rows = sqlx::query(
        "SELECT id, run_id, decision, evaluation_issue_id, reason, snoozed_until, \
         created_by_type, created_by_id, created_by_run_id, created_at \
         FROM heartbeat_run_watchdog_decisions WHERE run_id = $1 ORDER BY created_at ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let decisions: Vec<Value> = rows.iter().map(watchdog_decision_to_json).collect();
    Ok(Json(json!({ "runId": run_id, "decisions": decisions })))
}

/// X9: POST /heartbeat-runs/:run_id/watchdog-decisions
async fn submit_watchdog_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
    Json(body): Json<WatchdogDecisionInput>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let pool = &state.pool;
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?
        .ok_or(HeartbeatRunError::NotFound(run_id))?;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| HeartbeatRunError::NotFound(run_id))?;

    if !matches!(
        body.decision.as_str(),
        "snooze" | "continue" | "dismissed_false_positive"
    ) {
        return Err(HeartbeatRunError::BadRequest(
            "Unsupported watchdog decision".to_string(),
        ));
    }

    // snooze requires a future ISO datetime.
    let snoozed_until: Option<chrono::DateTime<chrono::Utc>> = if body.decision == "snooze" {
        let raw = body.snoozed_until.as_deref().ok_or_else(|| {
            HeartbeatRunError::BadRequest("snoozedUntil is required for snooze".to_string())
        })?;
        let dt = chrono::DateTime::parse_from_rfc3339(raw).map_err(|_| {
            HeartbeatRunError::BadRequest("snoozedUntil must be a valid ISO datetime".to_string())
        })?;
        if dt <= chrono::Utc::now() {
            return Err(HeartbeatRunError::BadRequest(
                "snoozedUntil must be a future ISO datetime".to_string(),
            ));
        }
        Some(dt.with_timezone(&chrono::Utc))
    } else {
        None
    };

    let reason = body
        .reason
        .as_deref()
        .map(|s| s.chars().take(4000).collect::<String>());

    let decision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO heartbeat_run_watchdog_decisions \
         (run_id, company_id, decision, evaluation_issue_id, reason, snoozed_until, \
          created_by_type, created_by_id, created_by_run_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(&body.decision)
    .bind(body.evaluation_issue_id)
    .bind(&reason)
    .bind(snoozed_until)
    .bind(actor.actor_type())
    .bind(actor.principal_id())
    .bind(None::<Uuid>)
    .fetch_one(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    let row = sqlx::query(
        "SELECT id, run_id, decision, evaluation_issue_id, reason, snoozed_until, \
         created_by_type, created_by_id, created_by_run_id, created_at \
         FROM heartbeat_run_watchdog_decisions WHERE id = $1",
    )
    .bind(decision_id)
    .fetch_one(pool)
    .await
    .map_err(|e| HeartbeatRunError::Database(e.to_string()))?;

    Ok(Json(watchdog_decision_to_json(&row)))
}

/// X10: GET /heartbeat-runs/:run_id/workspace-operations
async fn list_run_workspace_operations(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM heartbeat_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?
        .ok_or(HeartbeatRunError::NotFound(run_id))?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::Forbidden("Heartbeat run access denied".to_string()))?;
    let rows = sqlx::query("SELECT id, company_id, execution_workspace_id, heartbeat_run_id, issue_id, phase, command, cwd, status, exit_code, log_store, log_ref, log_bytes, log_sha256, log_compressed, stdout_excerpt, stderr_excerpt, metadata, started_at, finished_at, created_at, updated_at FROM workspace_operations WHERE heartbeat_run_id = $1 ORDER BY started_at ASC")
        .bind(run_id).fetch_all(&state.pool).await.map_err(|e| HeartbeatRunError::Database(e.to_string()))?;
    let operations: Vec<Value> = rows.into_iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"), "companyId": r.get::<Uuid, _>("company_id"),
        "executionWorkspaceId": r.get::<Option<Uuid>, _>("execution_workspace_id"), "heartbeatRunId": r.get::<Option<Uuid>, _>("heartbeat_run_id"),
        "issueId": r.get::<Option<Uuid>, _>("issue_id"), "phase": r.get::<String, _>("phase"), "command": r.get::<Option<String>, _>("command"),
        "cwd": r.get::<Option<String>, _>("cwd"), "status": r.get::<String, _>("status"), "exitCode": r.get::<Option<i32>, _>("exit_code"),
        "logStore": r.get::<Option<String>, _>("log_store"), "logRef": r.get::<Option<String>, _>("log_ref"), "logBytes": r.get::<Option<i64>, _>("log_bytes"),
        "logSha256": r.get::<Option<String>, _>("log_sha256"), "logCompressed": r.get::<bool, _>("log_compressed"),
        "stdoutExcerpt": r.get::<Option<String>, _>("stdout_excerpt"), "stderrExcerpt": r.get::<Option<String>, _>("stderr_excerpt"),
        "metadata": r.get::<Option<Value>, _>("metadata"), "startedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
        "finishedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"), "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"), "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    })).collect();
    Ok(Json(Value::Array(operations)))
}

/// X11: GET /workspace-operations/:operation_id/log
async fn get_workspace_operation_log(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(operation_id): Path<Uuid>,
    Query(q): Query<RunLogQuery>,
) -> Result<Json<Value>, HeartbeatRunError> {
    let offset = q.offset.unwrap_or(0).max(0);
    let limit_bytes = q.limit_bytes.unwrap_or(256 * 1024).clamp(1, 16 * 1024 * 1024);
    let row = sqlx::query("SELECT company_id, log_store, log_ref, stdout_excerpt, stderr_excerpt FROM workspace_operations WHERE id = $1")
        .bind(operation_id).fetch_optional(&state.pool).await
        .map_err(|e| HeartbeatRunError::Database(e.to_string()))?
        .ok_or(HeartbeatRunError::NotFound(operation_id))?;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| HeartbeatRunError::Forbidden("Workspace operation access denied".to_string()))?;
    let store = row.get::<Option<String>, _>("log_store").unwrap_or_else(|| "database".into());
    let reference = row.get::<Option<String>, _>("log_ref");
    let content = match (&store[..], reference.as_deref()) {
        ("local_file", Some(path)) => tokio::fs::read(path).await.ok().map(|b| {
            let start = (offset as usize).min(b.len()); let end = (start + limit_bytes as usize).min(b.len());
            String::from_utf8_lossy(&b[start..end]).to_string()
        }).unwrap_or_default(),
        _ => format!("{}{}", row.get::<Option<String>, _>("stdout_excerpt").unwrap_or_default(), row.get::<Option<String>, _>("stderr_excerpt").unwrap_or_default()),
    };
    Ok(Json(json!({
        "operationId": operation_id,
        "store": store,
        "logRef": reference,
        "content": content,
        "nextOffset": Value::Null,
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

    // Paperclip's issue-run client uses `runId` (while the general heartbeat
    // run contract uses `id`). Keep both names here so the UI can construct
    // stable historical transcript message ids.
    let runs: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut run = run_to_json(row);
            if let Some(object) = run.as_object_mut() {
                let id = object.get("id").cloned().unwrap_or(Value::Null);
                object.insert("runId".to_string(), id);
            }
            run
        })
        .collect();
    Ok(Json(Value::Array(runs)))
}

#[derive(Debug)]
pub enum HeartbeatRunError {
    NotFound(Uuid),
    Database(String),
    BadRequest(String),
    Forbidden(String),
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
            HeartbeatRunError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
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
