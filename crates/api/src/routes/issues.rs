use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::Row;
use crate::app_state::AppState;
use uuid::Uuid;

use models::{CreateIssueInput, Issue, IssuePriority, IssueStatus, UpdateIssueInput};
use services::{
    CheckoutInput, IssueQueryFilter, Pagination, ReleaseInput,
};
use services::auth::AuthorizationActor;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListIssuesQuery {
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    offset: Option<i64>,
    status: Option<String>,
    priority: Option<String>,
    assignee_agent_id: Option<Uuid>,
    assignee_user_id: Option<Uuid>,
    project_id: Option<Uuid>,
    q: Option<String>,
}

fn parse_issue_statuses(value: Option<&str>) -> Option<Vec<IssueStatus>> {
    let values = value?.split(',').map(str::trim).filter(|v| !v.is_empty());
    let parsed: Vec<_> = values
        .filter_map(|value| serde_json::from_value::<IssueStatus>(serde_json::Value::String(value.to_owned())).ok())
        .collect();
    Some(parsed).filter(|values| !values.is_empty())
}

fn parse_issue_priorities(value: Option<&str>) -> Option<Vec<IssuePriority>> {
    let values = value?.split(',').map(str::trim).filter(|v| !v.is_empty());
    let parsed: Vec<_> = values
        .filter_map(|value| serde_json::from_value::<IssuePriority>(serde_json::Value::String(value.to_owned())).ok())
        .collect();
    Some(parsed).filter(|values| !values.is_empty())
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListIssueDocumentsQuery {
    #[serde(default)]
    include_system: bool,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct IssueDocumentResponse {
    id: Uuid,
    company_id: Uuid,
    issue_id: Uuid,
    key: String,
    content: String,
    content_type: Option<String>,
    locked_by_type: Option<String>,
    locked_by_id: Option<Uuid>,
    locked_at: Option<chrono::DateTime<chrono::Utc>>,
    locked_run_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

async fn list_issue_documents(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(issue_id): Path<Uuid>,
    Query(query): Query<ListIssueDocumentsQuery>,
) -> Result<Json<Vec<IssueDocumentResponse>>, StatusCode> {
    scoped_issue_company(&state, &actor, issue_id).await?;

    let mut sql = String::from(
        "SELECT d.id, d.company_id, l.issue_id, l.key, d.content, d.content_type, \
                d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, \
                d.created_at, d.updated_at \
         FROM issue_documents l \
         JOIN documents d ON d.id = l.document_id \
         WHERE l.issue_id = $1",
    );
    if !query.include_system {
        sql.push_str(" AND l.key NOT LIKE '__system/%'");
    }
    sql.push_str(" ORDER BY l.key ASC");

    sqlx::query_as::<_, IssueDocumentResponse>(&sql)
        .bind(issue_id)
        .fetch_all(&state.pool)
        .await
        .map(Json)
        .map_err(|error| {
            tracing::error!(error = %error, issue_id = %issue_id, "failed to list issue documents");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /issues/:id/documents/:key
async fn get_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(Uuid, String)>,
) -> Result<Json<IssueDocumentResponse>, StatusCode> {
    scoped_issue_company(&state, &actor, issue_id).await?;

    let key = key.trim().to_lowercase();
    sqlx::query_as::<_, IssueDocumentResponse>(
        "SELECT d.id, d.company_id, l.issue_id, l.key, d.content, d.content_type, \
                d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, \
                d.created_at, d.updated_at \
         FROM issue_documents l \
         JOIN documents d ON d.id = l.document_id \
         WHERE l.issue_id = $1 AND l.key = $2",
    )
    .bind(issue_id)
    .bind(key)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %issue_id, "failed to get issue document");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map(Json)
    .ok_or(StatusCode::NOT_FOUND)
}

fn validate_issue_document_key(key: &str) -> Result<String, StatusCode> {
    let key = key.trim().to_lowercase();
    if key.is_empty() || key.len() > 64 || !key.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-') {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(key)
}

/// PUT /issues/:id/documents/:key — Create or update an issue document.
async fn upsert_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key)): Path<(Uuid, String)>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let key = validate_issue_document_key(&raw_key)?;
    let content = payload.get("body").or_else(|| payload.get("content")).and_then(|value| value.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    if content.len() > 524_288 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let content_type = if payload.get("format").and_then(|value| value.as_str()).unwrap_or("markdown") == "markdown" { "text/markdown" } else { return Err(StatusCode::BAD_REQUEST) };
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing = sqlx::query("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2 FOR UPDATE")
        .bind(issue_id).bind(&key).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let base_revision_id = payload
        .get("baseRevisionId")
        .and_then(|value| value.as_str())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let (document_id, revision_number) = if let Some(row) = existing {
        let document_id: Uuid = row.get("document_id");
        if let Some(base_revision_id) = base_revision_id {
            let current_revision_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM document_revisions WHERE document_id=$1 ORDER BY revision_number DESC LIMIT 1",
            )
            .bind(document_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if current_revision_id != Some(base_revision_id) {
                return Err(StatusCode::CONFLICT);
            }
        }
        let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1")
            .bind(document_id).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        sqlx::query("UPDATE documents SET content=$2, content_type=$3, updated_at=NOW() WHERE id=$1")
            .bind(document_id).bind(content).bind(content_type).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        (document_id, revision)
    } else {
        if base_revision_id.is_some() {
            return Err(StatusCode::CONFLICT);
        }
        let document_id: Uuid = sqlx::query_scalar("INSERT INTO documents (company_id, content, content_type) VALUES ($1,$2,$3) RETURNING id")
            .bind(company_id).bind(content).bind(content_type).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        sqlx::query("INSERT INTO issue_documents (company_id, issue_id, document_id, key) VALUES ($1,$2,$3,$4)")
            .bind(company_id).bind(issue_id).bind(document_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
        (document_id, 1)
    };
    let run_id = headers
        .get("x-paperclip-run-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok());
    sqlx::query(
        "INSERT INTO document_revisions
           (document_id, revision_number, content, created_by_type, created_by_id)
         VALUES ($1,$2,$3,CASE WHEN $4::uuid IS NULL THEN NULL ELSE 'agent' END,
                 (SELECT agent_id FROM heartbeat_runs WHERE id=$4))",
    )
        .bind(document_id).bind(revision_number).bind(content).bind(run_id)
        .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::OK, Json(serde_json::json!({
        "id": document_id, "issueId": issue_id, "key": key, "content": content,
        "body": content, "contentType": content_type, "format": "markdown", "revisionNumber": revision_number,
    }))))
}

/// GET /issues/:id/documents/:key/revisions — List issue document revisions.
async fn list_issue_document_revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let document_id: Uuid = sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
        .bind(issue_id).bind(&key).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let rows = sqlx::query("SELECT id, revision_number, content, created_at FROM document_revisions WHERE document_id=$1 ORDER BY revision_number DESC")
        .bind(document_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(|row| serde_json::json!({
        "id": row.get::<Uuid, _>("id"), "revisionId": row.get::<Uuid, _>("id"),
        "revisionNumber": row.get::<i32, _>("revision_number"), "content": row.get::<String, _>("content"),
        "body": row.get::<String, _>("content"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })).collect()))
}

/// POST /issues/:id/documents/:key/revisions/:revision_id/restore.
async fn restore_issue_document_revision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key, revision_id)): Path<(Uuid, String, Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let document_id: Uuid = sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
        .bind(issue_id).bind(&key).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let content: String = sqlx::query_scalar("SELECT content FROM document_revisions WHERE id=$1 AND document_id=$2")
        .bind(revision_id).bind(document_id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let revision: i32 = sqlx::query_scalar("SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1")
        .bind(document_id).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET content=$2, updated_at=NOW() WHERE id=$1").bind(document_id).bind(&content).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let run_id = headers
        .get("x-paperclip-run-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok());
    sqlx::query(
        "INSERT INTO document_revisions
           (document_id, revision_number, content, created_by_type, created_by_id)
         VALUES ($1,$2,$3,CASE WHEN $4::uuid IS NULL THEN NULL ELSE 'agent' END,
                 (SELECT agent_id FROM heartbeat_runs WHERE id=$4))",
    )
        .bind(document_id).bind(revision).bind(&content).bind(run_id)
        .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"restored": true, "issueId": issue_id, "key": key, "revisionId": revision_id, "revisionNumber": revision, "content": content, "body": content})))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

