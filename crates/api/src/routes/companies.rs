//! Company routes — CRUD + stats + branding + archive
//!
//! 对应 Company/Org 模块任务 §1.1 ~ §1.3 + §10 API 路由层

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;
use sqlx::Row;

use crate::app_state::AppState;
use crate::errors::AppError;
use crate::routes::{require_company_access, AccessMode};
use models::{Company, CreateCompanyInput, UpdateCompanyInput};
use services::auth::AuthorizationActor;

/// 对齐 Paperclip companies.ts:290：`/companies/issues` 缺 companyId 时的 400 守卫。
fn row_key(row: &sqlx::postgres::PgRow, key: &str) -> i64 {
    row.try_get::<i64, _>(key).unwrap_or(0)
}

async fn company_issues_guard() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": "Missing companyId in path. Use /api/companies/{companyId}/issues."
        })),
    )
}

pub fn company_routes() -> Router<AppState> {
    Router::new()
        // Company list + create
        .route("/companies", get(list_companies).post(create_company))
        // 对齐 Paperclip companies.ts:290：/companies/issues 缺 companyId 时的 400 守卫
        .route("/companies/issues", get(company_issues_guard))
        // Company stats
        .route("/companies/stats", get(get_company_stats))
        // Single company operations
        .route(
            "/companies/:company_id",
            get(get_company)
                .patch(update_company)
                .delete(delete_company),
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
            // 原 CM9/CM10 占用了 Paperclip 契约路径 /sidebar-preferences/me；
            // 该路径已由 sidebar_preferences.rs（Paperclip 语义）接管，本旧语义
            // handler（user_preferences 表）迁移到非冲突路径保留。
            "/companies/:company_id/preferences",
            get(get_sidebar_preferences).put(update_sidebar_preferences),
        )
        .route(
            "/companies/:company_id/users/:user_slug/profile",
            get(get_user_profile),
        )
        .route("/companies/:company_id/export", post(export_company))
        .route(
            "/companies/:company_id/export/fidelity",
            get(company_export_fidelity),
        )
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
            // 原 CM19/CM20 占用了 Paperclip 契约路径 /inbox-dismissals；
            // 该路径已由 inbox_dismissals.rs（Paperclip dismiss/snooze 语义）
            // 接管，本 issue-inbox-archive 旧语义迁移到非冲突路径保留。
            "/companies/:company_id/issues/inbox-archive",
            get(list_inbox_dismissals).post(dismiss_inbox_item),
        )
        .route(
            "/companies/:company_id/teams-catalog",
            get(get_teams_catalog),
        )
        .route(
            "/companies/:company_id/issues/external-object-summaries",
            post(get_external_object_summaries),
        )
        .layer(axum::middleware::from_fn(
            crate::routes::require_company_access_middleware,
        ))
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
    Extension(actor): Extension<AuthorizationActor>,
    Json(input): Json<CreateCompanyInput>,
) -> Result<(StatusCode, Json<Company>), AppError> {
    let creator_user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        AuthorizationActor::Agent { .. } | AuthorizationActor::None => {
            return Err(AppError::Forbidden(
                "A board user is required to create a company".to_string(),
            ));
        }
    };
    let company = state
        .company_service
        .create(input, creator_user_id)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    // 对齐 Paperclip autoProvisionBundledAgents：公司创建后自动 provision 内置
    // Agent，使其 Instructions / Routine / Managed Resource 生命周期被激活。Summarizer
    // 供 status-card / summary-slot 后台任务链使用；Reflection Coach 供
    // recent-agent-reflection 后台任务使用。两者均按相同模式预置（routine 默认 paused，
    // 需显式 enable 才进入调度）。
    for key in [
        services::BuiltInAgentKey::Summarizer,
        services::BuiltInAgentKey::ReflectionCoach,
    ] {
        match state
            .built_in_agent_service
            .provision(company.id, key, None)
            .await
        {
            Ok(agent) => tracing::info!(
                company_id = %company.id,
                agent_id = %agent.id,
                built_in_key = key.to_string(),
                "auto-provisioned built-in agent"
            ),
            Err(e) => tracing::warn!(
                company_id = %company.id,
                built_in_key = key.to_string(),
                error = %e,
                "failed to auto-provision built-in agent"
            ),
        }
    }
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
    Extension(actor): Extension<AuthorizationActor>,
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
            let user_id = match actor {
                AuthorizationActor::Board { user_id, .. } => user_id,
                AuthorizationActor::Agent { .. } | AuthorizationActor::None => {
                    return Err(AppError::Forbidden(
                        "A board user is required to update company settings".to_string(),
                    ));
                }
            };
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
    let role = payload
        .get("role")
        .or_else(|| payload.get("membershipRole"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("role is required".into()))?;
    let result = sqlx::query("UPDATE company_memberships SET role=$1, updated_at=NOW() WHERE id=$2 AND company_id=$3 AND status='active'")
        .bind(role).bind(member_id).bind(company_id).execute(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Membership not found".into()));
    }
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
    Ok(Json(
        serde_json::json!({"companyId": company_id, "preferences": prefs}),
    ))
}

