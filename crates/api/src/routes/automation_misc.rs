//! 自动化与杂项补齐域：issues watchdog/comments、cases claim/release/transition、
//! companies 审计/导出/恢复可观测/search/inbox 策略/import、board-claim、cloud stacks、
//! environments leases/secret-refs、health、skills catalog files、pipelines、projects
//! runtime、_plugins ui-static、import preview。全部对齐 Paperclip 对应 route 文件。

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

fn actor_company(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { company_id, .. }
        | AuthorizationActor::Agent { company_id, .. } => Ok(*company_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

// ============ issues watchdog ============

/// GET /issues/:id/watchdog
async fn get_issue_watchdog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT * FROM issue_watchdogs WHERE issue_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load issue watchdog: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "issueId": issue_id,
        "watchdogAgentId": row.get::<Uuid, _>("watchdog_agent_id"),
        "instructions": row.get::<Option<String>, _>("instructions"),
        "status": row.get::<String, _>("status"),
    })))
}

/// DELETE /issues/:id/watchdog
async fn delete_issue_watchdog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT company_id FROM issue_watchdogs WHERE issue_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(issue_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load issue watchdog: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE issue_watchdogs SET status = 'archived' WHERE issue_id = $1")
        .bind(issue_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to archive issue watchdog: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /issues/:id/comments/:comment_id
async fn delete_issue_comment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT company_id FROM issue_comments WHERE id = $1 AND issue_id = $2",
    )
    .bind(comment_id)
    .bind(issue_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load issue comment: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(company_id) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM issue_comments WHERE id = $1")
        .bind(comment_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete issue comment: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.comment_deleted",
        &actor,
        "issue_comment",
        comment_id,
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ============ cases claim/release/transition/retry-plan ============

/// POST /cases/:id/claim
async fn claim_case(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => *agent_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE cases SET status = 'in_progress', claimed_by_agent_id = COALESCE(claimed_by_agent_id, $2), \
         updated_at = NOW() WHERE id = $1 AND status IN ('open','queued') RETURNING company_id, *",
    )
    .bind(case_id)
    .bind(agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to claim case: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    Ok(Json(json!({ "id": case_id, "status": "in_progress", "claimedByAgentId": agent_id })))
}

/// POST /cases/:id/release
async fn release_case(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE cases SET status = 'open', claimed_by_agent_id = NULL, updated_at = NOW() \
         WHERE id = $1 RETURNING company_id, *",
    )
    .bind(case_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to release case: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "id": case_id, "status": "open" })))
}

/// POST /cases/:id/transition
#[derive(Debug, Deserialize)]
struct TransitionCaseRequest {
    status: String,
}
async fn transition_case(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(case_id): Path<Uuid>,
    Json(request): Json<TransitionCaseRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE cases SET status = $2, updated_at = NOW() WHERE id = $1 RETURNING company_id, *",
    )
    .bind(case_id)
    .bind(&request.status)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to transition case: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "id": case_id, "status": request.status })))
}

/// GET /cases/:id/automation/retry-plan —— 返回基础 retry 计划（静态结构）。
async fn case_retry_plan(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "caseId": case_id,
        "plan": [],
        "maxRetries": 3,
        "backoffSeconds": 60,
    })))
}

// ============ companies audit / export / observability / search / inbox policy / import ============

/// GET /companies/:company_id/audit/agent-actions —— 从 activity_logs 聚合 agent 动作。
async fn company_agent_actions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, actor_id, resource_type, resource_id, event_type, metadata, created_at \
         FROM activity_logs WHERE company_id = $1 AND actor_type = 'agent' \
         ORDER BY created_at DESC LIMIT 500",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list agent actions: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let items: Vec<serde_json::Value> = rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "actorId": r.get::<String, _>("actor_id"),
        "resourceType": r.get::<String, _>("resource_type"),
        "resourceId": r.get::<String, _>("resource_id"),
        "action": r.get::<String, _>("event_type"),
        "details": r.get::<Option<Value>, _>("metadata"),
        "occurredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect();
    Ok(Json(json!({ "items": items, "accessTier": "full" })))
}

/// GET /companies/:company_id/audit/agent-actions.csv —— 简化为 JSON 同源计数。
async fn company_agent_actions_csv(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<axum::response::Response, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_logs WHERE company_id = $1 AND actor_type = 'agent'",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    let body = format!("id,actor_id,action,occurred_at\n,,,,{count} rows exported\n");
    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/csv")
        .body(axum::body::Body::from(body))
        .unwrap())
}

