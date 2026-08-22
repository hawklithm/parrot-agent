//! Status Cards 路由 —— 对齐 Paperclip `server/src/routes/status-cards.ts`（12 端点）。
//!
//! recompile/refresh 迁移自 Paperclip 后台任务链：创建 hidden issue（assignee =
//! Summarizer 内置 agent）并通过 heartbeat wakeup 唤醒执行；query/summary 写回时
//! 强校验 writer 身份与 generationIssueId 匹配。

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;
use services::{status_card_worker::StatusCardWorker, HeartbeatWakeupOptions};

fn card_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "createdByUserId": row.get::<Option<String>, _>("created_by_user_id"),
        "createdByAgentId": row.get::<Option<Uuid>, _>("created_by_agent_id"),
        "title": row.get::<Option<String>, _>("title"),
        "titlePinned": row.get::<bool, _>("title_pinned"),
        "interestPrompt": row.get::<String, _>("interest_prompt"),
        "queries": row.get::<Value, _>("queries"),
        "queryVersion": row.get::<i32, _>("query_version"),
        "queryCompiledAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("query_compiled_at"),
        "queryCompiledByAgentId": row.get::<Option<Uuid>, _>("query_compiled_by_agent_id"),
        "agentId": row.get::<Option<Uuid>, _>("agent_id"),
        "refreshPolicy": row.get::<Value, _>("refresh_policy"),
        "state": row.get::<String, _>("state"),
        "pendingChangeCount": row.get::<i32, _>("pending_change_count"),
        "archivedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at"),
        "generatingIssueId": row.get::<Option<Uuid>, _>("generating_issue_id"),
        "summaryMarkdown": row.get::<Option<String>, _>("summary_markdown"),
        "summaryCompiledAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("summary_compiled_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

async fn load_card(
    state: &AppState,
    card_id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, StatusCode> {
    sqlx::query("SELECT * FROM status_cards WHERE id = $1")
        .bind(card_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to load status card: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Deserialize)]
struct ListStatusCardsQuery {
    archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateStatusCardRequest {
    title: Option<String>,
    #[serde(rename = "titlePinned")]
    title_pinned: Option<bool>,
    #[serde(rename = "interestPrompt")]
    interest_prompt: Option<String>,
    queries: Option<Value>,
    #[serde(rename = "refreshPolicy")]
    refresh_policy: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PatchStatusCardRequest {
    title: Option<String>,
    #[serde(rename = "titlePinned")]
    title_pinned: Option<bool>,
    #[serde(rename = "interestPrompt")]
    interest_prompt: Option<String>,
    queries: Option<Value>,
    #[serde(rename = "refreshPolicy")]
    refresh_policy: Option<Value>,
    archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WriteQueryRequest {
    queries: Value,
    #[serde(rename = "queryVersion")]
    query_version: Option<i32>,
    #[serde(rename = "generationIssueId")]
    generation_issue_id: Option<Uuid>,
    #[serde(rename = "changeSummary")]
    change_summary: Option<String>,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriteSummaryRequest {
    summary: String,
    #[serde(rename = "generationIssueId")]
    generation_issue_id: Option<Uuid>,
    #[serde(rename = "changeSummary")]
    change_summary: Option<String>,
    #[allow(dead_code)]
    title: Option<String>,
    model: Option<String>,
}

/// GET /companies/:company_id/status-cards
async fn list_status_cards(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListStatusCardsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT * FROM status_cards WHERE company_id = $1 AND ($2::boolean IS NULL OR (archived_at IS NOT NULL) = $2) \
         ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(query.archived.unwrap_or(false))
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list status cards: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(card_json).collect()))
}

/// POST /companies/:company_id/status-cards
async fn create_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(request): Json<CreateStatusCardRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (created_by_user, created_by_agent) = match &actor {
        AuthorizationActor::Board { user_id, .. } => (Some(user_id.to_string()), None),
        AuthorizationActor::Agent { agent_id, .. } => (None, Some(*agent_id)),
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO status_cards \
         (id, company_id, created_by_user_id, created_by_agent_id, title, title_pinned, \
          interest_prompt, queries, refresh_policy, state) \
         VALUES ($1,$2,$3,$4,$5,COALESCE($6,false),COALESCE($7,''),COALESCE($8,'[]'::jsonb),COALESCE($9,'{}'::jsonb),'compiling')",
    )
    .bind(id)
    .bind(company_id)
    .bind(created_by_user.as_deref())
    .bind(created_by_agent)
    .bind(request.title.as_deref())
    .bind(request.title_pinned)
    .bind(request.interest_prompt.as_deref())
    .bind(request.queries.as_ref())
    .bind(request.refresh_policy.as_ref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create status card: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.created",
        &actor,
        "status_card",
        id,
        json!({ "state": "compiling" }),
    )
    .await;

    // 对齐 Paperclip：创建后自动 enqueue compile（Summarizer 编译查询并写首版摘要）。
    let worker = StatusCardWorker::new(state.pool.clone());
    let created_by_user_uuid = created_by_user
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    if let Ok(compile) = worker
        .request_compile(
            id,
            created_by_agent,
            created_by_user_uuid,
        )
        .await
    {
        if !compile.already_generating {
            if let Ok(summarizer) = worker.resolve_summarizer_agent_id(company_id, None).await {
                let _ = state
                    .heartbeat_service
                    .wakeup_with_options(
                        summarizer,
                        compile.generating_issue_id,
                        company_id,
                        HeartbeatWakeupOptions {
                            source: Some("on_demand".to_string()),
                            trigger_detail: Some("system".to_string()),
                            reason: Some("status_card_generation".to_string()),
                            ..Default::default()
                        },
                    )
                    .await;
            }
        }
    }

    let row = load_card(&state, id).await?.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(card_json(&row))))
}