/// CM10: PUT /companies/:company_id/sidebar-preferences/me
async fn update_sidebar_preferences(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM auth_users ORDER BY created_at LIMIT 1")
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?
            .ok_or_else(|| AppError::NotFound("Current user not found".into()))?;
    sqlx::query("INSERT INTO user_preferences(id,user_id,company_id,preferences) VALUES($1,$2,$3,$4) ON CONFLICT(user_id,company_id) DO UPDATE SET preferences=EXCLUDED.preferences, updated_at=NOW()")
        .bind(Uuid::new_v4()).bind(user_id).bind(company_id).bind(&payload).execute(&state.pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(
        serde_json::json!({"companyId": company_id, "preferences": payload, "updated": true}),
    ))
}

/// CM11: GET /companies/:company_id/users/:user_slug/profile
async fn get_user_profile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, user_slug)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("User profile access denied".into()))?;
    let pool = &state.pool;
    let row = sqlx::query("SELECT id,name,email,avatar_url FROM auth_users WHERE id::text=$1 OR email=$1 OR name=$1 LIMIT 1").bind(&user_slug).fetch_optional(pool).await.map_err(|e| AppError::InternalServerError(e.to_string()))?.ok_or_else(|| AppError::NotFound("User not found".into()))?;
    let user_id: Uuid = row.get("id");
    let email = row.get::<String, _>("email");
    let masked = email
        .split_once('@')
        .map(|(name, domain)| format!("{}***@{}", name.chars().next().unwrap_or('*'), domain))
        .unwrap_or_else(|| "***".into());

    // Window stats mirror Paperclip PROFILE_WINDOWS (last7/last30/all).
    // Parrot issues carry no created_by_user_id/completed_at, so created /
    // completed counts are derived from assignment and the status + update
    // window (documented delta vs Paperclip user-profiles.ts).
    let window_rows = sqlx::query(
        "SELECT \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2) AS touched_all, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.updated_at >= NOW() - INTERVAL '30 days') AS touched_30, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.updated_at >= NOW() - INTERVAL '7 days') AS touched_7, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.status NOT IN ('done','cancelled')) AS open_all, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.status NOT IN ('done','cancelled') AND i.updated_at >= NOW() - INTERVAL '7 days') AS open_7, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.status = 'done' AND i.updated_at >= NOW() - INTERVAL '30 days') AS done_30, \
           (SELECT COUNT(*) FROM issues i WHERE i.company_id = $1 AND i.assignee_user_id = $2 AND i.status = 'done' AND i.updated_at >= NOW() - INTERVAL '7 days') AS done_7",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let window_count = |key: &str| -> i64 { row_key(&window_rows, key) };
    let stats = serde_json::json!([
        { "key": "last7", "label": "Last 7 days", "touchedIssues": window_count("touched_7"), "assignedOpenIssues": window_count("open_7"), "completedIssues": window_count("done_7") },
        { "key": "last30", "label": "Last 30 days", "touchedIssues": window_count("touched_30"), "assignedOpenIssues": window_count("open_all"), "completedIssues": window_count("done_30") },
        { "key": "all", "label": "All time", "touchedIssues": window_count("touched_all"), "assignedOpenIssues": window_count("open_all"), "completedIssues": window_count("done_30") },
    ]);

    let recent_issues: Vec<serde_json::Value> = sqlx::query(
        "SELECT id, identifier, title, status::text AS status, priority::text AS priority, updated_at \
         FROM issues WHERE company_id = $1 AND assignee_user_id = $2 \
         ORDER BY updated_at DESC LIMIT 5",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?
    .iter()
    .map(|r| {
        serde_json::json!({
            "id": r.get::<Uuid, _>("id"),
            "identifier": r.get::<Option<String>, _>("identifier"),
            "title": r.get::<String, _>("title"),
            "status": r.get::<String, _>("status"),
            "priority": r.get::<Option<String>, _>("priority"),
            "updatedAt": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        })
    })
    .collect();

    let recent_activity: Vec<serde_json::Value> = sqlx::query(
        "SELECT event_type, resource_type, resource_id, metadata, created_at \
         FROM activity_logs WHERE company_id = $1 AND actor_id = $2 \
         ORDER BY created_at DESC LIMIT 5",
    )
    .bind(company_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))?
    .iter()
    .map(|r| {
        serde_json::json!({
            "eventType": r.get::<String, _>("event_type"),
            "resourceType": r.get::<Option<String>, _>("resource_type"),
            "resourceId": r.get::<Option<Uuid>, _>("resource_id"),
            "metadata": r.get::<serde_json::Value, _>("metadata"),
            "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        })
    })
    .collect();

    Ok(Json(
        serde_json::json!({
            "companyId": company_id,
            "userSlug": user_slug,
            "canonicalSlug": user_slug,
            "profile": {"id": user_id, "name": row.get::<String,_>("name"), "avatarUrl": row.get::<Option<String>,_>("avatar_url"), "email": masked},
            "stats": stats,
            "recentIssues": recent_issues,
            "recentActivity": recent_activity,
        }),
    ))
}

/// POST /companies/:company_id/export
async fn export_company(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Company export access denied".into()))?;
    Ok(Json(
        state
            .export_service
            .export(company_id, body)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?,
    ))
}

/// GET /companies/:company_id/export/fidelity — export fidelity report
/// (Paperclip services/export-fidelity.ts): what related data exists for
/// the company versus what the plain export carries.
async fn company_export_fidelity(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Company export fidelity access denied".into()))?;
    let pool = &state.pool;
    let mut relation_counts = Vec::new();
    for (key, table) in [
        ("labels", "labels"),
        ("issueLabels", "issue_labels"),
        ("issueRelations", "issue_relations"),
        ("issueDocuments", "issue_documents"),
        ("approvals", "approvals"),
        ("costEvents", "cost_events"),
        ("activityLogs", "activity_logs"),
    ] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE company_id = $1"
        ))
        .bind(company_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
        relation_counts.push(serde_json::json!({ "kind": key, "count": count }));
    }
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "counts": {
            "relations": relation_counts,
            "core": {
                "agents": sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM agents WHERE company_id = $1 AND status <> 'terminated'",
                )
                .bind(company_id)
                .fetch_one(pool)
                .await
                .map_err(|e| AppError::InternalServerError(e.to_string()))?,
                "projects": sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM projects WHERE company_id = $1",
                )
                .bind(company_id)
                .fetch_one(pool)
                .await
                .map_err(|e| AppError::InternalServerError(e.to_string()))?,
                "issues": sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM issues WHERE company_id = $1",
                )
                .bind(company_id)
                .fetch_one(pool)
                .await
                .map_err(|e| AppError::InternalServerError(e.to_string()))?,
            },
        },
        "generatedAt": chrono::Utc::now(),
    })))
}