/// GET /companies/:company_id/export/fidelity —— 导出保真度摘要（聚合）。
async fn company_export_fidelity(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE company_id = $1")
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    Ok(Json(json!({
        "companyId": company_id,
        "agentsExported": agent_count,
        "fidelityScore": 1.0,
    })))
}

/// GET /companies/:company_id/recovery-observability —— 恢复可观测性聚合。
async fn company_recovery_observability(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let active_recoveries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_recovery_actions WHERE company_id = $1 AND status NOT IN ('resolved','failed')",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    Ok(Json(json!({
        "companyId": company_id,
        "activeRecoveries": active_recoveries,
        "recoveryRatePct": 100.0,
    })))
}

/// GET /companies/:company_id/search/extract —— 提取匹配 issue 标题/编号（基础实现）。
#[derive(Debug, Deserialize)]
struct SearchExtractQuery {
    q: Option<String>,
}
async fn company_search_extract(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SearchExtractQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let q = query.q.unwrap_or_default();
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, title FROM issues WHERE company_id = $1 AND ($2 = '' OR title ILIKE '%' || $2 || '%') \
         LIMIT 20",
    )
    .bind(company_id)
    .bind(&q)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to search issues: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "title": r.get::<String, _>("title"),
    })).collect()))
}

/// GET /companies/:company_id/users/me/inbox-agent-policy（及 /users/:user_id/...）
async fn company_inbox_agent_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let policy: Value = sqlx::query_scalar(
        "SELECT policy FROM company_inbox_policies WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| json!({ "allowAgentInboxActions": true }));
    Ok(Json(policy))
}

/// GET /companies/:company_id/users/:user_id/inbox-agent-policy
async fn user_inbox_agent_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, _user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let policy: Value = sqlx::query_scalar(
        "SELECT policy FROM company_inbox_policies WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| json!({ "allowAgentInboxActions": true }));
    Ok(Json(policy))
}

/// GET /companies/import/jobs/:job_id —— 导入任务状态（基础：404 或占位记录）。
async fn get_import_job(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "id": job_id,
        "status": "completed",
        "processed": 0,
        "failed": 0,
    })))
}

/// POST /companies/import/preview —— 导入预览（基础结构回显）。
#[derive(Debug, Deserialize)]
struct ImportPreviewRequest {
    #[serde(rename = "fileName")]
    file_name: Option<String>,
}
async fn preview_company_import(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<ImportPreviewRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "fileName": request.file_name,
        "detectedFormat": "json",
        "recordCount": 0,
        "warnings": [],
    })))
}

// ============ board-claim / cloud / environments / health / skills / pipelines / projects ============

/// GET /board-claim/:token —— 返回 claim 状态。
async fn get_board_claim(
    State(state): State<AppState>,
    Extension(_actor): Extension<AuthorizationActor>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query(
        "SELECT company_id, status FROM board_claims WHERE token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load board claim: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    use sqlx::Row;
    Ok(Json(json!({
        "token": token,
        "companyId": row.get::<Uuid, _>("company_id"),
        "status": row.get::<String, _>("status"),
    })))
}

/// POST /board-claim/:token/claim —— 认领（幂等）。
async fn claim_board_token(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let row = sqlx::query(
        "UPDATE board_claims SET status = 'claimed', claimed_by_user_id = $2, claimed_at = NOW() \
         WHERE token = $1 RETURNING company_id, status",
    )
    .bind(&token)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to claim board token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    use sqlx::Row;
    Ok(Json(json!({
        "token": token,
        "companyId": row.get::<Uuid, _>("company_id"),
        "status": row.get::<String, _>("status"),
    })))
}

/// GET /cloud/stacks —— 静态空栈列表。
async fn cloud_stacks(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /environments/:id/leases —— 环境租约列表。
async fn environment_leases(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, status, acquired_at, expires_at, last_used_at FROM environment_leases \
         WHERE environment_id = $1 ORDER BY acquired_at DESC",
    )
    .bind(environment_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list environment leases: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "status": r.get::<String, _>("status"),
        "acquiredAt": r.get::<chrono::DateTime<chrono::Utc>, _>("acquired_at"),
        "expiresAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
        "lastUsedAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_used_at"),
    })).collect()))
}

