//! Company routes — CRUD + stats + branding + archive
//!
//! 对应 Company/Org 模块任务 §1.1 ~ §1.3 + §10 API 路由层

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use models::{Company, CreateCompanyInput, UpdateCompanyInput};

pub fn company_routes() -> Router<AppState> {
    Router::new()
        // Company list + create
        .route("/companies", get(list_companies).post(create_company))
        // Company stats
        .route("/companies/stats", get(get_company_stats))
        // Single company operations
        .route(
            "/companies/:company_id",
            get(get_company)
                .patch(update_company)
                .delete(delete_company),
        )
        // Dashboard summary
        .route(
            "/companies/:company_id/dashboard",
            get(get_company_dashboard),
        )
        // Company branding
        .route(
            "/companies/:company_id/branding",
            patch(update_company_branding),
        )
        // Company archive
        .route("/companies/:company_id/archive", post(archive_company))
        // --- P3: Companies 补齐 (CM1-CM20) ---
        .route(
            "/companies/:company_id/members/:member_id/permissions",
            patch(update_member_permissions),
        )
        .route("/companies/:company_id/search", get(search_company))
        .route(
            "/companies/:company_id/sidebar-badges",
            get(get_sidebar_badges),
        )
        .route(
            "/companies/:company_id/sidebar-preferences/me",
            get(get_sidebar_preferences).put(update_sidebar_preferences),
        )
        .route(
            "/companies/:company_id/users/:user_slug/profile",
            get(get_user_profile),
        )
        .route("/companies/:company_id/export", post(export_company))
        .route("/companies/:company_id/exports", post(export_company))
        .route(
            "/companies/:company_id/exports/preview",
            post(preview_company_export),
        )
        .route("/companies/:company_id/timeline", get(get_company_timeline))
        .route(
            "/companies/:company_id/artifacts",
            get(get_company_artifacts),
        )
        .route(
            "/companies/:company_id/feedback-traces",
            get(list_company_feedback_traces),
        )
        .route(
            "/companies/:company_id/imports/preview",
            post(preview_company_import),
        )
        .route(
            "/companies/:company_id/imports/apply",
            post(apply_company_import),
        )
        .route(
            "/companies/:company_id/inbox-dismissals",
            get(list_inbox_dismissals).post(dismiss_inbox_item),
        )
        .route(
            "/companies/:company_id/teams-catalog",
            get(get_teams_catalog),
        )
        .layer(axum::middleware::from_fn(crate::routes::require_company_access))
}

#[derive(Debug, Deserialize)]
pub struct ListCompaniesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub issue_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArtifactsQuery {
    pub parent_type: Option<String>,
    pub parent_id: Option<Uuid>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct FeedbackTraceQuery {
    pub issue_id: Option<Uuid>,
    pub target_type: Option<String>,
    pub status: Option<String>,
    pub shared_only: Option<bool>,
    pub include_payload: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /companies
async fn list_companies(
    State(state): State<AppState>,
    Query(query): Query<ListCompaniesQuery>,
) -> Result<Json<Vec<Company>>, AppError> {
    let companies = state
        .company_service
        .list(query.limit.unwrap_or(50), query.offset.unwrap_or(0))
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(companies))
}

/// POST /companies
async fn create_company(
    State(state): State<AppState>,
    Json(input): Json<CreateCompanyInput>,
) -> Result<(StatusCode, Json<Company>), AppError> {
    // TODO: Extract creator_user_id from auth context
    let creator_user_id = Uuid::nil();
    let company = state
        .company_service
        .create(input, creator_user_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(company)))
}

/// GET /companies/stats
///
/// Returns global aggregate company statistics:
/// - total_companies: count of all companies
/// - active_companies: count of companies with status = 'active'
async fn get_company_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let total = state
        .company_service
        .count_all()
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let active = state
        .company_service
        .count_active()
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "total_companies": total,
        "active_companies": active,
    })))
}

