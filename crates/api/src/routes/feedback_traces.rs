use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

use crate::app_state::AppState;
use services::auth::AuthorizationActor;

#[derive(Debug, Deserialize)]
pub struct FeedbackTraceQuery {
    #[serde(rename = "includePayload")]
    pub include_payload: Option<bool>,
}

/// GET /feedback-traces/:traceId - Get feedback trace by ID
async fn get_feedback_trace(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(trace_id): Path<Uuid>,
    Query(query): Query<FeedbackTraceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Only board users can view feedback traces
    if !matches!(actor, AuthorizationActor::Board { .. }) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let include_payload = query.include_payload.unwrap_or(true);
    
    let row = if include_payload {
        sqlx::query("SELECT id, company_id, issue_id, vote_id, target_type, target_id, payload, status, failure_reason, shared_with_labs, created_at, updated_at FROM feedback_traces WHERE id = $1")
            .bind(trace_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        sqlx::query("SELECT id, company_id, issue_id, vote_id, target_type, target_id, NULL as payload, status, failure_reason, shared_with_labs, created_at, updated_at FROM feedback_traces WHERE id = $1")
            .bind(trace_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    
    let row = row.ok_or(StatusCode::NOT_FOUND)?;
    
    let company_id = row.get::<Uuid, _>("company_id");
    // Verify actor can access this company
    if actor.company_id() != Some(company_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    
    Ok(Json(serde_json::json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": company_id,
        "issueId": row.get::<Uuid, _>("issue_id"),
        "voteId": row.get::<Uuid, _>("vote_id"),
        "targetType": row.get::<String, _>("target_type"),
        "targetId": row.get::<Option<Uuid>, _>("target_id"),
        "payload": row.get::<Option<serde_json::Value>, _>("payload"),
        "status": row.get::<String, _>("status"),
        "failureReason": row.get::<Option<String>, _>("failure_reason"),
        "sharedWithLabs": row.get::<bool, _>("shared_with_labs"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
    })))
}

/// GET /feedback-traces/:traceId/bundle - Get feedback trace bundle
async fn get_feedback_trace_bundle(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(trace_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Only board users can view feedback trace bundles
    if !matches!(actor, AuthorizationActor::Board { .. }) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let row = sqlx::query("SELECT id, company_id, issue_id, vote_id, target_type, target_id, payload, status, failure_reason, shared_with_labs, created_at, updated_at FROM feedback_traces WHERE id = $1")
        .bind(trace_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    let company_id = row.get::<Uuid, _>("company_id");
    // Verify actor can access this company
    if actor.company_id() != Some(company_id) {
        return Err(StatusCode::NOT_FOUND);
    }
    
    // Bundle includes the trace plus related data
    let issue_id = row.get::<Uuid, _>("issue_id");
    let issue = sqlx::query("SELECT id, title, status FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "trace": {
            "id": row.get::<Uuid, _>("id"),
            "issueId": issue_id,
            "voteId": row.get::<Uuid, _>("vote_id"),
            "targetType": row.get::<String, _>("target_type"),
            "targetId": row.get::<Option<Uuid>, _>("target_id"),
            "payload": row.get::<serde_json::Value, _>("payload"),
            "status": row.get::<String, _>("status"),
            "failureReason": row.get::<Option<String>, _>("failure_reason"),
            "sharedWithLabs": row.get::<bool, _>("shared_with_labs"),
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
        },
        "issue": issue.map(|r| serde_json::json!({
            "id": r.get::<Uuid, _>("id"),
            "title": r.get::<String, _>("title"),
            "status": r.get::<String, _>("status")
        }))
    })))
}

pub fn feedback_trace_routes() -> Router<AppState> {
    Router::new()
        .route("/feedback-traces/:trace_id", get(get_feedback_trace))
        .route("/feedback-traces/:trace_id/bundle", get(get_feedback_trace_bundle))
}