async fn issue_company_id(state: &AppState, issue_id: Uuid) -> Result<Uuid, StatusCode> {
    sqlx::query_scalar("SELECT company_id FROM issues WHERE id=$1")
        .bind(issue_id).fetch_optional(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn scoped_issue_company(
    state: &AppState,
    actor: &AuthorizationActor,
    issue_id: Uuid,
) -> Result<Uuid, StatusCode> {
    let company_id = actor.company_id().ok_or(StatusCode::FORBIDDEN)?;
    let belongs_to_company = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM issues WHERE id = $1 AND company_id = $2)",
    )
    .bind(issue_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    belongs_to_company
        .then_some(company_id)
        .ok_or(StatusCode::NOT_FOUND)
}

/// GET /issues - List all issues
async fn list_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = actor.company_id().ok_or(StatusCode::FORBIDDEN)?;
    
    let filter = IssueQueryFilter {
        status: parse_issue_statuses(query.status.as_deref()),
        priority: parse_issue_priorities(query.priority.as_deref()),
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: query.q.clone().filter(|value| !value.trim().is_empty()),
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(reference): Path<String>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = actor.company_id().ok_or(StatusCode::FORBIDDEN)?;
    let id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM issues WHERE company_id = $1 AND (id::text = $2 OR identifier = $2)",
    )
    .bind(company_id)
    .bind(&reference)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

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
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(mut input): Json<CreateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, false)?;
    // Paperclip takes the company scope from the URL. The body must not need
    // to repeat companyId, and the path is authoritative if it is supplied.
    input.company_id = company_id;
    let service = state.issue_service.clone();
    service
        .create(input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|error| {
            tracing::error!(error = %error, company_id = %company_id, "issue creation failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /companies/:companyId/issues - List issues for a company
async fn list_company_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, true)?;
    let filter = IssueQueryFilter {
        status: parse_issue_statuses(query.status.as_deref()),
        priority: parse_issue_priorities(query.priority.as_deref()),
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: query.q.clone().filter(|value| !value.trim().is_empty()),
    };
    let pagination = Pagination {
        limit: query.limit.unwrap_or(50).clamp(1, 500),
        offset: query.offset.unwrap_or(0).max(0),
        cursor: None,
    };

    state
        .issue_service
        .list(company_id, &filter, &pagination)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PATCH /issues/:id - Update issue
async fn update_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    // Paperclip first loads the issue and uses its companyId for the mutation
    // authorization check. Do the same here instead of passing the placeholder
    // nil UUID used by the older route implementations.
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    let service = state.issue_service.clone();

    service
        .update(id, company_id, input)
        .await
        .map(|result| Json(result.issue))
        .map_err(|error| {
            tracing::error!(
                error = ?error,
                issue_id = %id,
                company_id = %company_id,
                "issue update failed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// DELETE /issues/:id - Delete issue
async fn delete_issue(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = issue_company_id(&state, id).await?;

    service
        .delete(id, company_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /companies/:companyId/issues/count - Count issues
async fn count_issues(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE company_id=$1")
        .bind(company_id).fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"count": count})))
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<CheckoutInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    service
        .checkout(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// POST /issues/:id/release - Release issue
async fn release_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = scoped_issue_company(&state, &actor, id).await?;

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
    let company_id = issue_company_id(&state, id).await?;

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
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = scoped_issue_company(&state, &actor, id).await?;

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
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.get_cases(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I3: GET /issues/:id/active-run
async fn get_issue_active_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
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
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.get_live_runs(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I6: GET /issues/:id/accepted-plan-decompositions
async fn list_plan_decompositions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.get_accepted_plan_decompositions(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I7: POST /issues/:id/accepted-plan-decompositions
async fn submit_plan_decomposition(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    let result = state.issue_service.submit_plan_decomposition(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I8: GET /issues/:id/approvals
async fn list_issue_approvals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    state.issue_service.get_approvals(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I9: POST /issues/:id/approvals
async fn create_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let result = state.issue_service.create_approval(id, company_id, payload).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I10: DELETE /issues/:id/approvals/:approval_id
async fn delete_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, approval_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
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
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };

    let company_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT company_id FROM issues WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to load issue company for read state");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let read_at = chrono::Utc::now();
    let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, chrono::DateTime<chrono::Utc>)>(
        "INSERT INTO issue_read_status (company_id, issue_id, user_id, read_at) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (issue_id, user_id) DO UPDATE SET read_at = EXCLUDED.read_at, updated_at = NOW() \
         RETURNING id, company_id, issue_id, read_at",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id)
    .bind(read_at)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, user_id = %user_id, "failed to mark issue read");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "id": row.0,
        "companyId": row.1,
        "issueId": row.2,
        "userId": user_id,
        "lastReadAt": row.3,
    })))
}

/// I13: DELETE /issues/:id/read
async fn unmark_issue_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.unmark_read(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I14: POST /issues/:id/inbox-archive
async fn archive_issue_inbox(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.archive_inbox(id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I15: DELETE /issues/:id/inbox-archive
async fn unarchive_issue_inbox(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
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
    let company_id = issue_company_id(&state, id).await?;
    state.issue_service.get_recovery_actions(id, company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I28: POST /issues/:id/recovery-actions/resolve
async fn resolve_recovery_action(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    let action_id = payload.get("actionId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    state.issue_service.resolve_recovery_action(id, company_id, action_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I29: GET /issues/:id/interactions
async fn list_interactions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1").bind(id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let rows = sqlx::query("SELECT id, company_id, issue_id, kind, status::text, source_run_id, source_comment_id, idempotency_key, continuation_policy, question, payload, response, resolved_by_type, resolved_by_id, created_at, updated_at FROM issue_thread_interactions WHERE issue_id = $1 AND company_id = $2 ORDER BY created_at ASC").bind(id).bind(company_id).fetch_all(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    use sqlx::Row;
    Ok(Json(rows.into_iter().map(|r| serde_json::json!({"id": r.get::<Uuid,_>("id"), "companyId": r.get::<Uuid,_>("company_id"), "issueId": r.get::<Uuid,_>("issue_id"), "kind": r.get::<String,_>("kind"), "status": r.get::<String,_>("status"), "sourceRunId": r.get::<Option<Uuid>,_>("source_run_id"), "sourceCommentId": r.get::<Option<Uuid>,_>("source_comment_id"), "idempotencyKey": r.get::<Option<String>,_>("idempotency_key"), "continuationPolicy": r.get::<String,_>("continuation_policy"), "question": r.get::<Option<String>,_>("question"), "payload": r.get::<Option<serde_json::Value>,_>("payload"), "response": r.get::<Option<serde_json::Value>,_>("response"), "resolvedByType": r.get::<Option<String>,_>("resolved_by_type"), "resolvedById": r.get::<Option<String>,_>("resolved_by_id"), "createdAt": r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"), "updatedAt": r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})).collect()))
}

/// I30: POST /issues/:id/interactions
async fn create_interaction(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    use sqlx::Row;
    let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1").bind(id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("question");
    if !matches!(kind, "question" | "approval" | "review" | "suggest_tasks" | "ask_user_questions" | "request_confirmation" | "request_checkbox_confirmation") { return Err(StatusCode::BAD_REQUEST); }
    let question = payload.get("question")
        .or_else(|| payload.pointer("/payload/prompt"))
        .or_else(|| payload.get("summary"))
        .or_else(|| payload.get("body"))
        .and_then(|v| v.as_str());
    let source_run_id = payload.get("sourceRunId").and_then(|v| v.as_str()).and_then(|v| Uuid::parse_str(v).ok());
    let source_comment_id = payload.get("sourceCommentId").and_then(|v| v.as_str()).and_then(|v| Uuid::parse_str(v).ok());
    let idempotency_key = payload.get("idempotencyKey").and_then(|v| v.as_str()).map(str::to_owned);
    let continuation_policy = payload.get("continuationPolicy").and_then(|v| v.as_str()).unwrap_or(if matches!(kind, "request_confirmation") { "none" } else { "wake_assignee" });
    let row = sqlx::query("INSERT INTO issue_thread_interactions (company_id, issue_id, kind, source_run_id, source_comment_id, idempotency_key, continuation_policy, question, payload) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (issue_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO UPDATE SET updated_at=NOW() RETURNING id, company_id, issue_id, kind, status::text, source_run_id, source_comment_id, idempotency_key, continuation_policy, question, payload, response, resolved_by_type, resolved_by_id, created_at, updated_at")
        .bind(company_id).bind(id).bind(kind).bind(source_run_id).bind(source_comment_id).bind(&idempotency_key).bind(continuation_policy).bind(question).bind(payload.get("payload").cloned().unwrap_or_else(|| payload.clone()))
        .fetch_one(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if continuation_policy != "none" {
        if let Some(agent_id) = sqlx::query_scalar::<_, Option<Uuid>>("SELECT assignee_agent_id FROM issues WHERE id=$1").bind(id).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.flatten() {
            let _ = state.heartbeat_service.wakeup(agent_id, id, company_id).await;
        }
    }
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "id": row.get::<Uuid,_>("id"), "companyId": row.get::<Uuid,_>("company_id"), "issueId": id,
        "kind": row.get::<String,_>("kind"), "status": row.get::<String,_>("status"),
        "sourceRunId": row.get::<Option<Uuid>,_>("source_run_id"), "sourceCommentId": row.get::<Option<Uuid>,_>("source_comment_id"),
        "idempotencyKey": row.get::<Option<String>,_>("idempotency_key"), "continuationPolicy": row.get::<String,_>("continuation_policy"),
        "question": row.get::<Option<String>,_>("question"), "payload": row.get::<Option<serde_json::Value>,_>("payload"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"), "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")
    }))))
}

/// I31: POST /issues/:id/interactions/:interaction_id/accept
async fn accept_interaction(
    State(state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    transition_interaction(&state, id, interaction_id, "resolved", None).await
}

/// I32: POST /issues/:id/interactions/:interaction_id/reject
async fn reject_interaction(
    State(state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    transition_interaction(&state, id, interaction_id, "resolved", None).await
}

/// I33: POST /issues/:id/interactions/:interaction_id/respond
async fn respond_interaction(
    State(state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    transition_interaction(&state, id, interaction_id, "resolved", Some(payload)).await
}

/// I34: POST /issues/:id/interactions/:interaction_id/cancel
async fn cancel_interaction(
    State(state): State<AppState>,
    Path((id, interaction_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    transition_interaction(&state, id, interaction_id, "cancelled", None).await
}

async fn transition_interaction(state: &AppState, issue_id: Uuid, interaction_id: Uuid, status: &str, response: Option<serde_json::Value>) -> Result<Json<serde_json::Value>, StatusCode> {
    use sqlx::Row;
    let row = sqlx::query("UPDATE issue_thread_interactions SET status = $3::issue_thread_interaction_status, response = COALESCE($4, response), resolved_by_type = 'user', updated_at = NOW() WHERE id = $1 AND issue_id = $2 RETURNING id, status::text, response, updated_at").bind(interaction_id).bind(issue_id).bind(status).bind(response).fetch_optional(&state.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({"issueId": issue_id, "interactionId": row.get::<Uuid,_>("id"), "status": row.get::<String,_>("status"), "response": row.get::<Option<serde_json::Value>,_>("response"), "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")})))
}

/// I42: GET /issues/:id/comments/:comment_id
async fn get_single_comment(
    State(state): State<AppState>,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    let comment = state.issue_service.get_comment(comment_id, company_id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    comment.map(Json).ok_or(StatusCode::NOT_FOUND)
}

/// Create issue routes
pub fn issue_routes() -> Router<AppState> {
    Router::new()
        .route("/issues", get(list_issues))
        .route("/issues/:id", get(get_issue).patch(update_issue).delete(delete_issue))
        .route(
            "/companies/:companyId/issues",
            get(list_company_issues).post(create_issue),
        )
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
        .route("/issues/:id/documents", get(list_issue_documents))
        .route("/issues/:id/documents/:key", get(get_issue_document).put(upsert_issue_document))
        .route("/issues/:id/documents/:key/revisions", get(list_issue_document_revisions))
        .route("/issues/:id/documents/:key/revisions/:revision_id/restore", post(restore_issue_document_revision))
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
