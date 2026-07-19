use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app_state::AppState;

/// Blocker diagnostics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockersDiagnostics {
    pub is_blocked: bool,
    pub blocker_chain: Vec<BlockedIssueInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedIssueInfo {
    pub issue_id: Uuid,
    pub title: String,
    pub status: String,
}

/// Wakes diagnostics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakesDiagnostics {
    pub pending_wakes: i64,
    pub active_wakes: Vec<serde_json::Value>,
}

/// Subtree diagnostics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtreeDiagnostics {
    pub issue_id: Uuid,
    pub total_descendants: i64,
    pub status_breakdown: Vec<StatusCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

/// Helper: 通过 issue_id 查询 company_id
async fn get_company_id_for_issue(state: &AppState, issue_id: Uuid) -> Result<Uuid, StatusCode> {
    let issue = state
        .issue_service
        .get(issue_id, Uuid::nil())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(issue.company_id)
}

/// GET /issues/:id/diagnostics/blockers
async fn get_blockers_diagnostics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BlockersDiagnostics>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = get_company_id_for_issue(&state, id).await?;

    let issue = service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let status_str = format!("{:?}", issue.status);
    let is_blocked = status_str.to_lowercase() == "blocked";

    // Walk up parent chain to find blocked ancestors
    let mut blocker_chain = Vec::new();
    let mut current_parent_id = issue.parent_id;

    while let Some(pid) = current_parent_id {
        if let Ok(Some(parent)) = service.get(pid, company_id).await {
            let parent_status = format!("{:?}", parent.status);
            if parent_status.to_lowercase() == "blocked" {
                blocker_chain.push(BlockedIssueInfo {
                    issue_id: parent.id,
                    title: parent.title.clone(),
                    status: parent_status,
                });
            }
            current_parent_id = parent.parent_id;
        } else {
            break;
        }
    }

    Ok(Json(BlockersDiagnostics {
        is_blocked,
        blocker_chain,
    }))
}

/// GET /issues/:id/diagnostics/wakes
async fn get_wakes_diagnostics(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<WakesDiagnostics>, StatusCode> {
    // In production: query heartbeat/wake system
    Ok(Json(WakesDiagnostics {
        pending_wakes: 0,
        active_wakes: vec![],
    }))
}

/// GET /issues/:id/diagnostics/subtree
async fn get_subtree_diagnostics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SubtreeDiagnostics>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = get_company_id_for_issue(&state, id).await?;

    // Verify issue exists
    let _ = service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // List direct children
    let filter = services::IssueQueryFilter {
        status: None,
        priority: None,
        assignee_agent_id: None,
        assignee_user_id: None,
        project_id: None,
        goal_id: None,
        parent_id: Some(id),
        search_query: None,
    };

    let pagination = services::Pagination {
        limit: 1000,
        offset: 0,
        cursor: None,
    };

    let children = service
        .list(company_id, &filter, &pagination)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut status_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for child in &children {
        let status_str = format!("{:?}", child.status);
        *status_counts.entry(status_str).or_insert(0) += 1;
    }

    let status_breakdown: Vec<StatusCount> = status_counts
        .into_iter()
        .map(|(status, count)| StatusCount { status, count })
        .collect();

    Ok(Json(SubtreeDiagnostics {
        issue_id: id,
        total_descendants: children.len() as i64,
        status_breakdown,
    }))
}

/// Create issue diagnostics routes
pub fn issue_diagnostics_routes() -> Router<AppState> {
    Router::new()
        .route("/issues/:id/diagnostics/blockers", get(get_blockers_diagnostics))
        .route("/issues/:id/diagnostics/wakes", get(get_wakes_diagnostics))
        .route("/issues/:id/diagnostics/subtree", get(get_subtree_diagnostics))
}
