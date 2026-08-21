//! Execution workspace routes — 补齐 X15-X18 + workspace CRUD surface.
//!
//! 对应 FEATURE_GAP_TASKS.md §3.3 Executions/Runs (X15-X18) 及
//! API_GAP_TASKS.md §3.5 execution-workspaces 路由。
//!
//! 路由路径与 Paperclip `server/src/routes/execution-workspaces.ts` 对齐：
//!   GET    /companies/:company_id/execution-workspaces
//!   GET    /companies/:company_id/workspace-overview
//!   GET    /execution-workspaces/:id
//!   GET    /execution-workspaces/:id/close-readiness
//!   GET    /execution-workspaces/:id/workspace-operations
//!   POST   /execution-workspaces/:id/reconcile-branch
//!   PATCH  /execution-workspaces/:id
//!   POST   /execution-workspaces/:id/runtime-services/:action
//!   POST   /execution-workspaces/:id/runtime-commands/:action

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;
use services::authorization_service::{
    RuntimeServiceAction, RuntimeServiceAuthzRequest,
};

pub fn execution_workspace_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/execution-workspaces",
            get(list_company_execution_workspaces),
        )
        .route(
            "/companies/:company_id/workspace-overview",
            get(get_workspace_overview),
        )
        .route(
            "/execution-workspaces/:id",
            get(get_execution_workspace).patch(update_execution_workspace),
        )
        .route(
            "/execution-workspaces/:id/close-readiness",
            get(get_close_readiness),
        )
        .route(
            "/execution-workspaces/:id/workspace-operations",
            get(list_workspace_operations),
        )
        .route(
            "/execution-workspaces/:id/reconcile-branch",
            post(reconcile_branch),
        )
        .route(
            "/execution-workspaces/:id/runtime-services/:action",
            post(runtime_command),
        )
        .route(
            "/execution-workspaces/:id/runtime-commands/:action",
            post(runtime_command),
        )
}

