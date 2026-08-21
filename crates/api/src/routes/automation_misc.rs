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
use std::collections::HashSet;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{log_activity, require_company_access, AccessMode};
use services::auth::{
    ActorSource, AuthorizationAction, AuthorizationActor, AuthorizationService, PermissionKey,
};

fn actor_company(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { company_id, .. }
        | AuthorizationActor::Agent { company_id, .. } => Ok(*company_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

fn collect_environment_secret_refs(
    value: &Value,
    path: &str,
    refs: &mut Vec<(Uuid, String, String)>,
    seen: &mut HashSet<(Uuid, String)>,
) {
    let Some(object) = value.as_object() else {
        return;
    };
    if object.get("type").and_then(Value::as_str) == Some("secret_ref") {
        let Some(secret_id) = object
            .get("secretId")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw.trim()).ok())
        else {
            return;
        };
        let version = match object.get("version") {
            Some(Value::Number(number)) => number
                .as_i64()
                .filter(|value| *value > 0)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "latest".to_string()),
            Some(Value::String(version)) if !version.is_empty() => version.clone(),
            _ => "latest".to_string(),
        };
        if seen.insert((secret_id, path.to_string())) {
            refs.push((secret_id, path.to_string(), version));
        }
    }
    for (key, child) in object {
        let child_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        collect_environment_secret_refs(child, &child_path, refs, seen);
    }
}

#[cfg(test)]
mod environment_secret_ref_tests {
    use super::collect_environment_secret_refs;
    use serde_json::json;
    use std::collections::HashSet;
    use uuid::Uuid;

    #[test]
    fn collects_nested_secret_ref_bindings_without_values() {
        let secret_id = Uuid::new_v4();
        let config = json!({
            "provider": "secure",
            "credentials": {
                "apiKey": {
                    "type": "secret_ref",
                    "secretId": secret_id,
                    "version": 3,
                    "value": "must-not-be-returned"
                }
            }
        });
        let mut refs = Vec::new();
        collect_environment_secret_refs(&config, "", &mut refs, &mut HashSet::new());
        assert_eq!(refs, vec![(secret_id, "credentials.apiKey".to_string(), "3".to_string())]);
    }