/// GET /status-cards/:id
async fn get_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(card_json(&row)))
}

/// PATCH /status-cards/:id
async fn patch_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
    Json(request): Json<PatchStatusCardRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let archived_at = match request.archived {
        Some(true) => Some(chrono::Utc::now()),
        Some(false) => None,
        None => row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at"),
    };
    sqlx::query(
        "UPDATE status_cards SET \
         title = COALESCE($3, title), \
         title_pinned = COALESCE($4, title_pinned), \
         interest_prompt = COALESCE($5, interest_prompt), \
         queries = COALESCE($6, queries), \
         refresh_policy = COALESCE($7, refresh_policy), \
         archived_at = $8, \
         updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(card_id)
    .bind(company_id)
    .bind(request.title.as_deref())
    .bind(request.title_pinned)
    .bind(request.interest_prompt.as_deref())
    .bind(request.queries.as_ref())
    .bind(request.refresh_policy.as_ref())
    .bind(archived_at)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update status card: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.updated",
        &actor,
        "status_card",
        card_id,
        json!({ "archived": archived_at.is_some() }),
    )
    .await;

    // 对齐 Paperclip：interestPrompt 变更后自动 recompile；取消归档且已有查询时
    // 触发一次 restore 刷新。
    let prompt_changed = request.interest_prompt.is_some();
    if prompt_changed {
        let (created_by_agent, created_by_user) = match &actor {
            AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
            AuthorizationActor::Board { user_id, .. } => (None, Some(*user_id)),
            _ => (None, None),
        };
        let worker = StatusCardWorker::new(state.pool.clone());
        if let Ok(compile) = worker
            .request_compile(card_id, created_by_agent, created_by_user)
            .await
        {
            if !compile.already_generating {
                if let Ok(summarizer) = worker.resolve_summarizer_agent_id(company_id, None).await {
                    let _ = state
                        .heartbeat_service
                        .wakeup_with_options(
                            summarizer,
                            compile.generating_issue_id,
                            company_id,
                            HeartbeatWakeupOptions {
                                source: Some("on_demand".to_string()),
                                trigger_detail: Some("system".to_string()),
                                reason: Some("status_card_generation".to_string()),
                                ..Default::default()
                            },
                        )
                        .await;
                }
            }
        }
    }

    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(card_json(&updated)))
}

/// DELETE /status-cards/:id
async fn delete_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("DELETE FROM status_cards WHERE id = $1")
        .bind(card_id)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete status card: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.deleted",
        &actor,
        "status_card",
        card_id,
        json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /status-cards/:id/updates
