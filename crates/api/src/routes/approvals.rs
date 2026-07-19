//! Approval routes — 整域新增 (AP1-AP10)
//!
//! 对应 FEATURE_GAP_TASKS.md §3.1 Approvals

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;

pub fn approval_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:company_id/approvals", get(list_approvals).post(create_approval))
        .route("/approvals/:id", get(get_approval))
        .route("/approvals/:id/issues", get(get_approval_issues))
        .route("/approvals/:id/approve", post(approve_approval))
        .route("/approvals/:id/reject", post(reject_approval))
        .route("/approvals/:id/request-revision", post(request_approval_revision))
        .route("/approvals/:id/resubmit", post(resubmit_approval))
        .route("/approvals/:id/comments", get(list_approval_comments).post(add_approval_comment))
}

#[derive(Debug, Deserialize)]
struct CreateApprovalBody {
    pub issue_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    #[allow(dead_code)]
    pub required_approvers: Option<Vec<Uuid>>,
}

/// AP1: GET /companies/:company_id/approvals
async fn list_approvals(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    // Use the approval_service from the state
    let approvals = state.approval_service.list_by_company(company_id, None).await
        .map(|a| a.into_iter().map(|app| serde_json::to_value(app).unwrap_or_default()).collect())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(approvals))
}

/// AP2: GET /approvals/:id
async fn get_approval(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let approval = state.approval_service.get_by_id(id).await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP3: POST /companies/:company_id/approvals
async fn create_approval(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateApprovalBody>,
) -> Result<impl IntoResponse, StatusCode> {
    use models::ApprovalType;
    use services::approval_service::CreateApprovalInput;
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();
    let input = CreateApprovalInput {
        company_id,
        approval_type: ApprovalType::CreateResource,
        requested_by_agent_id: None,
        requested_by_user_id: Some(current_user_id),
        payload: serde_json::json!({
            "title": body.title,
            "description": body.description,
        }),
        linked_issue_ids: body.issue_id.map(|id| vec![id]).unwrap_or_default(),
    };
    let approval = state.approval_service.create(input).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(approval).unwrap_or_default())))
}

/// AP4: GET /approvals/:id/issues
async fn get_approval_issues(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let issues = state.approval_service.get_by_issue_id(id).await
        .map(|a| a.into_iter().map(|app| serde_json::to_value(app).unwrap_or_default()).collect())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(issues))
}

/// AP5: POST /approvals/:id/approve
async fn approve_approval(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::Approve,
        decided_by_user_id: current_user_id,
        decision_note: body.get("decisionNote").and_then(|v| v.as_str()).map(String::from),
    };
    let approval = state.approval_service.review(input).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP6: POST /approvals/:id/reject
async fn reject_approval(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::Reject,
        decided_by_user_id: current_user_id,
        decision_note: body.get("decisionNote").and_then(|v| v.as_str()).map(String::from),
    };
    let approval = state.approval_service.review(input).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP7: POST /approvals/:id/request-revision
async fn request_approval_revision(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    // TODO: 从 AuthorizationActor 提取当前用户 ID（需要路由挂载 AuthMiddleware）
    let current_user_id = Uuid::nil();
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::RequestRevision,
        decided_by_user_id: current_user_id,
        decision_note: body.get("decisionNote").and_then(|v| v.as_str()).map(String::from),
    };
    let approval = state.approval_service.review(input).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP8: POST /approvals/:id/resubmit
async fn resubmit_approval(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Re-open the approval for review
    // In production: this would create a new approval version or reset status to pending
    Ok(Json(serde_json::json!({
        "approvalId": id,
        "resubmitted": true,
        "status": "pending",
    })))
}

/// AP9: GET /approvals/:id/comments
async fn list_approval_comments(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// AP10: POST /approvals/:id/comments
async fn add_approval_comment(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": Uuid::new_v4(),
        "created": true,
    }))))
}
