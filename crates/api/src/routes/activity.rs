//! Activity routes — P4 收尾域 (AD1-AD4)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn activity_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:id/activity", get(get_issue_activity))
        .route("/companies/:company_id/dashboard", get(get_company_dashboard))
}

async fn get_issue_activity(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

async fn get_company_dashboard(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "activeRuns": 0,
        "pendingApprovals": 0,
        "issuesByStatus": {},
        "agentStatus": {},
    })))
}
