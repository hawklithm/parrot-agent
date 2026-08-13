//! Status Cards 路由 —— 对齐 Paperclip `server/src/routes/status-cards.ts`（12 端点）。
//!
//! 说明：Paperclip 的 recompile/refresh 依赖后台 agent 编译/刷新 worker；Parrot
//! 暂无 agent 摘要执行器，本实现采用同步语义（置 state=compiling、返回 202 形状），
//! 后台执行链留待后续接入。

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use services::auth::AuthorizationActor;

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
}

#[derive(Debug, Deserialize)]
struct WriteSummaryRequest {
    summary: String,
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
    .bind(query.archived)
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
    .bind(created_by_user)
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
    // 同步语义：置 compiling、清 compiled_at、递增 query_version；后台编译链待接入。
    sqlx::query(
        "UPDATE status_cards SET state = 'compiling', query_compiled_at = NULL, \
         query_version = query_version + 1, updated_at = NOW() WHERE id = $1",
    )
    .bind(card_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to recompile status card: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.recompile_requested",
        &actor,
        "status_card",
        card_id,
        json!({}),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok((StatusCode::ACCEPTED, Json(card_json(&updated))))
}

/// POST /status-cards/:id/refresh
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
    sqlx::query(
        "UPDATE status_cards SET updated_at = NOW() WHERE id = $1",
    )
    .bind(card_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to refresh status card: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.refresh_requested",
        &actor,
        "status_card",
        card_id,
        json!({ "enqueued": false }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok((StatusCode::OK, Json(card_json(&updated))))
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
    let current_version = row.get::<i32, _>("query_version");
    let next_version = request.query_version.unwrap_or(current_version + 1).max(current_version);
    sqlx::query(
        "UPDATE status_cards SET queries = $3, query_version = $4, state = 'compiling', updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(card_id)
    .bind(company_id)
    .bind(&request.queries)
    .bind(next_version)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to write status card query: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "status_card.query_written",
        &actor,
        "status_card",
        card_id,
        json!({ "queryVersion": next_version }),
    )
    .await;
    let updated = load_card(&state, card_id).await?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(card_json(&updated)))
}

/// PUT /status-cards/:id/summary
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
    let (compiled_by_agent, compiled_by_user) = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (None, Some(user_id.to_string())),
        _ => (None, None),
    };
    sqlx::query(
        "UPDATE status_cards SET summary_markdown = $3, summary_compiled_at = NOW(), \
         summary_compiled_by_agent_id = $4, state = 'active', updated_at = NOW() \
         WHERE id = $1 AND company_id = $2",
    )
    .bind(card_id)
    .bind(company_id)
    .bind(&request.summary)
    .bind(compiled_by_agent)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to write status card summary: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO status_card_summary_revisions (card_id, markdown, compiled_by_agent_id) \
         VALUES ($1, $2, $3)",
    )
    .bind(card_id)
    .bind(&request.summary)
    .bind(compiled_by_agent)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to record summary revision: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let _ = compiled_by_user;
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