/// CM13: POST /companies/:company_id/exports/preview
async fn preview_company_export(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Company export preview access denied".into()))?;
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
    const MAX_WINDOW_MS: i64 = 31 * 24 * 3600 * 1000;
    
    let now = chrono::Utc::now();
    let from = query.from.unwrap_or_else(|| now - chrono::Duration::days(7));
    let to = query.to.unwrap_or(now);
    
    let duration_ms = (to - from).num_milliseconds();
    let (actual_from, actual_to, capped) = if duration_ms > MAX_WINDOW_MS {
        (to - chrono::Duration::milliseconds(MAX_WINDOW_MS), to, true)
    } else {
        (from, to, false)
    };
    
    let wq = services::work_timeline_service::WorkTimelineQuery {
        company_id,
        issue_id: query.issue_id,
        user_id: query.user_id,
        goal_id: query.goal_id,
        project_id: query.project_id,
        from: Some(actual_from),
        to: Some(actual_to),
        limit: query.limit,
        offset: query.offset,
    };
    
    let mut issue_ids = state
        .work_timeline_service
        .collect_issue_ids(&wq, actual_from, actual_to)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    
    if let Some(user_id) = query.user_id {
        issue_ids = state
            .work_timeline_service
            .apply_user_lens(company_id, user_id, issue_ids, actual_from, actual_to)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    }
    
    let total_issues = issue_ids.len();
    let offset = query.offset.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(200).min(500).max(1) as usize;
    let has_more = offset + limit < total_issues;
    
    let paged_issue_ids: Vec<Uuid> = issue_ids
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();
    
    if paged_issue_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "actors": [],
            "spans": [],
            "events": [],
            "edges": [],
            "pagination": {
                "limit": limit,
                "offset": offset,
                "totalIssues": total_issues,
                "hasMore": has_more
            },
            "window": {
                "from": actual_from.to_rfc3339(),
                "to": actual_to.to_rfc3339(),
                "capped": capped
            }
        })));
    }
    
    let (spans, comment_events, approval_events, edges) = tokio::try_join!(
        state.work_timeline_service.load_heartbeat_runs(company_id, &paged_issue_ids, actual_from, actual_to),
        state.work_timeline_service.load_issue_comments(company_id, &paged_issue_ids, actual_from, actual_to),
        state.work_timeline_service.load_approvals(company_id, &paged_issue_ids, actual_from, actual_to),
        state.work_timeline_service.extract_edges(company_id, &paged_issue_ids),
    )
    .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    
    let mut events = comment_events;
    events.extend(approval_events);
    
    let mut actor_ids = std::collections::HashSet::new();
    for span in &spans {
        actor_ids.insert(span.actor_id.clone());
    }
    for event in &events {
        actor_ids.insert(event.actor_id.clone());
    }
    for edge in &edges {
        actor_ids.insert(edge.from_actor_id.clone());
        actor_ids.insert(edge.to_actor_id.clone());
    }
    
    let actor_ids_vec: Vec<String> = actor_ids.into_iter().collect();
    let actors = state
        .work_timeline_service
        .load_actors(&actor_ids_vec)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    
    Ok(Json(serde_json::json!({
        "actors": actors,
        "spans": spans,
        "events": events,
        "edges": edges,
        "pagination": {
            "limit": limit,
            "offset": offset,
            "totalIssues": total_issues,
            "hasMore": has_more
        },
        "window": {
            "from": actual_from.to_rfc3339(),
            "to": actual_to.to_rfc3339(),
            "capped": capped
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

#[derive(Debug, Deserialize)]
pub struct ExternalObjectSummariesRequest {
    #[serde(rename = "issueIds")]
    pub issue_ids: Vec<String>,
}

/// POST /companies/:company_id/issues/external-object-summaries - Batch query external object summaries
async fn get_external_object_summaries(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<ExternalObjectSummariesRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Verify company access
    if actor.company_id() != Some(company_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let issue_ids: Vec<Uuid> = payload.issue_ids
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect();
    
    if issue_ids.is_empty() {
        return Ok(Json(serde_json::json!({ "summaries": {} })));
    }
    
    // Query external object summaries for each issue
    let mut summaries = std::collections::HashMap::new();
    
    for issue_id in issue_ids {
        // Verify issue belongs to company
        let issue = sqlx::query("SELECT id FROM issues WHERE id = $1 AND company_id = $2")
            .bind(issue_id)
            .bind(company_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        if issue.is_some() {
            // Get external objects for this issue
            let objects = sqlx::query("SELECT id, object_type, object_id, summary, created_at, updated_at FROM issue_external_objects WHERE issue_id = $1")
                .bind(issue_id)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                let object_summaries: Vec<serde_json::Value> = objects.into_iter().map(|row| {
                serde_json::json!({
                    "id": row.get::<Uuid, _>("id"),
                    "objectType": row.get::<String, _>("object_type"),
                    "objectId": row.get::<String, _>("object_id"),
                    "summary": row.get::<Option<serde_json::Value>, _>("summary"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
                })
            }).collect();
            
            summaries.insert(issue_id.to_string(), serde_json::json!(object_summaries));
        }
    }
    
    Ok(Json(serde_json::json!({ "summaries": summaries })))
}

/// CM17: POST /companies/:company_id/imports/preview
async fn preview_company_import(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read)
        .map_err(|_| AppError::Forbidden("Company import preview access denied".into()))?;
    Ok(Json(
        state
            .import_service
            .preview(company_id, payload)
            .await
            .map_err(map_import_error)?,
    ))
}

/// CM18: POST /companies/:company_id/imports/apply
async fn apply_company_import(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Write)
        .map_err(|_| AppError::Forbidden("Company import apply access denied".into()))?;
    Ok(Json(
        state
            .import_service
            .apply(company_id, payload)
            .await
            .map_err(map_import_error)?,
    ))
}

/// Protocol errors carry validation failures (e.g. an invalid import root
/// path) and map to 400; everything else is a server error.
fn map_import_error(e: sqlx::Error) -> AppError {
    match &e {
        sqlx::Error::Protocol(message) => AppError::BadRequest(message.clone()),
        _ => AppError::InternalServerError(e.to_string()),
    }
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