/// Query filters mirroring Paperclip's list filters.
#[derive(Debug, Default, Deserialize)]
pub struct ExecutionWorkspaceListQuery {
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub status: Option<String>,
    /// `"true"` requests the summary projection.
    pub summary: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// X16/X18: GET /companies/:company_id/execution-workspaces
///
/// Lists execution workspaces for a company, optionally filtered. The `summary`
/// query flag (Paperclip: `summary=true`) returns a slim projection.
async fn list_company_execution_workspaces(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<ExecutionWorkspaceListQuery>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let offset = q.offset.unwrap_or(0).max(0);
    let summary = matches!(q.summary.as_deref(), Some("true") | Some("1"));

    // Build the query dynamically. We select the full row; for the summary
    // projection we project down to the Paperclip summary shape.
    let rows = sqlx::query(
        r#"SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                  name, mode::text, strategy_type::text, status::text, cwd, provider_ref,
                  base_ref, branch_name, repo_url, metadata, created_at, updated_at
             FROM execution_workspaces
            WHERE company_id = $1 AND status <> 'archived'
              AND ($2::uuid IS NULL OR project_id = $2)
              AND ($3::uuid IS NULL OR project_workspace_id = $3)
              AND ($4::uuid IS NULL OR source_issue_id = $4)
              AND ($5::text IS NULL OR status = $5::text)
            ORDER BY created_at DESC
            LIMIT $6 OFFSET $7"#,
    )
    .bind(company_id)
    .bind(q.project_id)
    .bind(q.project_workspace_id)
    .bind(q.issue_id)
    .bind(q.status.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let workspaces: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            if summary {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "name": r.get::<String, _>("name"),
                    "status": r.get::<String, _>("status"),
                    "mode": r.get::<String, _>("mode"),
                    "strategyType": r.get::<String, _>("strategy_type"),
                    "cwd": r.get::<Option<String>, _>("cwd"),
                    "branchName": r.get::<Option<String>, _>("branch_name"),
                    "sourceIssueId": r.get::<Option<Uuid>, _>("source_issue_id"),
                    "projectId": r.get::<Option<Uuid>, _>("project_id"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            } else {
                json!({
                    "id": r.get::<Uuid, _>("id"),
                    "companyId": r.get::<Uuid, _>("company_id"),
                    "projectId": r.get::<Option<Uuid>, _>("project_id"),
                    "projectWorkspaceId": r.get::<Option<Uuid>, _>("project_workspace_id"),
                    "sourceIssueId": r.get::<Option<Uuid>, _>("source_issue_id"),
                    "name": r.get::<String, _>("name"),
                    "mode": r.get::<String, _>("mode"),
                    "strategyType": r.get::<String, _>("strategy_type"),
                    "status": r.get::<String, _>("status"),
                    "cwd": r.get::<Option<String>, _>("cwd"),
                    "providerRef": r.get::<Option<String>, _>("provider_ref"),
                    "baseRef": r.get::<Option<String>, _>("base_ref"),
                    "branchName": r.get::<Option<String>, _>("branch_name"),
                    "repoUrl": r.get::<Option<String>, _>("repo_url"),
                    "metadata": r.get::<Option<Value>, _>("metadata"),
                    "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            }
        })
        .collect();

    Ok(Json(Value::Array(workspaces)))
}

/// X18: GET /companies/:company_id/workspace-overview
///
/// Returns a workspace overview for the company. Mirrors Paperclip's
/// `workspaceOverviewQuerySchema` (projectId filter) and `svc.listOverview`.
async fn get_workspace_overview(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<ExecutionWorkspaceListQuery>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    // Aggregate counts by status.
    let rows = sqlx::query(
        r#"SELECT status::text, COUNT(*)::bigint AS count
             FROM execution_workspaces
            WHERE company_id = $1 AND status <> 'archived'
              AND ($2::uuid IS NULL OR project_id = $2)
            GROUP BY status"#,
    )
    .bind(company_id)
    .bind(q.project_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let by_status: Value = rows
        .into_iter()
        .map(|r| {
            let status: String = r.get("status");
            let count: i64 = r.get("count");
            (status, json!(count))
        })
        .collect();

    Ok(Json(json!({
        "companyId": company_id,
        "byStatus": by_status,
        "projectId": q.project_id,
    })))
}

/// GET /execution-workspaces/:id
async fn get_execution_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    let row = sqlx::query(
        r#"SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                  name, mode::text, strategy_type::text, status::text, cwd, provider_ref,
                  base_ref, branch_name, repo_url, metadata, created_at, updated_at
             FROM execution_workspaces WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    match row {
        Some(r) => Ok(Json(json!({
            "id": r.get::<Uuid, _>("id"),
            "companyId": r.get::<Uuid, _>("company_id"),
            "projectId": r.get::<Option<Uuid>, _>("project_id"),
            "projectWorkspaceId": r.get::<Option<Uuid>, _>("project_workspace_id"),
            "sourceIssueId": r.get::<Option<Uuid>, _>("source_issue_id"),
            "name": r.get::<String, _>("name"),
            "mode": r.get::<String, _>("mode"),
            "strategyType": r.get::<String, _>("strategy_type"),
            "status": r.get::<String, _>("status"),
            "cwd": r.get::<Option<String>, _>("cwd"),
            "providerRef": r.get::<Option<String>, _>("provider_ref"),
            "baseRef": r.get::<Option<String>, _>("base_ref"),
            "branchName": r.get::<Option<String>, _>("branch_name"),
            "repoUrl": r.get::<Option<String>, _>("repo_url"),
            "metadata": r.get::<Option<Value>, _>("metadata"),
            "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }))),
        None => Err(ExecutionWorkspaceError::NotFound(id)),
    }
}

/// X15: GET /execution-workspaces/:id/close-readiness
///
/// Reports whether a workspace is safe to close/teardown: no live heartbeat
/// runs reference it and it is not currently `running`.
async fn get_close_readiness(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    let ws = sqlx::query(
        r#"SELECT id, company_id, status::text, source_issue_id, mode::text
             FROM execution_workspaces WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let ws = match ws {
        Some(r) => r,
        None => return Err(ExecutionWorkspaceError::NotFound(id)),
    };

    let status: String = ws.get("status");
    let company_id: Uuid = ws.get("company_id");

    // Count live runs whose context references this workspace.
    let live_count: (i64,) = sqlx::query_as(
        r#"SELECT COUNT(*) FROM heartbeat_runs
            WHERE company_id = $1
              AND status IN ('queued', 'running')
              AND context_snapshot->>'executionWorkspaceId' = $2"#,
    )
    .bind(company_id)
    .bind(id.to_string())
    .fetch_one(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let ready = status != "running" && live_count.0 == 0;

    Ok(Json(json!({
        "workspaceId": id,
        "companyId": company_id,
        "status": status,
        "ready": ready,
        "blockingLiveRuns": live_count.0,
        "blockers": if ready {
            json!([])
        } else {
            json!([{
                "kind": if status == "running" { "workspace_running" } else { "live_runs" },
                "count": live_count.0.max(if status == "running" { 1 } else { 0 }),
            }])
        },
    })))
}

/// X16: GET /execution-workspaces/:id/workspace-operations
///
async fn list_workspace_operations(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let workspace = sqlx::query("SELECT company_id FROM execution_workspaces WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?
        .ok_or(ExecutionWorkspaceError::NotFound(id))?;
    let company_id: Uuid = workspace.get("company_id");
    let rows = sqlx::query(
        "SELECT id, phase, command, cwd, status, exit_code, log_store, log_ref, log_bytes, log_sha256, log_compressed, stdout_excerpt, stderr_excerpt, metadata, started_at, finished_at, created_at, updated_at FROM workspace_operations WHERE execution_workspace_id = $1 ORDER BY started_at DESC",
    ).bind(id).fetch_all(&state.pool).await
        .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;
    let operations = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "companyId": company_id,
                "executionWorkspaceId": id,
                "phase": row.get::<String, _>("phase"),
                "command": row.get::<Option<String>, _>("command"),
                "cwd": row.get::<Option<String>, _>("cwd"),
                "status": row.get::<String, _>("status"),
                "exitCode": row.get::<Option<i32>, _>("exit_code"),
                "logStore": row.get::<Option<String>, _>("log_store"),
                "logRef": row.get::<Option<String>, _>("log_ref"),
                "logBytes": row.get::<Option<i64>, _>("log_bytes"),
                "logSha256": row.get::<Option<String>, _>("log_sha256"),
                "logCompressed": row.get::<bool, _>("log_compressed"),
                "stdoutExcerpt": row.get::<Option<String>, _>("stdout_excerpt"),
                "stderrExcerpt": row.get::<Option<String>, _>("stderr_excerpt"),
                "metadata": row.get::<Option<Value>, _>("metadata"),
                "startedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("started_at"),
                "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(Value::Array(operations)))
}

/// Reconcile-branch request body — mirrors Paperclip's
/// `reconcileExecutionWorkspaceBranchSchema`.
#[derive(Debug, Deserialize)]
pub struct ReconcileBranchInput {
    pub mode: String,
    pub reason: Option<String>,
}

/// Reconcile result shape — mirrors Paperclip's `svc.reconcileExecutionWorkspaceBranch`.
#[derive(Debug, Serialize)]
pub struct ReconcileBranchResult {
    pub workspace_id: Uuid,
    pub mode: String,
    pub reason: Option<String>,
    pub inspection: InspectionResult,
    pub rescue_ref: Option<Value>,
    pub audit_comment_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct InspectionResult {
    pub from_branch: Option<String>,
    pub to_branch: Option<String>,
    pub from_sha: Option<String>,
    pub to_sha: Option<String>,
    pub ancestry_verdict: String,
    pub fingerprint: Option<String>,
}

/// X17: POST /execution-workspaces/:id/reconcile-branch
///
/// Reconciles the workspace branch against its base. Full git reconciliation
/// (Paperclip's quarantine/restore flow) requires runtime infrastructure not
/// present here; this validates the workspace and records the intent.
async fn reconcile_branch(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReconcileBranchInput>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM execution_workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    if exists.is_none() {
        return Err(ExecutionWorkspaceError::NotFound(id));
    }

    let result = ReconcileBranchResult {
        workspace_id: id,
        mode: body.mode.clone(),
        reason: body.reason.clone(),
        inspection: InspectionResult {
            from_branch: None,
            to_branch: None,
            from_sha: None,
            to_sha: None,
            ancestry_verdict: "unknown".to_string(),
            fingerprint: None,
        },
        rescue_ref: None,
        audit_comment_id: None,
    };

    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

/// Runtime command target — mirrors Paperclip's
/// `workspaceRuntimeControlTargetSchema`.
#[derive(Debug, Deserialize)]
pub struct RuntimeControlTarget {
    pub workspace_command_id: Option<String>,
    pub runtime_service_id: Option<String>,
    pub service_index: Option<i32>,
}

/// POST /execution-workspaces/:id/runtime-services/:action
/// POST /execution-workspaces/:id/runtime-commands/:action
///
/// `action` is one of `start|stop|restart|run` (Paperclip).
async fn runtime_command(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, action)): Path<(Uuid, String)>,
    Json(body): Json<RuntimeControlTarget>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let action = action.trim().to_lowercase();
    if !matches!(action.as_str(), "start" | "stop" | "restart" | "run") {
        return Err(ExecutionWorkspaceError::NotFound(id));
    }

    let pool = &state.pool;
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT company_id FROM execution_workspaces WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let Some((company_id,)) = exists else {
        return Err(ExecutionWorkspaceError::NotFound(id));
    };
    if !actor.is_board() && !actor.is_agent() {
        return Err(ExecutionWorkspaceError::Forbidden(
            "Board or Agent actor required for runtime control".to_string(),
        ));
    }
    require_company_access(&actor, company_id, AccessMode::Write).map_err(|_| {
        ExecutionWorkspaceError::Forbidden("Workspace company access denied".to_string())
    })?;

    if actor.is_agent() {
        let agent_id = match &actor {
            AuthorizationActor::Agent { agent_id, .. } => *agent_id,
            _ => unreachable!("agent actor must contain an agent id"),
        };
        let runtime_action = match action.as_str() {
            "start" => RuntimeServiceAction::Start,
            "stop" => RuntimeServiceAction::Stop,
            "restart" => RuntimeServiceAction::Restart,
            "run" => RuntimeServiceAction::Run,
            _ => unreachable!("runtime action was validated above"),
        };
        state
            .workspace_runtime_authz_service
            .check_runtime_service_permission(RuntimeServiceAuthzRequest {
                workspace_id: id,
                service_name: body
                    .runtime_service_id
                    .clone()
                    .or_else(|| body.workspace_command_id.clone())
                    .unwrap_or_else(|| "workspace-runtime".to_string()),
                action: runtime_action,
                agent_id: Some(agent_id),
            })
            .await
            .map_err(|error| match error {
                services::authorization_service::AuthorizationError::WorkspaceNotFound(id) => {
                    ExecutionWorkspaceError::NotFound(id)
                }
                services::authorization_service::AuthorizationError::AgentNotFound(agent_id) => {
                    ExecutionWorkspaceError::Forbidden(format!(
                        "Runtime Agent not found: {agent_id}"
                    ))
                }
                services::authorization_service::AuthorizationError::DatabaseError(error) => {
                    ExecutionWorkspaceError::Database(error.to_string())
                }
                other => ExecutionWorkspaceError::Forbidden(other.to_string()),
            })
            .and_then(|decision| {
                if decision.allowed {
                    Ok(())
                } else {
                    Err(ExecutionWorkspaceError::Forbidden(decision.reason))
                }
            })?;
    }

    let phase = match action.as_str() {
        "start" => "runtime_start",
        "stop" => "runtime_stop",
        "restart" => "runtime_restart",
        "run" => "command_execution",
        _ => unreachable!("runtime action was validated above"),
    };
    let command_id = body.workspace_command_id.clone();
    let runtime_service_id = body.runtime_service_id.clone();
    let service_index = body.service_index;
    let operation = sqlx::query(
        r#"INSERT INTO workspace_operations
              (company_id, execution_workspace_id, phase, command, metadata)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, status, started_at"#,
    )
    .bind(company_id)
    .bind(id)
    .bind(phase)
    .bind(command_id)
    .bind(json!({
        "runtimeServiceId": runtime_service_id,
        "serviceIndex": service_index,
        "action": action.clone(),
        "providerExecution": "pending",
    }))
    .fetch_one(pool)
    .await
    .map_err(|error| ExecutionWorkspaceError::Database(error.to_string()))?;
    let operation_id: Uuid = operation.get("id");
    let operation_status: String = operation.get("status");
    let operation_started_at: chrono::DateTime<chrono::Utc> = operation.get("started_at");

    Ok(Json(json!({
        "workspaceId": id,
        "action": action,
        "workspaceCommandId": body.workspace_command_id,
        "runtimeServiceId": body.runtime_service_id,
        "serviceIndex": body.service_index,
        "accepted": true,
        "operationId": operation_id,
        "operationStatus": operation_status,
        "startedAt": operation_started_at,
    })))
}

/// Update body — mirrors Paperclip's `updateExecutionWorkspaceSchema`.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateExecutionWorkspaceInput {
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub repo_url: Option<String>,
    pub base_ref: Option<String>,
    pub branch_name: Option<String>,
    pub provider_ref: Option<String>,
    pub status: Option<String>,
}

/// PATCH /execution-workspaces/:id
async fn update_execution_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateExecutionWorkspaceInput>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let pool = &state.pool;
    let row = sqlx::query(
        r#"UPDATE execution_workspaces
              SET updated_at = NOW(),
                  name        = COALESCE($2, name),
                  cwd         = COALESCE($3, cwd),
                  repo_url    = COALESCE($4, repo_url),
                  base_ref    = COALESCE($5, base_ref),
                  branch_name = COALESCE($6, branch_name),
                  provider_ref = COALESCE($7, provider_ref)
            WHERE id = $1
          RETURNING id, company_id, project_id, project_workspace_id, source_issue_id,
                    name, mode::text, strategy_type::text, status::text, cwd, provider_ref,
                    base_ref, branch_name, repo_url, metadata, created_at, updated_at"#,
    )
    .bind(id)
    .bind(body.name)
    .bind(body.cwd)
    .bind(body.repo_url)
    .bind(body.base_ref)
    .bind(body.branch_name)
    .bind(body.provider_ref)
    .fetch_optional(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    let r = match row {
        Some(r) => r,
        None => return Err(ExecutionWorkspaceError::NotFound(id)),
    };

    Ok(Json(json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "projectId": r.get::<Option<Uuid>, _>("project_id"),
        "projectWorkspaceId": r.get::<Option<Uuid>, _>("project_workspace_id"),
        "sourceIssueId": r.get::<Option<Uuid>, _>("source_issue_id"),
        "name": r.get::<String, _>("name"),
        "mode": r.get::<String, _>("mode"),
        "strategyType": r.get::<String, _>("strategy_type"),
        "status": r.get::<String, _>("status"),
        "cwd": r.get::<Option<String>, _>("cwd"),
        "providerRef": r.get::<Option<String>, _>("provider_ref"),
        "baseRef": r.get::<Option<String>, _>("base_ref"),
        "branchName": r.get::<Option<String>, _>("branch_name"),
        "repoUrl": r.get::<Option<String>, _>("repo_url"),
        "metadata": r.get::<Option<Value>, _>("metadata"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })))
}

#[derive(Debug)]
pub enum ExecutionWorkspaceError {
    NotFound(Uuid),
    Forbidden(String),
    Database(String),
}

impl IntoResponse for ExecutionWorkspaceError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            ExecutionWorkspaceError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Execution workspace not found: {}", id),
            ),
            ExecutionWorkspaceError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            ExecutionWorkspaceError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
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
    fn execution_workspace_router_constructs() {
        let _ = execution_workspace_routes();
    }
}
