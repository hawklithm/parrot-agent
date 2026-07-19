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
        // Company branding
        .route(
            "/companies/:company_id/branding",
            patch(update_company_branding),
        )
        // Company archive
        .route("/companies/:company_id/archive", post(archive_company))
        // --- P3: Companies 补齐 (CM1-CM20) ---
        .route(
            "/companies/:company_id/activity",
            get(list_company_activity).post(record_company_activity),
        )
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
pub struct TimelineQuery {
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub issue_id: Option<Uuid>,
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
async fn get_company_stats(
    State(_state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: Implement global company stats
    Ok(Json(serde_json::json!({
        "total_companies": 0,
        "active_companies": 0,
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

/// CM1: GET /companies/:company_id/activity
async fn list_company_activity(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// CM2: POST /companies/:company_id/activity
async fn record_company_activity(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "recorded": true}),
    ))
}

/// CM3: PATCH /companies/:company_id/members/:member_id/permissions
async fn update_member_permissions(
    State(_state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(_payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "memberId": member_id, "updated": true}),
    ))
}

/// CM4: GET /companies/:company_id/search
async fn search_company(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// CM8: GET /companies/:company_id/sidebar-badges
async fn get_sidebar_badges(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

/// CM9: GET /companies/:company_id/sidebar-preferences/me
async fn get_sidebar_preferences(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "preferences": {}}),
    ))
}

/// CM10: PUT /companies/:company_id/sidebar-preferences/me
async fn update_sidebar_preferences(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "preferences": payload, "updated": true}),
    ))
}

/// CM11: GET /companies/:company_id/users/:user_slug/profile
async fn get_user_profile(
    State(_state): State<AppState>,
    Path((company_id, user_slug)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        serde_json::json!({"companyId": company_id, "userSlug": user_slug, "profile": {}}),
    ))
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
async fn get_company_timeline(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let rows = sqlx::query("SELECT id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at FROM activity_logs WHERE company_id = $1 AND ($2::timestamptz IS NULL OR created_at >= $2) AND ($3::timestamptz IS NULL OR created_at <= $3) AND ($4::uuid IS NULL OR resource_id = $4) ORDER BY created_at DESC LIMIT $5 OFFSET $6")
        .bind(company_id).bind(query.from).bind(query.to).bind(query.issue_id)
        .bind(query.limit.unwrap_or(100).clamp(1, 500)).bind(query.offset.unwrap_or(0).max(0))
        .fetch_all(&state.pool).await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id": row.get::<Uuid, _>("id"), "eventType": row.get::<String, _>("event_type"),
        "actorType": row.get::<String, _>("actor_type"), "actorId": row.get::<Uuid, _>("actor_id"),
        "resourceType": row.get::<String, _>("resource_type"), "resourceId": row.get::<Uuid, _>("resource_id"),
        "metadata": row.get::<serde_json::Value, _>("metadata"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
    })).collect()))
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
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    // Paperclip's catalog is a package manifest, not an agents query.
    Ok(Json(Vec::new()))
}