/// GET /companies/:company_id/dashboard
///
/// Returns the company dashboard summary consumed by the Paperclip UI.
async fn get_company_dashboard(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let company = sqlx::query_as::<_, (Uuid, Option<i64>)>(
        "SELECT id, budget_monthly_cents FROM companies WHERE id = $1",
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load company: {e}")))?
    .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;

    let agent_rows = sqlx::query(
        "SELECT status::text AS status, COUNT(*)::bigint AS count
         FROM agents WHERE company_id = $1 GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load agent summary: {e}")))?;

    let mut agent_counts = serde_json::Map::from_iter([
        ("active".to_string(), serde_json::json!(0)),
        ("running".to_string(), serde_json::json!(0)),
        ("paused".to_string(), serde_json::json!(0)),
        ("error".to_string(), serde_json::json!(0)),
    ]);
    for row in agent_rows {
        let status: String = row
            .try_get("status")
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        let count: i64 = row
            .try_get("count")
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        let bucket = if status == "idle" { "active" } else { status.as_str() };
        if let Some(value) = agent_counts.get_mut(bucket) {
            *value = serde_json::json!(value.as_i64().unwrap_or(0) + count);
        }
    }

    let issue_rows = sqlx::query(
        "SELECT status::text AS status, COUNT(*)::bigint AS count
         FROM issues WHERE company_id = $1 AND hidden_at IS NULL GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load issue summary: {e}")))?;

    let mut task_counts = serde_json::Map::from_iter([
        ("open".to_string(), serde_json::json!(0)),
        ("inProgress".to_string(), serde_json::json!(0)),
        ("blocked".to_string(), serde_json::json!(0)),
        ("done".to_string(), serde_json::json!(0)),
    ]);
    for row in issue_rows {
        let status: String = row
            .try_get("status")
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        let count: i64 = row
            .try_get("count")
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        if status == "in_progress" {
            task_counts["inProgress"] = serde_json::json!(task_counts["inProgress"].as_i64().unwrap_or(0) + count);
        } else if status == "blocked" {
            task_counts["blocked"] = serde_json::json!(task_counts["blocked"].as_i64().unwrap_or(0) + count);
        } else if status == "done" {
            task_counts["done"] = serde_json::json!(task_counts["done"].as_i64().unwrap_or(0) + count);
        }
        if status != "done" && status != "cancelled" {
            task_counts["open"] = serde_json::json!(task_counts["open"].as_i64().unwrap_or(0) + count);
        }
    }

    let pending_approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM approvals WHERE company_id = $1 AND status = 'pending'",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load approvals: {e}")))?;

    let month_spend_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ce.amount_cents), 0)::bigint
         FROM cost_events ce
         JOIN agents a ON a.id = ce.agent_id
         WHERE a.company_id = $1
           AND ce.created_at >= date_trunc('month', CURRENT_TIMESTAMP)",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(format!("Failed to load cost summary: {e}")))?;

    let month_budget_cents = company.1.unwrap_or(0);
    let utilization = if month_budget_cents > 0 {
        (month_spend_cents as f64 / month_budget_cents as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(serde_json::json!({
        "companyId": company.0,
        "agents": agent_counts,
        "tasks": task_counts,
        "costs": {
            "monthSpendCents": month_spend_cents,
            "monthBudgetCents": month_budget_cents,
            "monthUtilizationPercent": (utilization * 100.0).round() / 100.0,
        },
        "pendingApprovals": pending_approvals,
        "budgets": {
            "activeIncidents": 0,
            "pendingApprovals": pending_approvals,
            "pausedAgents": agent_counts.get("paused").and_then(|v| v.as_i64()).unwrap_or(0),
            "pausedProjects": 0,
        },
        "runActivity": [],
    })))
}

