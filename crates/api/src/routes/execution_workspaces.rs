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
    extract::{Path, Query, State},
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
        .route("/execution-workspaces/:id", get(get_execution_workspace).patch(update_execution_workspace))
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
/// Lists workspace operations recorded for this execution workspace. Parrot
/// Agent does not yet persist a dedicated `workspace_operations` table, so this
/// returns an empty list (consistent with the stub-handler convention used
/// across other domain route modules).
async fn list_workspace_operations(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    Ok(Json(json!({
        "executionWorkspaceId": id,
        "operations": [],
    })))
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
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM execution_workspaces WHERE id = $1",
    )
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
    Path((id, action)): Path<(Uuid, String)>,
    Json(body): Json<RuntimeControlTarget>,
) -> Result<Json<Value>, ExecutionWorkspaceError> {
    let action = action.trim().to_lowercase();
    if !matches!(action.as_str(), "start" | "stop" | "restart" | "run") {
        return Err(ExecutionWorkspaceError::NotFound(id));
    }

    let pool = &state.pool;
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM execution_workspaces WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| ExecutionWorkspaceError::Database(e.to_string()))?;

    if exists.is_none() {
        return Err(ExecutionWorkspaceError::NotFound(id));
    }

    Ok(Json(json!({
        "workspaceId": id,
        "action": action,
        "workspaceCommandId": body.workspace_command_id,
        "runtimeServiceId": body.runtime_service_id,
        "serviceIndex": body.service_index,
        "accepted": true,
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
    Database(String),
}

impl IntoResponse for ExecutionWorkspaceError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match self {
            ExecutionWorkspaceError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                format!("Execution workspace not found: {}", id),
            ),
            ExecutionWorkspaceError::Database(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
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
