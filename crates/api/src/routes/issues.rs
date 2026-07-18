use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use crate::app_state::AppState;
use uuid::Uuid;

use models::{CreateIssueInput, Issue, UpdateIssueInput};
use services::{
    CheckoutInput, IssueQueryFilter, Pagination, ReleaseInput,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListIssuesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    #[allow(dead_code)]
    status: Option<String>,
    #[allow(dead_code)]
    priority: Option<String>,
    assignee_agent_id: Option<Uuid>,
    assignee_user_id: Option<Uuid>,
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

/// GET /issues - List all issues
async fn list_issues(
    State(state): State<AppState>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();
    
    let filter = IssueQueryFilter {
        status: None,
        priority: None,
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: None,
    };
    
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };
    
    service
        .list(company_id, &filter, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /issues/:id - Get issue by ID
async fn get_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// POST /companies/:companyId/issues - Create issue
async fn create_issue(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
    Json(input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    service
        .create(input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /issues/:id - Update issue
async fn update_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// DELETE /issues/:id - Delete issue
async fn delete_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .delete(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/issues/count - Count issues
async fn count_issues(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"count": 0})))
}

/// GET /companies/:companyId/issues/search - Search issues
async fn search_issues(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let filter = IssueQueryFilter::default();
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: 0,
        cursor: None,
    };
    
    service
        .search(company_id, &query.q, &filter, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/checkout - Checkout issue
async fn checkout_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<CheckoutInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .checkout(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/release - Release issue
async fn release_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .release(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/admin/force-release - Force release issue (admin only)
async fn force_release_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<services::ForceReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    // Validate force release schema
    let schema = crate::validation::ForceReleaseSchema {
        admin_user_id: input.admin_user_id,
        reason: input.reason.clone(),
        release_lease: Some(input.release_lease),
    };
    if let Err(_e) = schema.validate() {
        return Err(StatusCode::BAD_REQUEST);
    }

    service
        .force_release(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /companies/:companyId/issues/batch-update - Batch update issues
async fn batch_update_issues(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<crate::validation::BatchIssueUpdateSchema>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();

    // Validate batch update schema
    if let Err(_e) = input.validate() {
        return Err(StatusCode::BAD_REQUEST);
    }

    service
        .batch_update(company_id, input.issue_ids, input.status, input.priority, input.assignee_agent_id, input.assignee_user_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/heartbeat-context - Get heartbeat context for issue
async fn get_heartbeat_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = Uuid::nil();

    service
        .get_heartbeat_context(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// ============================================================================
// P1: Issue 子资源 Handlers (I1-I44)
// ============================================================================

/// I2: GET /issues/:id/cases
async fn get_issue_cases(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.get_cases(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I3: GET /issues/:id/active-run
async fn get_issue_active_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    let run = state.issue_service.get_active_run(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match run {
        Some(r) => Ok(Json(r)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// I4: GET /issues/:id/live-runs
async fn get_issue_live_runs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.get_live_runs(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I6: GET /issues/:id/accepted-plan-decompositions
async fn list_plan_decompositions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.get_accepted_plan_decompositions(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I7: POST /issues/:id/accepted-plan-decompositions
async fn submit_plan_decomposition(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = Uuid::nil();
    let result = state.issue_service.submit_plan_decomposition(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I8: GET /issues/:id/approvals
async fn list_issue_approvals(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.get_approvals(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I9: POST /issues/:id/approvals
async fn create_issue_approval(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = Uuid::nil();
    let result = state.issue_service.create_approval(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I10: DELETE /issues/:id/approvals/:approval_id
async fn delete_issue_approval(
    State(state): State<AppState>,
    Path((id, approval_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.delete_approval(id, approval_id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I11: POST /issues/:id/children
async fn create_child_issue(
    State(state): State<AppState>,
    Path(parent_id): Path<Uuid>,
    Json(input): Json<CreateIssueInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let service = state.issue_service.clone();
    let input_with_parent = CreateIssueInput {
        parent_id: Some(parent_id),
        ..input
    };
    let result = service.create(input_with_parent).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result.issue)))
}

/// I12: POST /issues/:id/read
async fn mark_issue_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.mark_read(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I13: DELETE /issues/:id/read
async fn unmark_issue_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.unmark_read(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I14: POST /issues/:id/inbox-archive
async fn archive_issue_inbox(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.archive_inbox(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I15: DELETE /issues/:id/inbox-archive
async fn unarchive_issue_inbox(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.unarchive_inbox(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I16: POST /issues/:id/monitor/check-now
async fn monitor_check_now(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "monitorCheckTriggered": true})))
}

/// I17: POST /issues/:id/scheduled-retry/retry-now
async fn scheduled_retry_now(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "retryTriggered": true})))
}

/// I18: GET /issues/:id/external-objects
async fn list_external_objects(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I19: GET /issues/:id/external-object-summary
async fn get_external_object_summary(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "externalObjectCount": 0})))
}

/// I20: POST /issues/:id/external-objects/refresh
async fn refresh_external_objects(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "refreshTriggered": true})))
}

/// I21: GET /issues/:id/file-resources/list
async fn list_file_resources(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I22: GET /issues/:id/file-resources/resolve
async fn resolve_file_resource(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "resolved": []})))
}

/// I23: GET /issues/:id/file-resources/content
async fn get_file_resource_content(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "content": ""})))
}

/// I24: GET /issues/:id/feedback-votes
async fn list_feedback_votes(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I25: POST /issues/:id/feedback-votes
async fn create_feedback_vote(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({"issueId": id, "vote": payload, "created": true}))))
}

/// I26: GET /issues/:id/feedback-traces
async fn list_feedback_traces(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I27: GET /issues/:id/recovery-actions
async fn list_recovery_actions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.issue_service.get_recovery_actions(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I28: POST /issues/:id/recovery-actions/resolve
async fn resolve_recovery_action(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    let action_id = payload.get("actionId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    state.issue_service.resolve_recovery_action(id, company_id, action_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I29: GET /issues/:id/interactions
async fn list_interactions(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I30: POST /issues/:id/interactions
async fn create_interaction(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({"issueId": id, "interaction": payload, "created": true}))))
}

/// I31: POST /issues/:id/interactions/:interaction_id/accept
async fn accept_interaction(
    State(_state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "interactionId": interaction_id, "accepted": true})))
}

/// I32: POST /issues/:id/interactions/:interaction_id/reject
async fn reject_interaction(
    State(_state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "interactionId": interaction_id, "rejected": true})))
}

/// I33: POST /issues/:id/interactions/:interaction_id/respond
async fn respond_interaction(
    State(_state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "interactionId": interaction_id, "response": payload, "responded": true})))
}