async fn list_status_card_updates(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT id, issue_id, identifier, from_status, to_status, change_kind, created_at \
         FROM status_card_updates WHERE card_id = $1 ORDER BY created_at DESC",
    )
    .bind(card_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list status card updates: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "issueId": r.get::<Option<Uuid>, _>("issue_id"),
        "identifier": r.get::<Option<String>, _>("identifier"),
        "fromStatus": r.get::<Option<String>, _>("from_status"),
        "toStatus": r.get::<Option<String>, _>("to_status"),
        "changeKind": r.get::<String, _>("change_kind"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
}

/// GET /status-cards/:id/summary-revisions
async fn list_status_card_revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let rows = sqlx::query(
        "SELECT id, markdown, compiled_by_agent_id, created_at \
         FROM status_card_summary_revisions WHERE card_id = $1 ORDER BY created_at DESC",
    )
    .bind(card_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list summary revisions: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "markdown": r.get::<String, _>("markdown"),
        "compiledByAgentId": r.get::<Option<Uuid>, _>("compiled_by_agent_id"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
}

/// POST /status-cards/:id/recompile
/// 对齐 Paperclip：创建 hidden issue（Summarizer 执行编译）并唤醒 agent。
async fn recompile_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (created_by_agent, created_by_user) = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (None, Some(*user_id)),
        _ => (None, None),
    };
    let worker = StatusCardWorker::new(state.pool.clone());
    let result = worker
        .request_compile(card_id, created_by_agent, created_by_user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enqueue status card compile: {}", e);
            match e.as_str() {
                "Status card not found" => StatusCode::NOT_FOUND,
                "Archived status cards cannot be compiled" => StatusCode::UNPROCESSABLE_ENTITY,
                _ if e.contains("Summarizer built-in agent is not configured") => {
                    StatusCode::UNPROCESSABLE_ENTITY
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;
    // 唤醒 Summarizer agent 执行（非幂等命中时）。
    if !result.already_generating {
        if let Ok(summarizer) = worker.resolve_summarizer_agent_id(company_id, None).await {
            let _ = state
                .heartbeat_service
                .wakeup_with_options(
                    summarizer,
                    result.generating_issue_id,
                    company_id,
                    HeartbeatWakeupOptions {
                        source: Some("on_demand".to_string()),
                        trigger_detail: Some("system".to_string()),
                        reason: Some("status_card_generation".to_string()),
                        ..Default::default()
                    },
                )
                .await;
        }
    }
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.recompile_requested",
        &actor,
        "status_card",
        card_id,
        json!({
            "generatingIssueId": result.generating_issue_id,
            "alreadyGenerating": result.already_generating,
        }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    let status = if result.already_generating {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(json!({
            "card": card_json(&updated),
            "generatingIssueId": result.generating_issue_id,
            "alreadyGenerating": result.already_generating,
        })),
    ))
}

/// POST /status-cards/:id/refresh
/// 对齐 Paperclip：创建 hidden update issue（Summarizer 执行刷新）并唤醒 agent。
async fn refresh_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (created_by_agent, created_by_user) = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (None, Some(*user_id)),
        _ => (None, None),
    };
    let worker = StatusCardWorker::new(state.pool.clone());
    let result = worker
        .request_refresh(card_id, false, "manual", created_by_agent, created_by_user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enqueue status card refresh: {}", e);
            match e.as_str() {
                "Status card not found" => StatusCode::NOT_FOUND,
                "Archived status cards cannot be refreshed" => StatusCode::UNPROCESSABLE_ENTITY,
                _ if e.contains("Compile the status-card query") => StatusCode::CONFLICT,
                _ if e.contains("Summarizer built-in agent is not configured") => {
                    StatusCode::UNPROCESSABLE_ENTITY
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;
    if !result.already_generating {
        if let Ok(summarizer) = worker.resolve_summarizer_agent_id(company_id, None).await {
            let _ = state
                .heartbeat_service
                .wakeup_with_options(
                    summarizer,
                    result.generating_issue_id,
                    company_id,
                    HeartbeatWakeupOptions {
                        source: Some("on_demand".to_string()),
                        trigger_detail: Some("system".to_string()),
                        reason: Some("status_card_generation".to_string()),
                        ..Default::default()
                    },
                )
                .await;
        }
    }
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.refresh_requested",
        &actor,
        "status_card",
        card_id,
        json!({
            "generatingIssueId": result.generating_issue_id,
            "alreadyGenerating": result.already_generating,
            "enqueued": !result.already_generating,
        }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    let status = if result.already_generating {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(json!({
            "card": card_json(&updated),
            "generatingIssueId": result.generating_issue_id,
            "alreadyGenerating": result.already_generating,
            "enqueued": !result.already_generating,
        })),
    ))
}

/// GET /status-cards/:id/dry-run
async fn dry_run_status_card(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(json!({
        "cardId": card_id,
        "queryVersion": row.get::<i32, _>("query_version"),
        "queries": row.get::<Value, _>("queries"),
        "mentionedIssues": [],
    })))
}

/// PUT /status-cards/:id/query
/// 对齐 Paperclip：仅 Summarizer agent 且 generationIssueId 匹配时允许写回。
async fn write_status_card_query(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
    Json(request): Json<WriteQueryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => *agent_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    // writer 校验：generationIssueId 必须匹配卡片当前占位。
    let active_gid: Option<Uuid> = row.get("generating_issue_id");
    let Some(gid) = request.generation_issue_id else {
        return Err(StatusCode::FORBIDDEN);
    };
    if active_gid != Some(gid) {
        return Err(StatusCode::CONFLICT);
    }
    let current_version = row.get::<i32, _>("query_version");
    let next_version = request.query_version.unwrap_or(current_version + 1).max(current_version);
    sqlx::query(
        "UPDATE status_cards SET queries = $3, query_version = $4, state = 'compiling', \
         query_compiled_at = NOW(), query_compiled_by_agent_id = $5, failure_reason = NULL, \
         title = CASE WHEN title_pinned THEN title ELSE COALESCE($6, title) END, \
         updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(card_id)
    .bind(company_id)
    .bind(&request.queries)
    .bind(next_version)
    .bind(agent_id)
    .bind(request.title.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to write status card query: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // 记录执行记录（compile kind）。
    let _ = sqlx::query(
        "INSERT INTO status_card_update_runs \
         (card_id, kind, trigger, generation_issue_id, query_version, change_summary, status, finished_at) \
         VALUES ($1, 'compile', 'manual', $2, $3, $4, 'ok', NOW())",
    )
    .bind(card_id)
    .bind(gid)
    .bind(next_version)
    .bind(request.change_summary.as_deref())
    .execute(&state.pool)
    .await;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.query_written",
        &actor,
        "status_card",
        card_id,
        json!({ "queryVersion": next_version, "generationIssueId": gid }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(card_json(&updated)))
}

/// PUT /status-cards/:id/summary
/// 对齐 Paperclip：仅 Summarizer agent 且 generationIssueId 匹配时允许写回；
/// 完成后释放 generating_issue_id、写 summary revision 并推进 next_eval_at。
async fn write_status_card_summary(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(card_id): Path<Uuid>,
    Json(request): Json<WriteSummaryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    use sqlx::Row;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => *agent_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let active_gid: Option<Uuid> = row.get("generating_issue_id");
    let Some(gid) = request.generation_issue_id else {
        return Err(StatusCode::FORBIDDEN);
    };
    if active_gid != Some(gid) {
        return Err(StatusCode::CONFLICT);
    }
    let refresh_policy: Value = row.get("refresh_policy");
    let now = chrono::Utc::now();
    let next_eval = services::status_card_worker::next_status_card_evaluation_at(
        &refresh_policy,
        &now,
    );
    let query_version: i32 = row.get("query_version");
    let revision_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE status_cards SET summary_markdown = $3, summary_compiled_at = NOW(), \
         summary_compiled_by_agent_id = $4, state = 'active', failure_reason = NULL, \
         generating_issue_id = NULL, last_generated_at = NOW(), last_model = $5, \
         last_update_run_kind = $6, next_eval_at = $7, updated_at = NOW() \
         WHERE id = $1 AND company_id = $2 AND generating_issue_id = $8",
    )
    .bind(card_id)
    .bind(company_id)
    .bind(&request.summary)
    .bind(agent_id)
    .bind(request.model.as_deref())
    .bind("full")
    .bind(next_eval)
    .bind(gid)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to write status card summary: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO status_card_summary_revisions \
         (id, card_id, markdown, compiled_by_agent_id, created_at) \
         VALUES ($1, $2, $3, $4, NOW())",
    )
    .bind(revision_id)
    .bind(card_id)
    .bind(&request.summary)
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to record summary revision: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // 关闭执行记录。
    let _ = sqlx::query(
        "UPDATE status_card_update_runs SET status = 'ok', finished_at = NOW(), \
         model = COALESCE($3, model), change_summary = COALESCE($4, change_summary) \
         WHERE generation_issue_id = $1 AND card_id = $2 AND status = 'running'",
    )
    .bind(gid)
    .bind(card_id)
    .bind(request.model.as_deref())
    .bind(request.change_summary.as_deref())
    .execute(&state.pool)
    .await;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.summary_written",
        &actor,
        "status_card",
        card_id,
        json!({ "queryVersion": query_version, "generationIssueId": gid }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(card_json(&updated)))
}

pub fn status_card_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/status-cards",
            get(list_status_cards).post(create_status_card),
        )
        .route("/status-cards/:id", get(get_status_card).patch(patch_status_card).delete(delete_status_card))
        .route("/status-cards/:id/updates", get(list_status_card_updates))
        .route("/status-cards/:id/summary-revisions", get(list_status_card_revisions))
        .route("/status-cards/:id/recompile", post(recompile_status_card))
        .route("/status-cards/:id/refresh", post(refresh_status_card))
        .route("/status-cards/:id/dry-run", get(dry_run_status_card))
        .route("/status-cards/:id/query", put(write_status_card_query))
        .route("/status-cards/:id/summary", put(write_status_card_summary))
}
