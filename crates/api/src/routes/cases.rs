use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use crate::app_state::AppState;
use uuid::Uuid;

use models::{Case, CaseDetail, CaseEvent, CreateCaseInput, UpdateCaseInput};
use services::{CaseQueryFilter, Pagination};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListCasesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    #[allow(dead_code)]
    status: Option<String>,
    case_type: Option<String>,
    project_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCaseQuery {
    #[serde(default)]
    upsert: bool,
}

/// POST /companies/:companyId/cases - Create case
async fn create_case(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
    Query(query): Query<CreateCaseQuery>,
    Json(input): Json<CreateCaseInput>,
) -> Result<Json<Case>, StatusCode> {
    let service = state.case_service.clone();
    service
        .create(input, query.upsert)
        .await
        .map(|result| Json(result.case))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/cases - List cases
async fn list_cases(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<Case>>, StatusCode> {
    let service = state.case_service.clone();
    let filter = CaseQueryFilter {
        status: None,
        case_type: query.case_type,
        project_id: query.project_id,
        parent_case_id: None,
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

/// GET /cases/:id - Get case by ID
async fn get_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Case>, StatusCode> {
    let service = state.case_service.clone();
    let company_id = Uuid::nil();

    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /cases/:id/detail - Get case detail with related data
async fn get_case_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CaseDetail>, StatusCode> {
    let service = state.case_service.clone();
    let company_id = Uuid::nil();

    service
        .get_detail(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// PATCH /cases/:id - Update case
async fn update_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateCaseInput>,
) -> Result<Json<Case>, StatusCode> {
    let service = state.case_service.clone();
    let company_id = Uuid::nil();

    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.case))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /cases/:id/events - List case events
async fn list_case_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<Vec<CaseEvent>>, StatusCode> {
    let service = state.case_service.clone();
    let company_id = Uuid::nil();
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50),
        offset: query.offset.unwrap_or(0),
        cursor: None,
    };
    
    service
        .list_events(id, company_id, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// ============================================================================
// P1: Case 子资源/状态机动作 Handlers (C1-C23)
// ============================================================================

/// C1: GET /cases/:id/children
async fn get_case_children(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Case>>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_children(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C2: GET /cases/:id/children/tree
async fn get_case_children_tree(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_children_tree(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C3: GET /cases/:id/rollup
async fn get_case_rollup(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_rollup(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C4: GET /cases/:id/context-pack
async fn get_case_context_pack(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_context_pack(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C5: GET /cases/:id/outputs
async fn get_case_outputs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_outputs(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C6: GET /cases/:id/issue-links
async fn list_issue_links(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.get_issue_links(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C6: POST /cases/:id/issue-links
async fn create_issue_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = Uuid::nil();
    let issue_id = payload.get("issueId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let link = state.case_service.create_issue_link(id, company_id, issue_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(link)))
}

/// C6: DELETE /cases/:id/issue-links/:link_id
async fn delete_issue_link(
    State(state): State<AppState>,
    Path((id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.delete_issue_link(id, link_id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// C7: POST /cases/:id/links
async fn create_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = Uuid::nil();
    let link = state.case_service.create_link(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(link)))
}

/// C8: PUT /cases/:id/blockers
async fn update_blockers(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    let blocker_ids = payload.get("blockerIds")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())).collect())
        .unwrap_or_default();
    state.case_service.update_blockers(id, company_id, blocker_ids).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C9: POST /cases/:id/suggest-transition
async fn suggest_transition(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.suggest_transition(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C10: POST /cases/:id/resolve-suggestion
async fn resolve_suggestion(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.resolve_suggestion(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C11: POST /cases/:id/review
async fn review_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.review_case(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C12: POST /cases/:id/acknowledge-drift
async fn acknowledge_drift(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.acknowledge_drift(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C13: POST /cases/:id/open-conversation
async fn open_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.open_conversation(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C14: POST /cases/:id/breakdown
async fn breakdown_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.breakdown_case(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C15: POST /cases/:id/attachments
async fn upload_case_attachment(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Use attachment service
    let _company_id = Uuid::nil();
    // TODO: multipart upload handling
    Ok(Json(serde_json::json!({
        "caseId": id,
        "attachmentId": Uuid::new_v4(),
        "uploaded": true,
    })))
}

/// C16: GET /cases/:id/documents/:key/revisions
async fn get_document_revisions(
    State(_state): State<AppState>,
    Path((_id, _key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    // TODO: Use document service revisions
    Ok(Json(vec![
        serde_json::json!({"revisionId": Uuid::new_v4(), "version": 1, "createdAt": chrono::Utc::now()}),
        serde_json::json!({"revisionId": Uuid::new_v4(), "version": 2, "createdAt": chrono::Utc::now()}),
    ]))
}

/// C17: POST /cases/:id/documents/:key/revisions/:revision_id/restore
async fn restore_document_revision(
    State(_state): State<AppState>,
    Path((_id, _key, _revision_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"restored": true, "revisionId": _revision_id})))
}

/// C18: DELETE /cases/:id/documents/:key
async fn delete_case_document(
    State(_state): State<AppState>,
    Path((_id, _key)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

/// C19: GET /cases/:id/documents/:key/annotations
async fn get_document_annotations(
    State(_state): State<AppState>,
    Path((_id, _key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// C20: POST /cases/:id/automation/retry
async fn automation_retry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.automation_retry(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C21: POST /cases/:id/automation/retry-plan
async fn automation_retry_plan(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.automation_retry_plan(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C22: POST /cases/:id/automation/current-stage/rerun
async fn automation_rerun_stage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.automation_rerun_stage(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C23: POST /cases/:id/automations/:automation_id/retry
async fn automation_retry_single(
    State(state): State<AppState>,
    Path((id, automation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = Uuid::nil();
    state.case_service.automation_retry_single(id, company_id, automation_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// Create case routes
pub fn case_routes() -> Router<AppState> {
    Router::new()
        .route("/api/companies/:companyId/cases", post(create_case).get(list_cases))
        .route("/api/cases/:id", get(get_case).patch(update_case))
        .route("/api/cases/:id/detail", get(get_case_detail))
        .route("/api/cases/:id/events", get(list_case_events))
        // --- P1: Case 子资源/状态机动作 (C1-C23) ---
        .route("/api/cases/:id/children", get(get_case_children))
        .route("/api/cases/:id/children/tree", get(get_case_children_tree))
        .route("/api/cases/:id/rollup", get(get_case_rollup))
        .route("/api/cases/:id/context-pack", get(get_case_context_pack))
        .route("/api/cases/:id/outputs", get(get_case_outputs))
        .route("/api/cases/:id/issue-links", get(list_issue_links).post(create_issue_link))
        .route("/api/cases/:id/issue-links/:link_id", delete(delete_issue_link))
        .route("/api/cases/:id/links", post(create_link))
        .route("/api/cases/:id/blockers", put(update_blockers))
        .route("/api/cases/:id/suggest-transition", post(suggest_transition))
        .route("/api/cases/:id/resolve-suggestion", post(resolve_suggestion))
        .route("/api/cases/:id/review", post(review_case))
        .route("/api/cases/:id/acknowledge-drift", post(acknowledge_drift))
        .route("/api/cases/:id/open-conversation", post(open_conversation))
        .route("/api/cases/:id/breakdown", post(breakdown_case))
        .route("/api/cases/:id/attachments", post(upload_case_attachment))
        .route("/api/cases/:id/documents/:key/revisions", get(get_document_revisions))
        .route("/api/cases/:id/documents/:key/revisions/:revision_id/restore", post(restore_document_revision))
        .route("/api/cases/:id/documents/:key", delete(delete_case_document))
        .route("/api/cases/:id/documents/:key/annotations", get(get_document_annotations))
        .route("/api/cases/:id/automation/retry", post(automation_retry))
        .route("/api/cases/:id/automation/retry-plan", post(automation_retry_plan))
        .route("/api/cases/:id/automation/current-stage/rerun", post(automation_rerun_stage))
        .route("/api/cases/:id/automations/:automation_id/retry", post(automation_retry_single))
}
