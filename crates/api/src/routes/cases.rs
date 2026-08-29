use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde::Deserialize;
use crate::app_state::AppState;
use crate::errors::AppError;
use uuid::Uuid;
use sqlx::{Postgres, Row, Transaction};

use models::{Case, CaseDetail, CaseEvent, CreateCaseInput, PipelineCase, UpdateCaseInput};
use services::{AdvanceCaseInput, CaseQueryFilter, Pagination};
use services::auth::AuthorizationActor;
use crate::routes::{require_company_access, AccessMode};

/// Helper: 通过 case_id 查询 company_id
async fn get_company_id_for_case(state: &AppState, case_id: Uuid) -> Result<Uuid, StatusCode> {
    sqlx::query_scalar("SELECT company_id FROM cases WHERE id = $1")
        .bind(case_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn scoped_case_company(
    state: &AppState,
    actor: &AuthorizationActor,
    case_id: Uuid,
    mode: AccessMode,
) -> Result<Uuid, StatusCode> {
    let company_id = get_company_id_for_case(state, case_id).await?;
    require_company_access(actor, company_id, mode)?;
    Ok(company_id)
}

fn validate_case_document_key(raw_key: &str) -> Result<String, StatusCode> {
    let key = raw_key.trim();
    if key.is_empty()
        || key.len() > 120
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(key.to_string())
}

fn document_actor(actor: &AuthorizationActor) -> (Option<&'static str>, Option<Uuid>) {
    match actor {
        AuthorizationActor::Board { user_id, .. } => (Some("user"), Some(*user_id)),
        AuthorizationActor::Agent { agent_id, .. } => (Some("agent"), Some(*agent_id)),
        AuthorizationActor::None => (None, None),
    }
}

fn document_content(payload: &serde_json::Value) -> Result<&str, StatusCode> {
    payload
        .get("body")
        .or_else(|| payload.get("content"))
        .and_then(serde_json::Value::as_str)
        .ok_or(StatusCode::BAD_REQUEST)
}

fn document_content_type(payload: &serde_json::Value) -> Result<&str, StatusCode> {
    let content_type = payload
        .get("format")
        .or_else(|| payload.get("contentType"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("text/markdown");
    if content_type == "markdown" {
        Ok("text/markdown")
    } else if content_type == "text/markdown" {
        Ok(content_type)
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

async fn lock_case_document_key(
    tx: &mut Transaction<'_, Postgres>,
    case_id: Uuid,
    key: &str,
) -> Result<(), StatusCode> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2, 0))")
        .bind(case_id)
        .bind(key)
        .execute(&mut **tx)
        .await
        .map(|_| ())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
        "SELECT d.id, c.company_id
         FROM cases c
         JOIN case_documents cd ON cd.case_id = c.id AND cd.company_id = c.company_id
         JOIN documents d ON d.id = cd.document_id AND d.company_id = c.company_id
         WHERE c.id = $1 AND cd.key = $2")
        .bind(case_id).bind(key).fetch_optional(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

/// C16: GET /cases/:id/documents/:key — Get case document content
async fn get_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Read).await?;
    let key = validate_case_document_key(&key)?;
    let row = sqlx::query("SELECT d.id, cd.company_id, cd.key, d.content, d.content_type, d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, d.created_at, d.updated_at FROM case_documents cd JOIN documents d ON d.id=cd.document_id AND d.company_id=cd.company_id WHERE cd.case_id=$1 AND cd.company_id=$2 AND cd.key=$3")
        .bind(id).bind(company_id).bind(&key).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({"id": row.get::<Uuid,_>("id"), "caseId": id, "companyId": row.get::<Uuid,_>("company_id"), "key": row.get::<String,_>("key"), "content": row.get::<String,_>("content"), "contentType": row.get::<Option<String>,_>("content_type").unwrap_or_else(|| "text/markdown".into()), "lockedByType": row.get::<Option<String>,_>("locked_by_type"), "lockedById": row.get::<Option<Uuid>,_>("locked_by_id"), "lockedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("locked_at"), "lockedRunId": row.get::<Option<Uuid>,_>("locked_run_id"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"), "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})))
}

/// C17: POST /cases/:id/documents/:key — Create case document
async fn create_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let content = document_content(&payload)?;
    if content.len() > 200_000 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let content_type = document_content_type(&payload)?;
    let title = payload
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&key);
    let (created_by_type, created_by_id) = document_actor(&actor);
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    if sqlx::query_scalar::<_, Uuid>(
        "SELECT document_id FROM case_documents WHERE case_id=$1 AND company_id=$2 AND key=$3 FOR UPDATE",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .is_some()
    {
        return Err(StatusCode::CONFLICT);
    }
    let document_id: Uuid = sqlx::query_scalar("INSERT INTO documents (company_id, title, content, content_type) VALUES ($1,$2,$3,$4) RETURNING id")
        .bind(company_id).bind(title).bind(content).bind(content_type).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO case_documents (company_id, case_id, document_id, key) VALUES ($1,$2,$3,$4)")
        .bind(company_id).bind(id).bind(document_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
    sqlx::query("INSERT INTO document_revisions (company_id, document_id, revision_number, content, created_by_type, created_by_id) VALUES ($1,$2,1,$3,$4,$5)")
        .bind(company_id).bind(document_id).bind(content).bind(created_by_type).bind(created_by_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id": document_id, "caseId": id, "key": key, "content": content, "body": content, "contentType": content_type, "format": "markdown", "revisionNumber": 1}))))
}

/// C18: PUT /cases/:id/documents/:key — Update case document
async fn update_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let content = document_content(&payload)?;
    if content.len() > 200_000 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let content_type = document_content_type(&payload)?;
    let title = payload
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let base_revision_id = payload
        .get("baseRevisionId")
        .and_then(serde_json::Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (created_by_type, created_by_id) = document_actor(&actor);
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents WHERE case_id=$1 AND company_id=$2 AND key=$3 FOR UPDATE",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let locked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT locked_at FROM documents WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(document_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .flatten();
    if locked_at.is_some() {
        return Err(StatusCode::CONFLICT);
    }
    let latest_revision_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM document_revisions WHERE document_id=$1 ORDER BY revision_number DESC LIMIT 1",
    )
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if base_revision_id.is_none() || base_revision_id != latest_revision_id {
        return Err(StatusCode::CONFLICT);
    }
    let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1")
        .bind(document_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET title=COALESCE($2,title), content=$3, content_type=$4, updated_by_agent_id=$5, updated_at=NOW() WHERE id=$1 AND company_id=$6")
        .bind(document_id).bind(title).bind(content).bind(content_type)
        .bind(match actor { AuthorizationActor::Agent { agent_id, .. } => Some(agent_id), _ => None })
        .bind(company_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let revision_id: Uuid = sqlx::query_scalar("INSERT INTO document_revisions (company_id, document_id, revision_number, content, created_by_type, created_by_id) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(company_id).bind(document_id).bind(revision).bind(content).bind(created_by_type).bind(created_by_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE case_documents SET updated_at=NOW() WHERE case_id=$1 AND company_id=$2 AND key=$3")
        .bind(id).bind(company_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"id": document_id, "caseId": id, "key": key, "content": content, "body": content, "contentType": content_type, "revisionId": revision_id, "revisionNumber": revision})))
}

/// C19: POST /cases/:id/documents/:key/lock — Lock case document
async fn lock_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(_payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let (actor_type, actor_id) = document_actor(&actor);
    let run_id = match &actor {
        AuthorizationActor::Agent { run_id, .. } => *run_id,
        _ => None,
    };
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents WHERE case_id=$1 AND company_id=$2 AND key=$3 FOR UPDATE",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let existing: (Option<String>, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT locked_by_type, locked_by_id, locked_at FROM documents WHERE id=$1 AND company_id=$2 FOR UPDATE")
            .bind(document_id)
            .bind(company_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let locked_at = if existing.2.is_none() {
        sqlx::query_scalar("UPDATE documents SET locked_by_type=$2, locked_by_id=$3, locked_at=NOW(), locked_run_id=$4, updated_at=NOW() WHERE id=$1 AND company_id=$5 RETURNING locked_at")
            .bind(document_id).bind(actor_type).bind(actor_id).bind(run_id).bind(company_id)
            .fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        existing.2
    };
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "caseId": id,
        "key": key,
        "locked": true,
        "lockedByType": existing.0.or(actor_type.map(str::to_string)),
        "lockedById": existing.1.or(actor_id),
        "lockedAt": locked_at,
    })))
}

