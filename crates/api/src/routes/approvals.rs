//! Approval routes — 整域新增 (AP1-AP10)
//!
//! 对应 FEATURE_GAP_TASKS.md §3.1 Approvals

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use services::auth::AuthorizationActor;

pub fn approval_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/approvals",
            get(list_approvals).post(create_approval),
        )
        .route("/approvals/:id", get(get_approval))
        .route("/approvals/:id/issues", get(get_approval_issues))
        .route("/approvals/:id/approve", post(approve_approval))
        .route("/approvals/:id/reject", post(reject_approval))
        .route(
            "/approvals/:id/request-revision",
            post(request_approval_revision),
        )
        .route("/approvals/:id/resubmit", post(resubmit_approval))
        .route(
            "/approvals/:id/comments",
            get(list_approval_comments).post(add_approval_comment),
        )
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    // Use the approval_service from the state
    let approvals = state
        .approval_service
        .list_by_company(company_id, None)
        .await
        .map(|a| {
            a.into_iter()
                .map(|app| serde_json::to_value(app).unwrap_or_default())
                .collect()
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(approvals))
}

/// AP2: GET /approvals/:id
async fn get_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let approval = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, approval.approval.company_id, true)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP3: POST /companies/:company_id/approvals
async fn create_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateApprovalBody>,
) -> Result<impl IntoResponse, StatusCode> {
    use models::ApprovalType;
    use services::approval_service::CreateApprovalInput;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let current_user_id = actor_user_id(&actor)?;
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
    let approval = state
        .approval_service
        .create(input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(approval).unwrap_or_default()),
    ))
}

/// AP4: GET /approvals/:id/issues
async fn get_approval_issues(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let issues = state
        .approval_service
        .get_by_issue_id(id)
        .await
        .map(|a| {
            a.into_iter()
                .map(|app| serde_json::to_value(app).unwrap_or_default())
                .collect()
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(issues))
}

/// AP5: POST /approvals/:id/approve
async fn approve_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    let current_user_id = actor_user_id(&actor)?;
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::Approve,
        decided_by_user_id: current_user_id,
        decision_note: body
            .get("decisionNote")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    let approval = state
        .approval_service
        .review(input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP6: POST /approvals/:id/reject
async fn reject_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    let current_user_id = actor_user_id(&actor)?;
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::Reject,
        decided_by_user_id: current_user_id,
        decision_note: body
            .get("decisionNote")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    let approval = state
        .approval_service
        .review(input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP7: POST /approvals/:id/request-revision
async fn request_approval_revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use services::approval_service::*;
    let current_user_id = actor_user_id(&actor)?;
    let input = ReviewApprovalInput {
        approval_id: id,
        decision: ApprovalDecision::RequestRevision,
        decided_by_user_id: current_user_id,
        decision_note: body
            .get("decisionNote")
            .and_then(|v| v.as_str())
            .map(String::from),
    };
    let approval = state
        .approval_service
        .review(input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(approval).unwrap_or_default()))
}

/// AP8: POST /approvals/:id/resubmit
async fn resubmit_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let approval = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, approval.approval.company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE approvals SET status = 'pending', decision_note = NULL, decided_by_user_id = NULL, decided_at = NULL, updated_at = NOW() WHERE id = $1 AND status = 'revision_requested'")
        .bind(id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(
        serde_json::to_value(updated.approval).unwrap_or_default(),
    ))
}

/// AP9: GET /approvals/:id/comments
async fn list_approval_comments(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let approval = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, approval.approval.company_id, true)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let comments = sqlx::query_as::<_, (Uuid, Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, approval_id, author_user_id, body, created_at FROM approval_comments WHERE approval_id = $1 ORDER BY created_at ASC")
        .bind(id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(comments.into_iter().map(|(id, approval_id, author_user_id, body, created_at)| serde_json::json!({"id": id, "approvalId": approval_id, "authorUserId": author_user_id, "body": body, "createdAt": created_at})).collect()))
}

/// AP10: POST /approvals/:id/comments
async fn add_approval_comment(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let approval = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, approval.approval.company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let author_user_id = actor_user_id(&actor)?;
    let body_text = body
        .get("body")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let comment = sqlx::query_as::<_, (Uuid, Uuid, Uuid, String, chrono::DateTime<chrono::Utc>)>(
        "INSERT INTO approval_comments (approval_id, author_user_id, body) VALUES ($1, $2, $3) RETURNING id, approval_id, author_user_id, body, created_at")
        .bind(id).bind(author_user_id).bind(body_text).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::CREATED,
        Json(
            serde_json::json!({"id": comment.0, "approvalId": comment.1, "authorUserId": comment.2, "body": comment.3, "createdAt": comment.4}),
        ),
    ))
}

fn actor_user_id(actor: &AuthorizationActor) -> Result<Uuid, StatusCode> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        AuthorizationActor::Agent { .. } | AuthorizationActor::None => Err(StatusCode::FORBIDDEN),
    }
}
