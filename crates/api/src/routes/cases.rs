use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use crate::app_state::AppState;
use crate::errors::AppError;
use uuid::Uuid;

use models::{Case, CaseDetail, CaseEvent, CreateCaseInput, PipelineCase, UpdateCaseInput};
use services::{AdvanceCaseInput, CaseQueryFilter, Pagination};

/// Helper: 通过 case_id 查询 company_id
async fn get_company_id_for_case(state: &AppState, case_id: Uuid) -> Result<Uuid, StatusCode> {
    sqlx::query_scalar("SELECT company_id FROM cases WHERE id = $1")
        .bind(case_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

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
    let company_id = get_company_id_for_case(&state, id).await?;

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
    let company_id = get_company_id_for_case(&state, id).await?;

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
    let company_id = get_company_id_for_case(&state, id).await?;

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
    let company_id = get_company_id_for_case(&state, id).await?;
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
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_children(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C2: GET /cases/:id/children/tree
async fn get_case_children_tree(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_children_tree(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C3: GET /cases/:id/rollup
async fn get_case_rollup(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_rollup(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C4: GET /cases/:id/context-pack
async fn get_case_context_pack(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_context_pack(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C5: GET /cases/:id/outputs
async fn get_case_outputs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_outputs(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C6: GET /cases/:id/issue-links
async fn list_issue_links(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.get_issue_links(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C6: POST /cases/:id/issue-links
async fn create_issue_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
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
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.delete_issue_link(id, link_id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// C7: POST /cases/:id/links
async fn create_link(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    let link = state.case_service.create_link(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(link)))
}

/// C8: PUT /cases/:id/blockers
async fn update_blockers(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
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
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.suggest_transition(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C10: POST /cases/:id/resolve-suggestion
async fn resolve_suggestion(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.resolve_suggestion(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C11: POST /cases/:id/review
async fn review_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.review_case(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C12: POST /cases/:id/acknowledge-drift
async fn acknowledge_drift(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.acknowledge_drift(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C13: POST /cases/:id/open-conversation
async fn open_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.open_conversation(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C14: POST /cases/:id/breakdown
async fn breakdown_case(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.breakdown_case(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C15: POST /cases/:id/attachments
async fn upload_case_attachment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<models::issue_auxiliary::UploadAttachmentInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.attachment_service
        .upload_attachment("case", id, company_id, input)
        .await
        .map(|attachment| Json(serde_json::to_value(attachment).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn case_document_id(state: &AppState, case_id: Uuid, key: &str) -> Result<(Uuid, Uuid), StatusCode> {
    sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT d.id, cd.company_id FROM case_documents cd JOIN documents d ON d.id = cd.document_id WHERE cd.case_id=$1 AND cd.key=$2")
        .bind(case_id).bind(key).fetch_optional(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

/// C16: GET /cases/:id/documents/:key — Get case document content
async fn get_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let row = sqlx::query("SELECT d.id, cd.company_id, cd.key, d.content, d.content_type, d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, d.created_at, d.updated_at FROM case_documents cd JOIN documents d ON d.id=cd.document_id WHERE cd.case_id=$1 AND cd.key=$2")
        .bind(id).bind(&key).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({"id": row.get::<Uuid,_>("id"), "caseId": id, "companyId": row.get::<Uuid,_>("company_id"), "key": row.get::<String,_>("key"), "content": row.get::<String,_>("content"), "contentType": row.get::<Option<String>,_>("content_type").unwrap_or_else(|| "text/markdown".into()), "lockedByType": row.get::<Option<String>,_>("locked_by_type"), "lockedById": row.get::<Option<Uuid>,_>("locked_by_id"), "lockedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("locked_at"), "lockedRunId": row.get::<Option<Uuid>,_>("locked_run_id"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"), "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})))
}

/// C17: POST /cases/:id/documents/:key — Create case document
async fn create_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    let content = payload.get("content").and_then(|v| v.as_str()).unwrap_or_default();
    let content_type = payload.get("contentType").and_then(|v| v.as_str()).unwrap_or("text/markdown");
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let document_id: Uuid = sqlx::query_scalar("INSERT INTO documents (company_id, content, content_type) VALUES ($1,$2,$3) RETURNING id").bind(company_id).bind(content).bind(content_type).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO case_documents (company_id, case_id, document_id, key) VALUES ($1,$2,$3,$4)").bind(company_id).bind(id).bind(document_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
    sqlx::query("INSERT INTO document_revisions (document_id, revision_number, content) VALUES ($1,1,$2)").bind(document_id).bind(content).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": document_id, "caseId": id, "key": key, "content": content, "contentType": content_type, "revisionNumber": 1}))))
}

/// C18: PUT /cases/:id/documents/:key — Update case document
async fn update_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let content = payload.get("content").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1").bind(document_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET content=$2, content_type=COALESCE($3,content_type), updated_at=NOW() WHERE id=$1").bind(document_id).bind(content).bind(payload.get("contentType").and_then(|v| v.as_str())).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO document_revisions (document_id,revision_number,content) VALUES ($1,$2,$3)").bind(document_id).bind(revision).bind(content).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"id": document_id, "caseId": id, "key": key, "content": content, "revisionNumber": revision})))
}

/// C19: POST /cases/:id/documents/:key/lock — Lock case document
async fn lock_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    sqlx::query("UPDATE documents SET locked_by_type=$2, locked_by_id=$3, locked_at=NOW(), locked_run_id=$4 WHERE id=$1 AND locked_at IS NULL")
        .bind(document_id).bind(payload.get("actorType").and_then(|v|v.as_str()).unwrap_or("user"))
        .bind(payload.get("actorId").and_then(|v|v.as_str()).and_then(|v|Uuid::parse_str(v).ok()))
        .bind(payload.get("runId").and_then(|v|v.as_str()).and_then(|v|Uuid::parse_str(v).ok())).execute(&state.pool).await.map_err(|_| StatusCode::CONFLICT)?;
    Ok(Json(serde_json::json!({"caseId": id, "key": key, "locked": true, "lockedBy": payload})))
}

/// C20: POST /cases/:id/documents/:key/unlock — Unlock case document
async fn unlock_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    sqlx::query("UPDATE documents SET locked_by_type=NULL, locked_by_id=NULL, locked_at=NULL, locked_run_id=NULL WHERE id=$1").bind(document_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"caseId": id, "key": key, "unlocked": true})))
}

/// C21: GET /cases/:id/documents/:key/revisions
async fn get_document_revisions(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let rows = sqlx::query("SELECT id,revision_number,content,created_at FROM document_revisions WHERE document_id=$1 ORDER BY revision_number DESC").bind(document_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({"id":r.get::<Uuid,_>("id"),"revisionId":r.get::<Uuid,_>("id"),"revisionNumber":r.get::<i32,_>("revision_number"),"version":r.get::<i32,_>("revision_number"),"content":r.get::<String,_>("content"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")})).collect()))
}

/// C22: POST /cases/:id/documents/:key/revisions/:revision_id/restore
async fn restore_document_revision(
    State(state): State<AppState>,
    Path((id, key, revision_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let content: String = sqlx::query_scalar("SELECT content FROM document_revisions WHERE id=$1 AND document_id=$2").bind(revision_id).bind(document_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1").bind(document_id).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET content=$2,updated_at=NOW() WHERE id=$1").bind(document_id).bind(&content).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO document_revisions(document_id,revision_number,content) VALUES($1,$2,$3)").bind(document_id).bind(revision).bind(&content).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"restored":true,"revisionId":revision_id,"revisionNumber":revision,"content":content})))
}

/// C23: DELETE /cases/:id/documents/:key
async fn delete_case_document(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    let (_, company_id) = case_document_id(&state, id, &key).await?;
    sqlx::query("DELETE FROM case_documents WHERE case_id=$1 AND key=$2 AND company_id=$3").bind(id).bind(key).bind(company_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// C24: GET /cases/:id/documents/:key/annotations
async fn get_document_annotations(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let rows = sqlx::query("SELECT id, status, anchor_state, selected_text, anchor_selector, created_at, updated_at FROM document_annotation_threads WHERE case_id=$1 AND document_id=$2 AND document_key=$3 ORDER BY updated_at DESC")
        .bind(id).bind(document_id).bind(&key).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let thread_id: Uuid = row.get("id");
        let comments = sqlx::query("SELECT id, body, author_type, author_agent_id, author_user_id, created_at, updated_at FROM document_annotation_comments WHERE thread_id=$1 ORDER BY created_at ASC")
            .bind(thread_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        result.push(serde_json::json!({"id":thread_id,"threadId":thread_id,"caseId":id,"documentKey":key,"status":row.get::<String,_>("status"),"anchorState":row.get::<String,_>("anchor_state"),"selectedText":row.get::<String,_>("selected_text"),"anchorSelector":row.get::<serde_json::Value,_>("anchor_selector"),"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),"comments":comments.into_iter().map(|c|serde_json::json!({"id":c.get::<Uuid,_>("id"),"body":c.get::<String,_>("body"),"authorType":c.get::<String,_>("author_type"),"authorAgentId":c.get::<Option<Uuid>,_>("author_agent_id"),"authorUserId":c.get::<Option<String>,_>("author_user_id"),"createdAt":c.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":c.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})).collect::<Vec<_>>() }));
    }
    Ok(Json(result))
}

/// C25: GET /cases/:id/documents/:key/annotations/:thread_id
async fn get_document_annotation_thread(
    State(state): State<AppState>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let row = sqlx::query("SELECT id,status,anchor_state,selected_text,anchor_selector,created_at,updated_at FROM document_annotation_threads WHERE id=$1 AND case_id=$2 AND document_id=$3 AND document_key=$4")
        .bind(thread_id).bind(id).bind(document_id).bind(&key).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let comments = sqlx::query("SELECT id,body,author_type,author_agent_id,author_user_id,created_at,updated_at FROM document_annotation_comments WHERE thread_id=$1 ORDER BY created_at ASC").bind(thread_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"id":thread_id,"threadId":thread_id,"caseId":id,"documentKey":key,"status":row.get::<String,_>("status"),"anchorState":row.get::<String,_>("anchor_state"),"selectedText":row.get::<String,_>("selected_text"),"anchorSelector":row.get::<serde_json::Value,_>("anchor_selector"),"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),"comments":comments.into_iter().map(|c|serde_json::json!({"id":c.get::<Uuid,_>("id"),"body":c.get::<String,_>("body"),"authorType":c.get::<String,_>("author_type"),"authorAgentId":c.get::<Option<Uuid>,_>("author_agent_id"),"authorUserId":c.get::<Option<String>,_>("author_user_id"),"createdAt":c.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":c.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})).collect::<Vec<_>>() })))
}

/// C26: POST /cases/:id/documents/:key/annotations — Create document annotation
async fn create_document_annotation(
    State(state): State<AppState>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let (document_id, company_id) = case_document_id(&state, id, &key).await?;
    let selected = payload.get("selectedText").and_then(|v|v.as_str()).unwrap_or_default();
    let selector = payload.get("anchorSelector").or_else(||payload.get("selector")).cloned().unwrap_or_else(||serde_json::json!({}));
    let thread_id: Uuid = sqlx::query_scalar("INSERT INTO document_annotation_threads (company_id,case_id,document_id,document_key,selected_text,anchor_selector,original_revision_number,current_revision_number) SELECT $1,$2,$3,$4,$5,$6,COALESCE(MAX(revision_number),0),COALESCE(MAX(revision_number),0) FROM document_revisions WHERE document_id=$3 RETURNING id")
        .bind(company_id).bind(id).bind(document_id).bind(&key).bind(selected).bind(&selector).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "threadId": thread_id, "id": thread_id, "caseId": id, "documentKey": key,
        "status": "open", "selectedText": selected, "anchorSelector": selector, "comments": [],
    }))))
}

/// C27: POST /cases/:id/documents/:key/annotations/:thread_id/reply — Reply to annotation thread
async fn reply_document_annotation(
    State(state): State<AppState>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let (document_id, company_id) = case_document_id(&state, id, &key).await?;
    let body = payload.get("body").and_then(|v|v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let comment_id: Uuid = sqlx::query_scalar("INSERT INTO document_annotation_comments (company_id,thread_id,case_id,document_id,body,author_type) SELECT $1,$2,$3,$4,$5,'user' WHERE EXISTS (SELECT 1 FROM document_annotation_threads WHERE id=$2 AND case_id=$3 AND document_id=$4) RETURNING id")
        .bind(company_id).bind(thread_id).bind(id).bind(document_id).bind(body).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    sqlx::query("UPDATE document_annotation_threads SET updated_at=NOW() WHERE id=$1").bind(thread_id).execute(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": comment_id, "threadId": thread_id, "caseId": id, "documentKey": key, "body": body,
    }))))
}

/// C28: PATCH /cases/:id/documents/:key/annotations/:thread_id — Update annotation thread
async fn update_document_annotation(
    State(state): State<AppState>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let status = payload.get("status").and_then(|v|v.as_str()).unwrap_or("open");
    if !matches!(status, "open" | "resolved") { return Err(StatusCode::BAD_REQUEST); }
    let updated = sqlx::query("UPDATE document_annotation_threads SET status=$1, resolved_at=CASE WHEN $1='resolved' THEN NOW() ELSE NULL END, updated_at=NOW() WHERE id=$2 AND case_id=$3 AND document_id=$4 RETURNING id,status,updated_at")
        .bind(status).bind(thread_id).bind(id).bind(document_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({
        "threadId": updated.get::<Uuid,_>("id"), "caseId": id, "documentKey": key,
        "status": updated.get::<String,_>("status"), "updatedAt": updated.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),
    })))
}

/// C29: POST /cases/:id/automation/retry
async fn automation_retry(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.automation_retry(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C30: POST /cases/:id/automation/retry-plan
async fn automation_retry_plan(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.automation_retry_plan(id, company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C31: POST /cases/:id/automation/current-stage/rerun
async fn automation_rerun_stage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.automation_rerun_stage(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// C32: POST /cases/:id/automations/:automation_id/retry
async fn automation_retry_single(
    State(state): State<AppState>,
    Path((id, automation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = get_company_id_for_case(&state, id).await?;
    state.case_service.automation_retry_single(id, company_id, automation_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /cases/:id/advance — Advance pipeline case to next stage
async fn advance_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<PipelineCase>, AppError> {
    let to_stage_id: Uuid = body.get("to_stage_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::BadRequest("Missing to_stage_id".to_string()))?;

    let input = AdvanceCaseInput {
        case_id,
        to_stage_id,
        actor_type: body.get("actor_type").and_then(|v| v.as_str().map(String::from)),
        actor_id: body.get("actor_id").and_then(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok())),
        note: body.get("note").and_then(|v| v.as_str().map(String::from)),
    };

    let case = state
        .pipeline_service
        .advance_case(input)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(case))
}

/// POST /cases/:id/terminal — Mark pipeline case as terminal (done/cancelled)
async fn mark_terminal(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<PipelineCase>, AppError> {
    let kind_str = body.get("kind").and_then(|v| v.as_str()).unwrap_or("done");
    let kind = match kind_str {
        "cancelled" => models::pipeline::TerminalKind::Cancelled,
        _ => models::pipeline::TerminalKind::Done,
    };

    let case = state
        .pipeline_service
        .mark_terminal(case_id, kind)
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(Json(case))
}

/// Create case routes
pub fn case_routes() -> Router<AppState> {
    Router::new()
        .route("/companies/:companyId/cases", post(create_case).get(list_cases))
        .route("/cases/:id", get(get_case).patch(update_case))
        .route("/cases/:id/detail", get(get_case_detail))
        .route("/cases/:id/events", get(list_case_events))
        // --- P1: Case 子资源/状态机动作 (C1-C32) ---
        .route("/cases/:id/children", get(get_case_children))
        .route("/cases/:id/children/tree", get(get_case_children_tree))
        .route("/cases/:id/rollup", get(get_case_rollup))
        .route("/cases/:id/context-pack", get(get_case_context_pack))
        .route("/cases/:id/outputs", get(get_case_outputs))
        .route("/cases/:id/issue-links", get(list_issue_links).post(create_issue_link))
        .route("/cases/:id/issue-links/:link_id", delete(delete_issue_link))
        .route("/cases/:id/links", post(create_link))
        .route("/cases/:id/blockers", put(update_blockers))
        .route("/cases/:id/suggest-transition", post(suggest_transition))
        .route("/cases/:id/resolve-suggestion", post(resolve_suggestion))
        .route("/cases/:id/review", post(review_case))
        .route("/cases/:id/acknowledge-drift", post(acknowledge_drift))
        .route("/cases/:id/open-conversation", post(open_conversation))
        .route("/cases/:id/breakdown", post(breakdown_case))
        .route("/cases/:id/attachments", post(upload_case_attachment))
        // Pipeline case operations (advance, terminal) — owned by cases module
        .route("/cases/:id/advance", patch(advance_case))
        .route("/cases/:id/terminal", post(mark_terminal))
        // Case documents CRUD (C16-C20)
        .route("/cases/:id/documents/:key", get(get_case_document).post(create_case_document).put(update_case_document).delete(delete_case_document))
        .route("/cases/:id/documents/:key/lock", post(lock_case_document))
        .route("/cases/:id/documents/:key/unlock", post(unlock_case_document))
        // Case document revisions (C21-C23)
        .route("/cases/:id/documents/:key/revisions", get(get_document_revisions))
        .route("/cases/:id/documents/:key/revisions/:revision_id/restore", post(restore_document_revision))
        // Case document annotations (C24-C28)
        .route("/cases/:id/documents/:key/annotations", get(get_document_annotations).post(create_document_annotation))
        .route("/cases/:id/documents/:key/annotations/:thread_id", get(get_document_annotation_thread).patch(update_document_annotation))
        .route("/cases/:id/documents/:key/annotations/:thread_id/reply", post(reply_document_annotation))
        // Case automation (C29-C32)
        .route("/cases/:id/automation/retry", post(automation_retry))
        .route("/cases/:id/automation/retry-plan", post(automation_retry_plan))
        .route("/cases/:id/automation/current-stage/rerun", post(automation_rerun_stage))
        .route("/cases/:id/automations/:automation_id/retry", post(automation_retry_single))
}