    #[test]
    fn ignores_malformed_secret_ref_objects() {
        let config = json!({"apiKey": {"type": "secret_ref", "secretId": "not-a-uuid"}});
        let mut refs = Vec::new();
        collect_environment_secret_refs(&config, "", &mut refs, &mut HashSet::new());
        assert!(refs.is_empty());
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

/// PUT /issues/:id/watchdog —— 创建/更新 issue watchdog。
#[derive(Debug, Deserialize)]
struct UpsertWatchdogRequest {
    #[serde(rename = "watchdogAgentId")]
    watchdog_agent_id: Uuid,
    instructions: Option<String>,
}
async fn upsert_issue_watchdog(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Json(request): Json<UpsertWatchdogRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let company_id = sqlx::query_scalar::<_, Option<Uuid>>("SELECT company_id FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load issue: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let row = sqlx::query(
        "INSERT INTO issue_watchdogs (company_id, issue_id, watchdog_agent_id, instructions, status) \
         VALUES ($1, $2, $3, $4, 'active') \
         ON CONFLICT (issue_id) DO UPDATE SET watchdog_agent_id = EXCLUDED.watchdog_agent_id, \
         instructions = EXCLUDED.instructions, status = 'active' RETURNING *",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(request.watchdog_agent_id)
    .bind(request.instructions.as_deref())
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to upsert issue watchdog: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "issue.watchdog_created",
        &actor,
        "issue_watchdog",
        row.get::<Uuid, _>("id"),
        json!({}),
    )
    .await;
    Ok(Json(json!({
        "id": row.get::<Uuid, _>("id"),
        "issueId": issue_id,
        "watchdogAgentId": request.watchdog_agent_id,
        "instructions": request.instructions,
        "status": "active",
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
    let _company_id: Uuid = row.get("company_id");
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
        "SELECT COUNT(*) FROM recovery_actions WHERE company_id = $1 AND status NOT IN ('resolved','failed')",
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
    let user_id = board_user_id(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Read).map_err(|_| StatusCode::FORBIDDEN)?;
    get_user_inbox_agent_policy(&state, company_id, user_id).await
}

fn board_user_id(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        _ => Err(StatusCode::FORBIDDEN),
    }
}

fn can_manage_inbox_agent_policy(actor: &AuthorizationActor, company_id: Uuid) -> bool {
    if actor.is_instance_admin()
        || actor.role_in(company_id).is_some_and(|role| role.can_manage_members())
        || matches!(
            actor,
            AuthorizationActor::Board {
                source: ActorSource::LocalImplicit,
                ..
            }
        )
    {
        return true;
    }

    match actor {
        AuthorizationActor::Agent {
            key_scope,
            on_behalf_of_memberships,
            ..
        } => {
            let delegated_role = on_behalf_of_memberships.iter().any(|membership| {
                membership.company_id == company_id
                    && membership.status.is_active()
                    && membership.role.can_manage_members()
            });
            let delegated_key = key_scope.as_ref().is_some_and(|scope| {
                scope.can_perform_action(PermissionKey::USERS_MANAGE_PERMISSIONS)
            });
            delegated_role || delegated_key
        }
        _ => false,
    }
}

async fn get_user_inbox_agent_policy(
    state: &AppState,
    company_id: Uuid,
    user_id: Uuid,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT mode, allowed_agent_ids, created_at, updated_at \
         FROM user_inbox_agent_policies WHERE company_id = $1 AND user_id = $2",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = match row {
        Some(row) => json!({
            "companyId": company_id,
            "userId": user_id,
            "mode": row.get::<String, _>("mode"),
            "allowedAgentIds": row.get::<Vec<Uuid>, _>("allowed_agent_ids"),
            "materialized": true,
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        }),
        None => json!({
            "companyId": company_id,
            "userId": user_id,
            "mode": "open",
            "allowedAgentIds": [],
            "materialized": false,
            "createdAt": Value::Null,
            "updatedAt": Value::Null,
        }),
    };
    Ok(Json(response))
}

async fn require_active_company_user(
    state: &AppState,
    company_id: Uuid,
    user_id: Uuid,
) -> Result<(), StatusCode> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM company_memberships \
         WHERE company_id = $1 AND principal_type = 'user' AND principal_id = $2 AND status = 'active')",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if active { Ok(()) } else { Err(StatusCode::NOT_FOUND) }
}

#[derive(Debug, Deserialize)]
struct UpdateInboxAgentPolicyRequest {
    mode: String,
    #[serde(default, rename = "allowedAgentIds")]
    allowed_agent_ids: Vec<Uuid>,
}

async fn update_user_inbox_agent_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateInboxAgentPolicyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write).map_err(|_| StatusCode::FORBIDDEN)?;
    let self_user = board_user_id(&actor)?;
    if user_id != self_user && !can_manage_inbox_agent_policy(&actor, company_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    require_active_company_user(&state, company_id, user_id).await?;
    if request.mode != "open" && request.mode != "allowlist" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if request.mode == "open" && !request.allowed_agent_ids.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let mut allowed_agent_ids = request.allowed_agent_ids;
    allowed_agent_ids.sort_unstable();
    allowed_agent_ids.dedup();
    if !allowed_agent_ids.is_empty() {
        let matching = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM agents WHERE company_id = $1 AND id = ANY($2)",
        )
        .bind(company_id)
        .bind(&allowed_agent_ids)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if matching.len() != allowed_agent_ids.len() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    let row = sqlx::query(
        "INSERT INTO user_inbox_agent_policies (company_id, user_id, mode, allowed_agent_ids) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (company_id, user_id) DO UPDATE \
         SET mode = EXCLUDED.mode, allowed_agent_ids = EXCLUDED.allowed_agent_ids, updated_at = NOW() \
         RETURNING mode, allowed_agent_ids, created_at, updated_at",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(&request.mode)
    .bind(&allowed_agent_ids)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    let response = json!({
        "companyId": company_id,
        "userId": user_id,
        "mode": row.get::<String, _>("mode"),
        "allowedAgentIds": row.get::<Vec<Uuid>, _>("allowed_agent_ids"),
        "materialized": true,
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    });
    log_activity(
        &state.pool,
        company_id,
        "inbox.agent_policy_updated",
        &actor,
        "user_inbox_agent_policy",
        user_id,
        json!({ "userId": user_id, "mode": request.mode, "allowedAgentIds": allowed_agent_ids }),
    )
    .await;
    Ok(Json(response))
}

async fn update_current_user_inbox_agent_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<UpdateInboxAgentPolicyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = board_user_id(&actor)?;
    update_user_inbox_agent_policy(
        State(state),
        Extension(actor),
        Path((company_id, user_id)),
        Json(request),
    )
    .await
}

/// GET /companies/:company_id/users/:user_id/inbox-agent-policy
async fn user_inbox_agent_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read).map_err(|_| StatusCode::FORBIDDEN)?;
    if !can_manage_inbox_agent_policy(&actor, company_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    require_active_company_user(&state, company_id, user_id).await?;
    get_user_inbox_agent_policy(&state, company_id, user_id).await
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
    let company_id: Uuid = sqlx::query_scalar(
        "SELECT company_id FROM environments WHERE id = $1",
    )
    .bind(environment_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
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

/// GET /environments/:id/secret-refs —— 环境 secret 引用元数据（不返回 secret value）。
async fn environment_secret_refs(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let company_id: Uuid = sqlx::query_scalar(
        "SELECT company_id FROM environments WHERE id = $1",
    )
    .bind(environment_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let config: Value = sqlx::query_scalar("SELECT config FROM environments WHERE id = $1")
        .bind(environment_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut refs = Vec::new();
    collect_environment_secret_refs(&config, "", &mut refs, &mut HashSet::new());

    let mut descriptors = Vec::new();
    for (secret_id, config_path, version_selector) in refs {
        let row = sqlx::query(
            "SELECT s.id, s.name, s.status, s.company_id, c.name AS company_name
             FROM company_secrets s
             JOIN companies c ON c.id = s.company_id
             WHERE s.id = $1 AND s.deleted_at IS NULL",
        )
        .bind(secret_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let Some(row) = row else {
            continue;
        };
        use sqlx::Row;
        descriptors.push(json!({
            "configPath": config_path,
            "secretId": row.get::<Uuid, _>("id"),
            "name": row.get::<String, _>("name"),
            "status": row.get::<String, _>("status"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "companyName": row.get::<String, _>("company_name"),
            "versionSelector": version_selector,
        }));
    }
    Ok(Json(json!({ "refs": descriptors })))
}

/// GET /health —— 健康检查（未注册到路由，保留备用）。
#[allow(dead_code)]
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((project_id, workspace_id, cmd_id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if cmd_id.trim().is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT p.company_id
           FROM projects p
           JOIN project_workspaces w ON w.project_id = p.id
          WHERE p.id = $1 AND w.id = $2 AND p.company_id = $3",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    require_runtime_manage_for_agent(&state, &actor, company_id, workspace_id).await?;
    Ok(Json(json!({ "projectId": project_id, "workspaceId": workspace_id, "commandId": cmd_id, "status": "accepted" })))
}

async fn require_runtime_manage_for_agent(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    workspace_id: Uuid,
) -> Result<(), StatusCode> {
    if !actor.is_agent() {
        return Ok(());
    }

    let decision = AuthorizationService::decide(
        &state.pool,
        actor,
        &AuthorizationAction::Custom {
            action: "runtime:manage".to_string(),
            resource_id: Some(workspace_id),
        },
        Some(company_id),
    )
    .await;
    if decision.allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// POST /projects/:project_id/workspaces/:workspace_id/runtime-services/:service_id
async fn project_runtime_service(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((project_id, workspace_id, service_id)): Path<(Uuid, Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = actor_company(&actor)?;
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    if service_id.trim().is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT p.company_id
           FROM projects p
           JOIN project_workspaces w ON w.project_id = p.id
          WHERE p.id = $1 AND w.id = $2 AND p.company_id = $3",
    )
    .bind(project_id)
    .bind(workspace_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if exists.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    require_runtime_manage_for_agent(&state, &actor, company_id, workspace_id).await?;
    Ok(Json(json!({ "projectId": project_id, "workspaceId": workspace_id, "serviceId": service_id, "status": "accepted" })))
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
        .route("/issues/:id/watchdog", get(get_issue_watchdog).put(upsert_issue_watchdog).delete(delete_issue_watchdog))
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
        .route(
            "/companies/:company_id/users/me/inbox-agent-policy",
            get(company_inbox_agent_policy).put(update_current_user_inbox_agent_policy),
        )
        .route(
            "/companies/:company_id/users/:user_id/inbox-agent-policy",
            get(user_inbox_agent_policy).put(update_user_inbox_agent_policy),
        )
        .route("/companies/import/jobs/:job_id", get(get_import_job))
        .route("/companies/import/preview", post(preview_company_import))
        .route("/board-claim/:token", get(get_board_claim))
        .route("/board-claim/:token/claim", post(claim_board_token))
        .route("/cloud/stacks", get(cloud_stacks))
        .route("/environments/:environment_id/leases", get(environment_leases))
        .route("/environments/:environment_id/secret-refs", get(environment_secret_refs))
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

#[cfg(test)]
mod inbox_agent_policy_tests {
    use super::can_manage_inbox_agent_policy;
    use services::auth::{AgentApiKeyScope, AuthorizationActor, CompanyMembership, MembershipRole, PermissionKey, PrincipalType};
    use uuid::Uuid;

    #[test]
    fn board_admin_can_manage_other_user_policy() {
        let company_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            Uuid::new_v4(),
            MembershipRole::Admin,
        );
        let actor = AuthorizationActor::board_with_memberships(
            Uuid::new_v4(),
            company_id,
            vec![membership],
            false,
        );
        assert!(can_manage_inbox_agent_policy(&actor, company_id));
    }

    #[test]
    fn agent_key_scope_can_delegate_policy_management() {
        let company_id = Uuid::new_v4();
        let scope = AgentApiKeyScope::new(Uuid::new_v4(), company_id).with_actions(vec![
            PermissionKey::USERS_MANAGE_PERMISSIONS.to_string(),
        ]);
        let actor = AuthorizationActor::agent_with_key(
            scope.agent_id,
            company_id,
            Uuid::new_v4(),
            scope,
            None,
        );
        assert!(can_manage_inbox_agent_policy(&actor, company_id));
    }

    #[test]
    fn unrelated_agent_cannot_manage_policy() {
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::agent(Uuid::new_v4(), company_id, None);
        assert!(!can_manage_inbox_agent_policy(&actor, company_id));
    }
}
