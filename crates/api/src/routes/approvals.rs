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
use axum::extract::Query;
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
struct ListApprovalsQuery {
    status: Option<models::ApprovalStatus>,
}

/// AP1: GET /companies/:company_id/approvals
async fn list_approvals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListApprovalsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, true)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    // Use the approval_service from the state
    let approvals = state
        .approval_service
        .list_by_company(company_id, query.status)
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
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    use models::ApprovalType;
    use services::approval_service::CreateApprovalInput;
    crate::routes::assert_company_access(&actor, company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    let approval_type = body
        .get("type")
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)
        .and_then(|value| serde_json::from_value::<ApprovalType>(value).map_err(|_| StatusCode::BAD_REQUEST))?;
    let payload = body
        .get("payload")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)?;
    let (requested_by_agent_id, requested_by_user_id) = match actor {
        AuthorizationActor::Agent { agent_id, .. } => (Some(agent_id), None),
        AuthorizationActor::Board { user_id, .. } => (
            body.get("requestedByAgentId")
                .and_then(|value| value.as_str())
                .and_then(|value| Uuid::parse_str(value).ok()),
            Some(user_id),
        ),
        AuthorizationActor::None => return Err(StatusCode::FORBIDDEN),
    };
    let input = CreateApprovalInput {
        company_id,
        approval_type,
        requested_by_agent_id,
        requested_by_user_id,
        payload,
        linked_issue_ids: body
            .get("issueIds")
            .and_then(|value| value.as_array())
            .map(|values| values.iter().filter_map(|value| value.as_str()).filter_map(|value| Uuid::parse_str(value).ok()).collect())
            .unwrap_or_default(),
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
    let issues = sqlx::query_as::<_, models::Issue>(
        "SELECT i.* FROM issues i JOIN issue_approvals ia ON ia.issue_id=i.id JOIN approvals a ON a.id=ia.approval_id WHERE ia.approval_id=$1 AND i.company_id=a.company_id ORDER BY i.created_at ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .into_iter()
    .map(|issue| serde_json::to_value(issue).unwrap_or_default())
    .collect();
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
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let approval = state
        .approval_service
        .get_by_id(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    crate::routes::assert_company_access(&actor, approval.approval.company_id, false)
        .map_err(|_| StatusCode::FORBIDDEN)?;
    sqlx::query("UPDATE approvals SET status = 'pending', payload = CASE WHEN $2::jsonb = '{}'::jsonb THEN payload ELSE $2::jsonb END, decision_note = NULL, decided_by_user_id = NULL, decided_at = NULL, updated_at = NOW() WHERE id = $1 AND status = 'revision_requested'")
        .bind(id).bind(body.get("payload").cloned().unwrap_or_else(|| body.clone())).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