/// GET /companies/:company_id
async fn get_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Company>, AppError> {
    let company = state
        .company_service
        .get_by_id(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;
    Ok(Json(company))
}

/// PATCH /companies/:company_id
///
/// Mirrors Paperclip's company update handler.  When `feedbackDataSharingEnabled`
/// transitions from `false` → `true`, consent fields are auto-populated using
/// the [`TermService`].
async fn update_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<UpdateCompanyInput>,
) -> Result<Json<Company>, AppError> {
    // Mirror Paperclip: when enabling feedback data sharing for the first time,
    // auto-set consent timestamp, user, and terms version.
    if input.feedback_data_sharing_enabled == Some(true) {
        let existing = state
            .company_service
            .get_by_id(company_id)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?
            .ok_or_else(|| AppError::NotFound(format!("Company {} not found", company_id)))?;

        if !existing.feedback_data_sharing_enabled {
            // TODO: Extract user_id from auth context
            let user_id = Uuid::nil();
            input.feedback_data_sharing_consent_at = Some(chrono::Utc::now());
            input.feedback_data_sharing_consent_by_user_id = Some(user_id);
            input.feedback_data_sharing_terms_version = input
                .feedback_data_sharing_terms_version
                .filter(|v| !v.is_empty())
                .or_else(|| Some(state.term_service.default_terms_version().to_string()));
        }
    }

    let company = state
        .company_service
        .update(company_id, input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}

/// DELETE /companies/:company_id
async fn delete_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .company_service
        .delete(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /companies/:company_id/branding
async fn update_company_branding(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<serde_json::Value>,
) -> Result<Json<Company>, AppError> {
    let brand_color = input
        .get("brand_color")
        .and_then(|v| v.as_str().map(String::from));
    let logo_asset_id = input
        .get("logo_asset_id")
        .and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()));
    let company = state
        .company_service
        .update_branding(company_id, brand_color, logo_asset_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}

/// POST /companies/:company_id/archive
async fn archive_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Company>, AppError> {
    let company = state
        .company_service
        .archive(company_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(company))
}

// ============================================================================
// P3: Companies 补齐 Handlers (CM1-CM20)
// ============================================================================

/// CM3: PATCH /companies/:company_id/members/:member_id/permissions
async fn update_member_permissions(
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let role = payload.get("role").or_else(|| payload.get("membershipRole")).and_then(|v| v.as_str()).ok_or_else(|| AppError::BadRequest("role is required".into()))?;
    let result = sqlx::query("UPDATE company_memberships SET role=$1, updated_at=NOW() WHERE id=$2 AND company_id=$3 AND status='active'")
        .bind(role).bind(member_id).bind(company_id).execute(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    if result.rows_affected() == 0 { return Err(AppError::NotFound("Membership not found".into())); }
    Ok(Json(
        serde_json::json!({"companyId": company_id, "memberId": member_id, "updated": true}),
    ))
}

/// CM4: GET /companies/:company_id/search
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

async fn search_company(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Company>>, AppError> {
    let q = query.q.as_deref().unwrap_or("");
    let results = state
        .company_service
        .search(q)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(results))
}

/// CM8: GET /companies/:company_id/sidebar-badges
///
/// Returns notification badge counts for the current user:
/// - pending_approvals: number of approvals awaiting action
/// - unread_issues: issues with unread status for the user
/// - active_monitors: monitors currently running for the company
async fn get_sidebar_badges(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM approvals WHERE company_id = $1 AND status IN ('pending', 'revision_requested')",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    // Paperclip counts the latest failed/timed-out heartbeat per non-terminated agent.
    let failed_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
           SELECT DISTINCT ON (hr.agent_id) hr.status
           FROM heartbeat_runs hr
           JOIN agents a ON a.id = hr.agent_id AND a.company_id = $1
           WHERE hr.company_id = $1 AND a.status <> 'terminated'
           ORDER BY hr.agent_id, hr.created_at DESC
         ) latest WHERE status IN ('failed', 'timed_out')",
    )
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "inbox": approvals + failed_runs,
        "approvals": approvals,
        "failedRuns": failed_runs,
        "joinRequests": 0
    })))
}

/// CM9: GET /companies/:company_id/sidebar-preferences/me
async fn get_sidebar_preferences(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let prefs = sqlx::query_scalar::<_, serde_json::Value>("SELECT preferences FROM user_preferences WHERE company_id=$1 AND user_id=(SELECT id FROM auth_users ORDER BY created_at LIMIT 1)")
        .bind(company_id).fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(serde_json::json!({"companyId": company_id, "preferences": prefs})))
}