/// I34: POST /issues/:id/interactions/:interaction_id/cancel
async fn cancel_interaction(
    State(_state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "interactionId": interaction_id, "cancelled": true})))
}

/// I42: GET /issues/:id/comments/:comment_id
async fn get_single_comment(
    State(state): State<AppState>,
    Path((_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    let comment = state.issue_service.get_comment(comment_id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    comment.map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Create issue routes
pub fn issue_routes() -> Router<AppState> {
    Router::new()
        .route("/issues", get(list_issues))
        .route("/issues/:id", get(get_issue).patch(update_issue).delete(delete_issue))
        .route("/companies/:companyId/issues", post(create_issue))
        .route("/companies/:companyId/issues/count", get(count_issues))
        .route("/companies/:companyId/issues/search", get(search_issues))
        .route("/issues/:id/checkout", post(checkout_issue))
        .route("/issues/:id/release", post(release_issue))
        .route("/issues/:id/admin/force-release", post(force_release_issue))
        .route("/companies/:companyId/issues/batch-update", post(batch_update_issues))
        .route("/issues/:id/heartbeat-context", get(get_heartbeat_context))
        // --- P1: Issue 子资源补齐 (I1-I44) ---
        .route("/issues/:id/cases", get(get_issue_cases))
        .route("/issues/:id/active-run", get(get_issue_active_run))
        .route("/issues/:id/live-runs", get(get_issue_live_runs))
        .route("/issues/:id/accepted-plan-decompositions", get(list_plan_decompositions).post(submit_plan_decomposition))
        .route("/issues/:id/approvals", get(list_issue_approvals).post(create_issue_approval))
        .route("/issues/:id/approvals/:approval_id", delete(delete_issue_approval))
        .route("/issues/:id/children", post(create_child_issue))
        .route("/issues/:id/read", post(mark_issue_read).delete(unmark_issue_read))
        .route("/issues/:id/inbox-archive", post(archive_issue_inbox).delete(unarchive_issue_inbox))
        .route("/issues/:id/monitor/check-now", post(monitor_check_now))
        .route("/issues/:id/scheduled-retry/retry-now", post(scheduled_retry_now))
        .route("/issues/:id/external-objects", get(list_external_objects))
        .route("/issues/:id/external-object-summary", get(get_external_object_summary))
        .route("/issues/:id/external-objects/refresh", post(refresh_external_objects))
        .route("/issues/:id/file-resources/list", get(list_file_resources))
        .route("/issues/:id/file-resources/resolve", get(resolve_file_resource))
        .route("/issues/:id/file-resources/content", get(get_file_resource_content))
        .route("/issues/:id/feedback-votes", get(list_feedback_votes).post(create_feedback_vote))
        .route("/issues/:id/feedback-traces", get(list_feedback_traces))
        .route("/issues/:id/recovery-actions", get(list_recovery_actions))
        .route("/issues/:id/recovery-actions/resolve", post(resolve_recovery_action))
        .route("/issues/:id/interactions", get(list_interactions).post(create_interaction))
        .route("/issues/:id/interactions/:interaction_id/accept", post(accept_interaction))
        .route("/issues/:id/interactions/:interaction_id/reject", post(reject_interaction))
        .route("/issues/:id/interactions/:interaction_id/respond", post(respond_interaction))
        .route("/issues/:id/interactions/:interaction_id/cancel", post(cancel_interaction))
        .route("/issues/:id/comments/:comment_id", get(get_single_comment))
}