/// C20: POST /cases/:id/documents/:key/unlock — Unlock case document
async fn unlock_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let principal_id = actor.principal_id().ok_or(StatusCode::UNAUTHORIZED)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents WHERE case_id=$1 AND company_id=$2 AND key=$3 FOR UPDATE",
    )
    .bind(id).bind(company_id).bind(&key).fetch_optional(&mut *tx).await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let locked_by: (Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT locked_by_id, locked_at FROM documents WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(document_id).bind(company_id).fetch_one(&mut *tx).await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if locked_by.1.is_some() && locked_by.0 != Some(principal_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("UPDATE documents SET locked_by_type=NULL, locked_by_id=NULL, locked_at=NULL, locked_run_id=NULL, updated_at=NOW() WHERE id=$1 AND company_id=$2")
        .bind(document_id).bind(company_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"caseId": id, "key": key, "unlocked": true})))
}

/// C21: GET /cases/:id/documents/:key/revisions
async fn get_document_revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Read).await?;
    let key = validate_case_document_key(&key)?;
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let rows = sqlx::query("SELECT id,revision_number,content,created_at,created_by_type,created_by_id FROM document_revisions WHERE company_id=$1 AND document_id=$2 ORDER BY revision_number DESC")
        .bind(company_id).bind(document_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({"id":r.get::<Uuid,_>("id"),"revisionId":r.get::<Uuid,_>("id"),"revisionNumber":r.get::<i32,_>("revision_number"),"version":r.get::<i32,_>("revision_number"),"content":r.get::<String,_>("content"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at")})).collect()))
}

/// C22: POST /cases/:id/documents/:key/revisions/:revision_id/restore
async fn restore_document_revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key, revision_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let (created_by_type, created_by_id) = document_actor(&actor);
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents WHERE case_id=$1 AND company_id=$2 AND key=$3 FOR UPDATE",
    )
    .bind(id).bind(company_id).bind(&key).fetch_optional(&mut *tx).await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let locked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT locked_at FROM documents WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(document_id).bind(company_id).fetch_one(&mut *tx).await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if locked_at.is_some() { return Err(StatusCode::CONFLICT); }
    let source: (String, i32) = sqlx::query_as(
        "SELECT content, revision_number FROM document_revisions WHERE id=$1 AND document_id=$2 AND company_id=$3",
    )
    .bind(revision_id).bind(document_id).bind(company_id).fetch_optional(&mut *tx).await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE company_id=$1 AND document_id=$2")
        .bind(company_id).bind(document_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET content=$2, updated_at=NOW() WHERE id=$1 AND company_id=$3")
        .bind(document_id).bind(&source.0).bind(company_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let restored_revision_id: Uuid = sqlx::query_scalar("INSERT INTO document_revisions(company_id, document_id, revision_number, content, created_by_type, created_by_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
        .bind(company_id).bind(document_id).bind(revision).bind(&source.0).bind(created_by_type).bind(created_by_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE case_documents SET updated_at=NOW() WHERE case_id=$1 AND company_id=$2 AND key=$3")
        .bind(id).bind(company_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"restored":true,"revisionId":restored_revision_id,"restoredFromRevisionId":revision_id,"restoredFromRevisionNumber":source.1,"revisionNumber":revision,"content":source.0,"body":source.0})))
}

/// C23: DELETE /cases/:id/documents/:key
async fn delete_case_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_case_document_key(&mut tx, id, &key).await?;
    let document_id: Option<Uuid> = sqlx::query_scalar("SELECT document_id FROM case_documents WHERE case_id=$1 AND key=$2 AND company_id=$3 FOR UPDATE")
        .bind(id).bind(&key).bind(company_id).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(document_id) = document_id else { return Ok(StatusCode::NO_CONTENT); };
    let locked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT locked_at FROM documents WHERE id=$1 AND company_id=$2 FOR UPDATE")
        .bind(document_id).bind(company_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if locked_at.is_some() { return Err(StatusCode::CONFLICT); }
    sqlx::query("DELETE FROM case_documents WHERE case_id=$1 AND key=$2 AND company_id=$3")
        .bind(id).bind(&key).bind(company_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM documents WHERE id=$1 AND company_id=$2")
        .bind(document_id).bind(company_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// C24: GET /cases/:id/documents/:key/annotations
async fn get_document_annotations(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Read).await?;
    let key = validate_case_document_key(&key)?;
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let status = params.get("status").map(String::as_str).unwrap_or("all");
    if !matches!(status, "all" | "open" | "resolved") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let include_comments = params
        .get("includeComments")
        .map(|value| value == "true")
        .unwrap_or(true);
    let rows = sqlx::query(
        "SELECT id, status, anchor_state, selected_text, anchor_selector, anchor_confidence,
                prefix_text, suffix_text, normalized_start, normalized_end, markdown_start,
                markdown_end, original_revision_number, current_revision_number, created_at,
                updated_at
         FROM document_annotation_threads
         WHERE company_id=$1 AND case_id=$2 AND document_id=$3 AND document_key=$4
           AND ($5 = 'all' OR status = $5)
         ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .bind(id)
    .bind(document_id)
    .bind(&key)
    .bind(status)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let thread_id: Uuid = row.get("id");
        let mut thread_json = serde_json::json!({
            "id": thread_id,
            "threadId": thread_id,
            "caseId": id,
            "documentKey": key,
            "status": row.get::<String, _>("status"),
            "anchorState": row.get::<String, _>("anchor_state"),
            "anchorConfidence": row.get::<String, _>("anchor_confidence"),
            "selectedText": row.get::<String, _>("selected_text"),
            "anchorSelector": row.get::<serde_json::Value, _>("anchor_selector"),
            "prefixText": row.get::<String, _>("prefix_text"),
            "suffixText": row.get::<String, _>("suffix_text"),
            "normalizedStart": row.get::<i32, _>("normalized_start"),
            "normalizedEnd": row.get::<i32, _>("normalized_end"),
            "markdownStart": row.get::<i32, _>("markdown_start"),
            "markdownEnd": row.get::<i32, _>("markdown_end"),
            "originalRevisionNumber": row.get::<i32, _>("original_revision_number"),
            "currentRevisionNumber": row.get::<i32, _>("current_revision_number"),
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        });
        if include_comments {
            let comments = sqlx::query(
                "SELECT id, body, author_type, author_agent_id, author_user_id, created_at, updated_at
                 FROM document_annotation_comments
                 WHERE company_id=$1 AND case_id=$2 AND thread_id=$3
                 ORDER BY created_at ASC",
            )
            .bind(company_id)
            .bind(id)
            .bind(thread_id)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let comments_json: Vec<serde_json::Value> = comments
                .into_iter()
                .map(|comment| {
                    serde_json::json!({
                        "id": comment.get::<Uuid, _>("id"),
                        "body": comment.get::<String, _>("body"),
                        "authorType": comment.get::<String, _>("author_type"),
                        "authorAgentId": comment.get::<Option<Uuid>, _>("author_agent_id"),
                        "authorUserId": comment.get::<Option<String>, _>("author_user_id"),
                        "createdAt": comment.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                        "updatedAt": comment.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                    })
                })
                .collect();
            thread_json["comments"] = serde_json::json!(comments_json);
        }
        result.push(thread_json);
    }
    Ok(Json(result))
}

/// C25: GET /cases/:id/documents/:key/annotations/:thread_id
async fn get_document_annotation_thread(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Read).await?;
    let key = validate_case_document_key(&key)?;
    let (document_id, _) = case_document_id(&state, id, &key).await?;
    let row = sqlx::query(
        "SELECT id, status, anchor_state, selected_text, anchor_selector, anchor_confidence,
                prefix_text, suffix_text, normalized_start, normalized_end, markdown_start,
                markdown_end, original_revision_number, current_revision_number, created_at,
                updated_at
         FROM document_annotation_threads
         WHERE id=$1 AND company_id=$2 AND case_id=$3 AND document_id=$4 AND document_key=$5",
    )
    .bind(thread_id)
    .bind(company_id)
    .bind(id)
    .bind(document_id)
    .bind(&key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let comments = sqlx::query(
        "SELECT id, body, author_type, author_agent_id, author_user_id, created_at, updated_at
         FROM document_annotation_comments
         WHERE company_id=$1 AND case_id=$2 AND thread_id=$3
         ORDER BY created_at ASC",
    )
    .bind(company_id)
    .bind(id)
    .bind(thread_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let comments_json: Vec<serde_json::Value> = comments
        .into_iter()
        .map(|comment| {
            serde_json::json!({
                "id": comment.get::<Uuid, _>("id"),
                "body": comment.get::<String, _>("body"),
                "authorType": comment.get::<String, _>("author_type"),
                "authorAgentId": comment.get::<Option<Uuid>, _>("author_agent_id"),
                "authorUserId": comment.get::<Option<String>, _>("author_user_id"),
                "createdAt": comment.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": comment.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({
        "id": thread_id,
        "threadId": thread_id,
        "caseId": id,
        "documentKey": key,
        "status": row.get::<String, _>("status"),
        "anchorState": row.get::<String, _>("anchor_state"),
        "anchorConfidence": row.get::<String, _>("anchor_confidence"),
        "selectedText": row.get::<String, _>("selected_text"),
        "anchorSelector": row.get::<serde_json::Value, _>("anchor_selector"),
        "prefixText": row.get::<String, _>("prefix_text"),
        "suffixText": row.get::<String, _>("suffix_text"),
        "normalizedStart": row.get::<i32, _>("normalized_start"),
        "normalizedEnd": row.get::<i32, _>("normalized_end"),
        "markdownStart": row.get::<i32, _>("markdown_start"),
        "markdownEnd": row.get::<i32, _>("markdown_end"),
        "originalRevisionNumber": row.get::<i32, _>("original_revision_number"),
        "currentRevisionNumber": row.get::<i32, _>("current_revision_number"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        "comments": comments_json,
    })))
}

/// C26: POST /cases/:id/documents/:key/annotations — Create document annotation
async fn create_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key)): Path<(Uuid, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let selected = payload
        .get("selectedText")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let selector = payload
        .get("anchorSelector")
        .or_else(|| payload.get("selector"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let base_revision_id = payload
        .get("baseRevisionId")
        .and_then(|value| value.as_str())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let base_revision_number = payload
        .get("baseRevisionNumber")
        .and_then(|value| value.as_i64())
        .map(i32::try_from)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if base_revision_id.is_some() != base_revision_number.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let parse_position = |name: &str| -> Result<i32, StatusCode> {
        payload
            .get(name)
            .and_then(|value| value.as_i64())
            .map(i32::try_from)
            .transpose()
            .map_err(|_| StatusCode::BAD_REQUEST)
            .map(|value| value.unwrap_or(0))
    };
    let normalized_start = parse_position("normalizedStart")?;
    let normalized_end = parse_position("normalizedEnd")?;
    let markdown_start = parse_position("markdownStart")?;
    let markdown_end = parse_position("markdownEnd")?;
    let prefix_text = payload
        .get("prefixText")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let suffix_text = payload
        .get("suffixText")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let (author_type, author_agent_id, author_user_id, created_by_run_id) = match &actor {
        AuthorizationActor::Board { user_id, .. } => (
            "user",
            None,
            Some(user_id.to_string()),
            None,
        ),
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => ("agent", Some(*agent_id), None, *run_id),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (document_id, document_content): (Uuid, String) = sqlx::query_as(
        "SELECT d.id, d.content
         FROM case_documents link
         JOIN documents d ON d.id=link.document_id AND d.company_id=link.company_id
         WHERE link.case_id=$1 AND link.company_id=$2 AND link.key=$3
         FOR UPDATE OF link, d",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let (current_revision_id, revision_number): (Option<Uuid>, i32) = sqlx::query_as(
        "SELECT id, revision_number
         FROM document_revisions
         WHERE company_id=$1 AND document_id=$2
         ORDER BY revision_number DESC
         LIMIT 1
         FOR UPDATE",
    )
    .bind(company_id)
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(|(revision_id, number): (Uuid, i32)| (Some(revision_id), number))
    .unwrap_or((None, 0));
    if let (Some(expected_id), Some(expected_number)) = (base_revision_id, base_revision_number) {
        if current_revision_id != Some(expected_id) || revision_number != expected_number {
            return Err(StatusCode::CONFLICT);
        }
    }
    if !selected.is_empty() && !document_content.contains(selected) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let thread_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_threads
         (company_id, case_id, document_id, document_key, selected_text, anchor_selector,
          original_revision_id, original_revision_number, current_revision_id,
          current_revision_number, normalized_start, normalized_end, markdown_start,
          markdown_end, prefix_text, suffix_text, created_by_agent_id, created_by_user_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
         RETURNING id",
    )
    .bind(company_id)
    .bind(id)
    .bind(document_id)
    .bind(&key)
    .bind(selected)
    .bind(&selector)
    .bind(current_revision_id)
    .bind(revision_number)
    .bind(normalized_start)
    .bind(normalized_end)
    .bind(markdown_start)
    .bind(markdown_end)
    .bind(prefix_text)
    .bind(suffix_text)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments
         (company_id, thread_id, case_id, document_id, body, author_type,
          author_agent_id, author_user_id, created_by_run_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(id)
    .bind(document_id)
    .bind(body)
    .bind(author_type)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .bind(created_by_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "threadId": thread_id, "id": thread_id, "caseId": id, "documentKey": key,
        "status": "open", "selectedText": selected, "anchorSelector": selector,
        "comments": [{"id": comment_id, "body": body, "authorType": author_type,
            "authorAgentId": author_agent_id, "authorUserId": author_user_id,
            "createdByRunId": created_by_run_id}],
    }))))
}

/// C27: POST /cases/:id/documents/:key/annotations/:thread_id/reply — Reply to annotation thread
async fn reply_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let (author_type, author_agent_id, author_user_id, created_by_run_id) = match &actor {
        AuthorizationActor::Board { user_id, .. } => (
            "user",
            None,
            Some(user_id.to_string()),
            None,
        ),
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => ("agent", Some(*agent_id), None, *run_id),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents
         WHERE case_id=$1 AND company_id=$2 AND key=$3
         FOR UPDATE",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM document_annotation_threads
         WHERE id=$1 AND company_id=$2 AND case_id=$3 AND document_id=$4
         FOR UPDATE",
    )
    .bind(thread_id)
    .bind(company_id)
    .bind(id)
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments
         (company_id, thread_id, case_id, document_id, body, author_type,
          author_agent_id, author_user_id, created_by_run_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id",
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(id)
    .bind(document_id)
    .bind(body)
    .bind(author_type)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .bind(created_by_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE document_annotation_threads SET updated_at=NOW() WHERE id=$1 AND company_id=$2")
        .bind(thread_id)
        .bind(company_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": comment_id, "threadId": thread_id, "caseId": id, "documentKey": key, "body": body,
        "authorType": author_type, "authorAgentId": author_agent_id,
        "authorUserId": author_user_id, "createdByRunId": created_by_run_id,
    }))))
}

/// C28: PATCH /cases/:id/documents/:key/annotations/:thread_id — Update annotation thread
async fn update_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, key, thread_id)): Path<(Uuid, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_case_company(&state, &actor, id, AccessMode::Write).await?;
    let key = validate_case_document_key(&key)?;
    let status = payload
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("open");
    if !matches!(status, "open" | "resolved") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (resolved_by_agent_id, resolved_by_user_id) = match &actor {
        AuthorizationActor::Board { user_id, .. } => (None, Some(user_id.to_string())),
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM case_documents
         WHERE case_id=$1 AND company_id=$2 AND key=$3
         FOR UPDATE",
    )
    .bind(id)
    .bind(company_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let updated = sqlx::query(
        "UPDATE document_annotation_threads
         SET status=$1,
             resolved_by_agent_id=CASE WHEN $1='resolved' THEN $5 ELSE NULL END,
             resolved_by_user_id=CASE WHEN $1='resolved' THEN $6 ELSE NULL END,
             resolved_at=CASE WHEN $1='resolved' THEN NOW() ELSE NULL END,
             updated_at=NOW()
         WHERE id=$2 AND company_id=$3 AND case_id=$4 AND document_id=$7
         RETURNING id, status, updated_at",
    )
    .bind(status)
    .bind(thread_id)
    .bind(company_id)
    .bind(id)
    .bind(resolved_by_agent_id)
    .bind(resolved_by_user_id)
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