/// CM10: PUT /companies/:company_id/sidebar-preferences/me
async fn update_sidebar_preferences(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM auth_users ORDER BY created_at LIMIT 1").fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("Current user not found".into()))?;
    sqlx::query("INSERT INTO user_preferences(id,user_id,company_id,preferences) VALUES($1,$2,$3,$4) ON CONFLICT(user_id,company_id) DO UPDATE SET preferences=EXCLUDED.preferences, updated_at=NOW()")
        .bind(Uuid::new_v4()).bind(user_id).bind(company_id).bind(&payload).execute(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(serde_json::json!({"companyId": company_id, "preferences": payload, "updated": true})))
}

/// CM11: GET /companies/:company_id/users/:user_slug/profile
async fn get_user_profile(
    State(state): State<AppState>,
    Path((company_id, user_slug)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query("SELECT id,name,email,avatar_url FROM auth_users WHERE id::text=$1 OR email=$1 OR name=$1 LIMIT 1").bind(&user_slug).fetch_optional(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("User not found".into()))?;
    let email = row.get::<String,_>("email");
    let masked = email.split_once('@').map(|(name,domain)| format!("{}***@{}", name.chars().next().unwrap_or('*'), domain)).unwrap_or_else(|| "***".into());
    Ok(Json(serde_json::json!({"companyId": company_id, "userSlug": user_slug, "profile": {"id": row.get::<Uuid,_>("id"), "name": row.get::<String,_>("name"), "avatarUrl": row.get::<Option<String>,_>("avatar_url"), "email": masked}})))
}

/// POST /companies/:company_id/export
async fn export_company(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .export_service
            .export(company_id, body)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// CM13: POST /companies/:company_id/exports/preview
async fn preview_company_export(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .export_service
            .preview(company_id, body)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// CM14: GET /companies/:company_id/timeline
///
/// Delegates to `WorkTimelineService` to collect issue IDs and load events
/// from multiple sources (activity_logs, heartbeat_runs, issue_comments, etc.).
async fn get_company_timeline(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let wq = services::work_timeline_service::WorkTimelineQuery {
        company_id,
        issue_id: query.issue_id,
        user_id: query.user_id,
        goal_id: query.goal_id,
        project_id: query.project_id,
    };
    let raw_events = state
        .work_timeline_service
        .load_events(&wq)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let now = chrono::Utc::now();
    let from = query
        .from
        .unwrap_or_else(|| now - chrono::Duration::days(7));
    let to = query.to.unwrap_or(now);

    let mut actors = Vec::new();
    let mut events = Vec::new();
    for event in raw_events {
        let actor_id = event
            .get("actorId")
            .and_then(|value| value.as_str())
            .map(|id| format!("system:{id}"))
            .unwrap_or_else(|| "system:system".to_string());
        if !actors.iter().any(|actor: &serde_json::Value| actor["id"] == actor_id) {
            actors.push(serde_json::json!({
                "id": actor_id,
                "type": "system",
                "name": "System",
                "avatar": null
            }));
        }
        let issue_id = event.get("resourceId").cloned().unwrap_or(serde_json::Value::Null);
        let at = event.get("createdAt").cloned().unwrap_or(serde_json::json!(to));
        let kind = match event.get("eventType").and_then(|value| value.as_str()) {
            Some(value) if value.contains("comment") => "commented",
            Some(value) if value.contains("assign") => "assigned",
            Some(value) if value.contains("approv") => "approved",
            Some(value) if value.contains("delegat") => "delegated",
            _ => "created",
        };
        events.push(serde_json::json!({
            "actorId": actor_id,
            "kind": kind,
            "issueId": issue_id,
            "at": at
        }));
    }

    Ok(Json(serde_json::json!({
        "actors": actors,
        "spans": [],
        "events": events,
        "edges": [],
        "pagination": {
            "limit": 200,
            "offset": 0,
            "totalIssues": events.len(),
            "hasMore": false
        },
        "window": {
            "from": from,
            "to": to,
            "capped": false
        }
    })))
}

/// CM15: GET /companies/:company_id/artifacts
async fn get_company_artifacts(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ArtifactsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT id, parent_type, parent_id, asset_id, filename, content_type, size_bytes, created_at FROM attachments WHERE company_id = $1 AND ($2::text IS NULL OR parent_type = $2) AND ($3::uuid IS NULL OR parent_id = $3) AND ($4::timestamptz IS NULL OR created_at >= $4) AND ($5::timestamptz IS NULL OR created_at <= $5) ORDER BY created_at DESC LIMIT $6 OFFSET $7")
        .bind(company_id).bind(query.parent_type).bind(query.parent_id).bind(query.from).bind(query.to)
        .bind(query.limit.unwrap_or(100).clamp(1, 500)).bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id": row.get::<Uuid, _>("id"), "parentType": row.get::<String, _>("parent_type"),
        "parentId": row.get::<Uuid, _>("parent_id"), "assetId": row.get::<Uuid, _>("asset_id"),
        "filename": row.get::<String, _>("filename"), "contentType": row.get::<String, _>("content_type"),
        "sizeBytes": row.get::<i64, _>("size_bytes"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
    })).collect()))
}

