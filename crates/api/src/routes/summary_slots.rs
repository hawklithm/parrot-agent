//! Summary Slots 路由 —— 对齐 Paperclip `server/src/routes/summary-slots.ts`（4 端点）。
//! generate 迁移自 Paperclip 后台任务链：创建 hidden issue（Summarizer 内置 agent
//! 执行摘要生成）并通过 heartbeat wakeup 唤醒；write 写回时校验 Summarizer +
//! generationIssueId 匹配。

use axum::{
    extract::{Extension, Path, State},
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
use services::summary_slot_worker::SummarySlotWorker;

fn slot_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    use sqlx::Row;
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "scopeKind": row.get::<String, _>("scope_kind"),
        "scopeId": row.get::<Option<Uuid>, _>("scope_id"),
        "slotKey": row.get::<String, _>("slot_key"),
        "documentId": row.get::<Option<Uuid>, _>("document_id"),
        "status": row.get::<String, _>("status"),
        "failureReason": row.get::<Option<String>, _>("failure_reason"),
        "generatingIssueId": row.get::<Option<Uuid>, _>("generating_issue_id"),
        "lastGeneratedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_generated_at"),
        "lastGeneratedByAgentId": row.get::<Option<Uuid>, _>("last_generated_by_agent_id"),
        "lastModel": row.get::<Option<String>, _>("last_model"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

async fn upsert_slot(
    state: &AppState,
    company_id: Uuid,
    scope_kind: &str,
    scope_id: Option<Uuid>,
    slot_key: &str,
) -> Result<sqlx::postgres::PgRow, StatusCode> {
    let row = sqlx::query(
        "INSERT INTO summary_slots (company_id, scope_kind, scope_id, slot_key) \
         VALUES ($1,$2,$3,$4) \
         ON CONFLICT (company_id, scope_kind, scope_id, slot_key) DO UPDATE SET updated_at = NOW() \
         RETURNING *",
    )
    .bind(company_id)
    .bind(scope_kind)
    .bind(scope_id)
    .bind(slot_key)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to upsert summary slot: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(row)
}

/// GET /companies/:company_id/summary-slots/:scope_kind/:slot_key
async fn get_summary_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, scope_kind, slot_key)): Path<(Uuid, String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT * FROM summary_slots WHERE company_id = $1 AND scope_kind = $2 AND slot_key = $3 \
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(&scope_kind)
    .bind(&slot_key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load summary slot: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    let slot_id: Uuid = row.get("id");
    let revisions = list_revisions(&state, slot_id).await?;
    Ok(Json(json!({ "slot": slot_json(&row), "revisions": revisions })))
}

async fn list_revisions(
    state: &AppState,
    slot_id: Uuid,
) -> Result<Vec<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT id, revision_number, markdown, title, change_summary, model, created_at \
         FROM summary_slot_revisions WHERE slot_id = $1 ORDER BY revision_number DESC",
    )
    .bind(slot_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list summary revisions: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(rows.iter().map(|r| json!({
        "id": r.get::<Uuid, _>("id"),
        "revisionNumber": r.get::<i32, _>("revision_number"),
        "markdown": r.get::<String, _>("markdown"),
        "title": r.get::<Option<String>, _>("title"),
        "changeSummary": r.get::<Option<String>, _>("change_summary"),
        "model": r.get::<Option<String>, _>("model"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect())
}

/// GET /companies/:company_id/summary-slots/:scope_kind/:slot_key/revisions
async fn list_summary_slot_revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, scope_kind, slot_key)): Path<(Uuid, String, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    use sqlx::Row;
    let row = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT id FROM summary_slots WHERE company_id = $1 AND scope_kind = $2 AND slot_key = $3 \
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(&scope_kind)
    .bind(&slot_key)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to load summary slot: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(slot_id) = row else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(list_revisions(&state, slot_id).await?))
}

#[derive(Debug, Deserialize)]
struct GenerateSummarySlotRequest {
    #[serde(rename = "scopeId")]
    scope_id: Option<Uuid>,
}

/// POST /companies/:company_id/summary-slots/:scope_kind/:slot_key/generate
/// 对齐 Paperclip：创建 hidden issue（Summarizer 执行）并唤醒 agent。
async fn generate_summary_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, scope_kind, slot_key)): Path<(Uuid, String, String)>,
    Json(request): Json<GenerateSummarySlotRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let (created_by_agent, created_by_user) = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (None, Some(user_id.to_string())),
        _ => (None, None),
    };
    let worker = SummarySlotWorker::new(state.pool.clone());
    let result = worker
        .generate(
            company_id,
            &scope_kind,
            request.scope_id,
            &slot_key,
            created_by_agent,
            created_by_user,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to enqueue summary slot generation: {}", e);
            if e.contains("Summarizer built-in agent is not configured") {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    if !result.already_generating {
        if let Ok(summarizer) = worker.resolve_summarizer_agent_id(company_id).await {
            let _ = state
                .heartbeat_service
                .wakeup(summarizer, result.generating_issue_id, company_id)
                .await;
        }
    }
    crate::routes::log_activity(
        &state.pool,
        company_id,
        "summary_slot.generate_requested",
        &actor,
        "summary_slot",
        result.slot_id,
        json!({
            "scopeKind": scope_kind,
            "slotKey": slot_key,
            "generatingIssueId": result.generating_issue_id,
            "alreadyGenerating": result.already_generating,
        }),
    )
    .await;
    let row = sqlx::query("SELECT * FROM summary_slots WHERE id = $1")
        .bind(result.slot_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload summary slot: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let status = if result.already_generating {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((
        status,
        Json(json!({
            "slot": slot_json(&row),
            "generatingIssue": {
                "id": result.generating_issue_id,
            },
            "alreadyGenerating": result.already_generating,
        })),
    ))
}

#[derive(Debug, Deserialize)]
struct WriteSummarySlotRequest {
    #[serde(rename = "scopeId")]
    scope_id: Option<Uuid>,
    markdown: String,
    title: Option<String>,
    #[serde(rename = "changeSummary")]
    change_summary: Option<String>,
    #[serde(rename = "baseRevisionId")]
    base_revision_id: Option<Uuid>,
    #[serde(rename = "generationIssueId")]
    generation_issue_id: Option<Uuid>,
    model: Option<String>,
}

/// PUT /companies/:company_id/summary-slots/:scope_kind/:slot_key
/// 仅 Summarizer agent 可写（对齐 Paperclip assertSummarizerWriter）。
async fn write_summary_slot(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, scope_kind, slot_key)): Path<(Uuid, String, String)>,
    Json(request): Json<WriteSummarySlotRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let agent_id = match &actor {
        AuthorizationActor::Agent { agent_id, .. } => *agent_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    // writer 校验：Summarizer 内置 agent + generationIssueId 匹配 slot 占位。
    let worker = SummarySlotWorker::new(state.pool.clone());
    worker
        .assert_summarizer_writer(
            company_id,
            agent_id,
            request.generation_issue_id,
            &scope_kind,
            request.scope_id,
            &slot_key,
        )
        .await
        .map_err(|e| {
            tracing::warn!("Summary slot writer check failed: {}", e);
            StatusCode::FORBIDDEN
        })?;
    let slot = upsert_slot(&state, company_id, &scope_kind, request.scope_id, &slot_key).await?;
    use sqlx::Row;
    let slot_id: Uuid = slot.get("id");
    let next_number: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision_number), 0) + 1 FROM summary_slot_revisions WHERE slot_id = $1",
    )
    .bind(slot_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to compute revision number: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let revision_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO summary_slot_revisions \
         (id, slot_id, revision_number, markdown, title, change_summary, base_revision_id, \
          generation_issue_id, model, created_by_agent_id) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(revision_id)
    .bind(slot_id)
    .bind(next_number)
    .bind(&request.markdown)
    .bind(request.title.as_deref())
    .bind(request.change_summary.as_deref())
    .bind(request.base_revision_id)
    .bind(request.generation_issue_id)
    .bind(request.model.as_deref())
    .bind(agent_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to insert summary revision: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "UPDATE summary_slots SET status = 'idle', last_generated_at = NOW(), \
         last_generated_by_agent_id = $2, last_model = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(slot_id)
    .bind(agent_id)
    .bind(request.model.as_deref())
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update summary slot: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "summary_slot.write",
        &actor,
        "summary_slot",
        slot_id,
        json!({ "scopeKind": scope_kind, "slotKey": slot_key, "revisionId": revision_id, "revisionNumber": next_number }),
    )
    .await;

    let updated = sqlx::query("SELECT * FROM summary_slots WHERE id = $1")
        .bind(slot_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reload slot: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(json!({
        "slot": slot_json(&updated),
        "revision": {
            "id": revision_id,
            "revisionNumber": next_number,
            "markdown": request.markdown,
        },
    })))
}

pub fn summary_slot_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/summary-slots/:scope_kind/:slot_key",
            get(get_summary_slot).put(write_summary_slot),
        )
        .route(
            "/companies/:company_id/summary-slots/:scope_kind/:slot_key/revisions",
            get(list_summary_slot_revisions),
        )
        .route(
            "/companies/:company_id/summary-slots/:scope_kind/:slot_key/generate",
            post(generate_summary_slot),
        )
}