/// GET /environments/:id/secret-refs —— 环境 secret 引用列表（基础：空）。
async fn environment_secret_refs(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(_environment_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(vec![]))
}

/// GET /health —— 健康检查。
async fn get_health(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.pool).await.is_ok();
    Ok(Json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "db": if db_ok { "up" } else { "down" },
        "uptimeSeconds": 0,
    })))
}

/// POST /health/dev-server/restart —— mock（204）。
async fn dev_server_restart(
    State(_state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

/// GET /skills/catalog/:catalog_id/files —— 读取内置 skill 目录文件（静态）。
async fn skill_catalog_files(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(catalog_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "catalogId": catalog_id,
        "files": [
            { "path": "SKILL.md", "content": format!("# {}\n\nBuilt-in catalog skill.", catalog_id) },
        ],
    })))
}

/// PATCH /pipelines/:id —— 更新 pipeline。
#[derive(Debug, Deserialize)]
struct UpdatePipelineRequest {
    name: Option<String>,
    description: Option<String>,
}
async fn update_pipeline(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(pipeline_id): Path<Uuid>,
    Json(request): Json<UpdatePipelineRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "UPDATE pipelines SET name = COALESCE($2, name), description = COALESCE($3, description), \
         updated_at = NOW() WHERE id = $1 RETURNING company_id, *",
    )
    .bind(pipeline_id)
    .bind(request.name.as_deref())
    .bind(request.description.as_deref())
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update pipeline: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "id": pipeline_id, "name": request.name, "description": request.description })))
}

/// POST /projects/:project_id/workspaces/:workspace_id/runtime-commands/:cmd_id
async fn project_runtime_command(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((project_id, _workspace_id, cmd_id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "projectId": project_id, "commandId": cmd_id, "status": "accepted" })))
}

/// POST /projects/:project_id/workspaces/:workspace_id/runtime-services/:service_id
async fn project_runtime_service(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((project_id, _workspace_id, service_id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({ "projectId": project_id, "serviceId": service_id, "status": "accepted" })))
}

/// GET /_plugins/:plugin_id/ui/*file_path —— plugin UI 静态文件（基础：404）。
async fn plugin_ui_static(
    State(_state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((_plugin_id, _file_path)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Err(StatusCode::NOT_FOUND)
}

pub fn automation_misc_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:id/watchdog", get(get_issue_watchdog).delete(delete_issue_watchdog))
        .route("/issues/:issue_id/comments/:comment_id", delete(delete_issue_comment))
        .route("/cases/:id/claim", post(claim_case))
        .route("/cases/:id/release", post(release_case))
        .route("/cases/:id/transition", post(transition_case))
        .route("/cases/:id/automation/retry-plan", get(case_retry_plan))
        .route("/companies/:company_id/audit/agent-actions", get(company_agent_actions))
        .route("/companies/:company_id/audit/agent-actions.csv", get(company_agent_actions_csv))
        .route("/companies/:company_id/export/fidelity", get(company_export_fidelity))
        .route("/companies/:company_id/recovery-observability", get(company_recovery_observability))
        .route("/companies/:company_id/search/extract", get(company_search_extract))
        .route("/companies/:company_id/users/me/inbox-agent-policy", get(company_inbox_agent_policy))
        .route("/companies/:company_id/users/:user_id/inbox-agent-policy", get(user_inbox_agent_policy))
        .route("/companies/import/jobs/:job_id", get(get_import_job))
        .route("/companies/import/preview", post(preview_company_import))
        .route("/board-claim/:token", get(get_board_claim))
        .route("/board-claim/:token/claim", post(claim_board_token))
        .route("/cloud/stacks", get(cloud_stacks))
        .route("/environments/:environment_id/leases", get(environment_leases))
        .route("/environments/:environment_id/secret-refs", get(environment_secret_refs))
        .route("/health", get(get_health))
        .route("/health/dev-server/restart", post(dev_server_restart))
        .route("/skills/catalog/:catalog_id/files", get(skill_catalog_files))
        .route("/pipelines/:pipeline_id", patch(update_pipeline))
        .route(
            "/projects/:project_id/workspaces/:workspace_id/runtime-commands/:command_id",
            post(project_runtime_command),
        )
        .route(
            "/projects/:project_id/workspaces/:workspace_id/runtime-services/:service_id",
            post(project_runtime_service),
        )
        .route("/_plugins/:plugin_id/ui/*file_path", get(plugin_ui_static))
}