/// CM16: GET /companies/:company_id/feedback-traces
async fn list_company_feedback_traces(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<FeedbackTraceQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT id, issue_id, vote_id, target_type, target_id, payload, status, failure_reason, shared_with_labs, created_at, updated_at FROM feedback_traces WHERE company_id = $1 AND ($2::uuid IS NULL OR issue_id = $2) AND ($3::text IS NULL OR target_type = $3) AND ($4::text IS NULL OR status = $4) AND ($5::bool IS NULL OR shared_with_labs = $5) ORDER BY created_at DESC LIMIT $6 OFFSET $7")
        .bind(company_id).bind(query.issue_id).bind(query.target_type).bind(query.status).bind(query.shared_only)
        .bind(query.limit.unwrap_or(100).clamp(1, 500)).bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id": row.get::<Uuid, _>("id"), "issueId": row.get::<Uuid, _>("issue_id"),
        "voteId": row.get::<Uuid, _>("vote_id"), "targetType": row.get::<String, _>("target_type"),
        "targetId": row.get::<Option<Uuid>, _>("target_id"), "payload": row.get::<serde_json::Value, _>("payload"),
        "status": row.get::<String, _>("status"), "failureReason": row.get::<Option<String>, _>("failure_reason"),
        "sharedWithLabs": row.get::<bool, _>("shared_with_labs"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    })).collect()))
}

/// CM17: POST /companies/:company_id/imports/preview
async fn preview_company_import(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .import_service
            .preview(company_id, payload)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// CM18: POST /companies/:company_id/imports/apply
async fn apply_company_import(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .import_service
            .apply(company_id, payload)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// CM19: GET /companies/:company_id/inbox-dismissals
async fn list_inbox_dismissals(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT id, issue_id, user_id, archived_at, updated_at FROM issue_inbox_archives WHERE company_id = $1 ORDER BY updated_at DESC LIMIT 500")
        .bind(company_id).fetch_all(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id": row.get::<Uuid, _>("id"), "issueId": row.get::<Uuid, _>("issue_id"),
        "userId": row.get::<Uuid, _>("user_id"), "archivedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("archived_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    })).collect()))
}

/// CM20: POST /companies/:company_id/inbox-dismissals
async fn dismiss_inbox_item(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .inbox_service
            .dismiss(company_id, payload)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// CM21: GET /companies/:company_id/teams-catalog
async fn get_teams_catalog(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT manifest FROM plugins WHERE status IN ('ready','enabled','installed') AND (manifest->>'type'='team-catalog' OR manifest->'teamCatalog' IS NOT NULL) ORDER BY install_order")
        .fetch_all(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({"companyId": company_id, "manifest": row.get::<serde_json::Value,_>("manifest")})).collect()))
}
