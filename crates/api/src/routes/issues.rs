use crate::app_state::AppState;
use crate::extractors::IssueId;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use models::{CreateIssueInput, Issue, IssuePriority, IssueStatus, UpdateIssueInput};
use services::auth::AuthorizationActor;
use services::{
    CheckoutInput, CrossIssueInfluenceKind, CrossIssueInfluenceLimitService,
    DefaultCrossIssueInfluenceLimitService, IssueQueryFilter, Pagination, ReleaseInput,
};

// Issue relations handlers
mod issue_relations_handlers {
    use super::*;

    /// GET /issues/:issue_id/relations
    pub async fn get_issue_relations(
        State(state): State<AppState>,
        Path(issue_id): Path<Uuid>,
    ) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
        let relation_service =
            services::issue_relation_service::IssueRelationService::new(state.pool.clone());

        let relations = relation_service
            .get_relation_summaries(issue_id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(Json(serde_json::json!({
            "blockedBy": relations.blocked_by,
            "blocks": relations.blocks,
        })))
    }

    /// POST /issues/:issue_id/relations/blocked-by
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UpdateBlockedByInput {
        pub blocked_by_issue_ids: Vec<Uuid>,
    }

    pub async fn update_blocked_by_relations(
        State(state): State<AppState>,
        Path(issue_id): Path<Uuid>,
        Json(input): Json<UpdateBlockedByInput>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        // Get issue to retrieve company_id
        let issue: Issue = sqlx::query_as("SELECT * FROM issues WHERE id = $1")
            .bind(issue_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, format!("Issue not found: {}", e)))?;

        let relation_service =
            services::issue_relation_service::IssueRelationService::new(state.pool.clone());

        relation_service
            .update_blocked_by_relations(
                issue.company_id,
                issue_id,
                input.blocked_by_issue_ids,
                None, // TODO: Get from auth context
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(StatusCode::OK)
    }
}

use issue_relations_handlers::{get_issue_relations, update_blocked_by_relations};

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
    participant_agent_id: Option<Uuid>,
    // Paperclip accepts the current-user sentinel `me` as well as a UUID for
    // these filters. Keep the wire type as a string and resolve it only after
    // authentication has supplied the current actor.
    touched_by_user_id: Option<String>,
    inbox_archived_by_user_id: Option<String>,
    unread_for_user_id: Option<Uuid>,
    label_id: Option<Uuid>,
    execution_workspace_id: Option<Uuid>,
    origin_kind: Option<String>,
    origin_id: Option<String>,
}

const TASK_WATCHDOG_PRODUCT_BUG_ORIGIN_KIND: &str = "task_watchdog_product_bug";

fn issue_service_status(error: &str) -> StatusCode {
    if error.starts_with("Not found:") {
        StatusCode::NOT_FOUND
    } else if error.starts_with("Invalid input:")
        || error.starts_with("Validation error:")
        || error.starts_with("Bad request:")
    {
        StatusCode::UNPROCESSABLE_ENTITY
    } else if error.starts_with("Conflict:") {
        StatusCode::CONFLICT
    } else if error.starts_with("Forbidden:") {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

fn resolve_user_filter(
    value: Option<&str>,
    actor: &AuthorizationActor,
) -> Result<Option<Uuid>, StatusCode> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    if value.eq_ignore_ascii_case("me") {
        return match actor {
            AuthorizationActor::Board { user_id, .. } => Ok(Some(*user_id)),
            AuthorizationActor::Agent {
                on_behalf_of_user_id: Some(user_id),
                ..
            } => Ok(Some(*user_id)),
            AuthorizationActor::Agent { .. } | AuthorizationActor::None => {
                Err(StatusCode::FORBIDDEN)
            }
        };
    }

    Uuid::parse_str(value)
        .map(Some)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

struct WatchdogDiscoveryScope {
    watchdog_id: Uuid,
    watched_issue_id: Uuid,
    watchdog_issue_id: Option<Uuid>,
    stop_fingerprint: Option<String>,
}

fn context_string(context: Option<&Value>, key: &str) -> Option<String> {
    context
        .and_then(Value::as_object)
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn resolve_watchdog_discovery_scope(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    requested: bool,
) -> Result<Option<WatchdogDiscoveryScope>, StatusCode> {
    if !requested {
        return Ok(None);
    }
    let (agent_id, run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id,
            run_id: Some(run_id),
            company_id: actor_company_id,
            ..
        } if *actor_company_id == company_id => (*agent_id, *run_id),
        _ => return Err(StatusCode::FORBIDDEN),
    };

    let run = sqlx::query(
        "SELECT company_id, agent_id, context_snapshot FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::FORBIDDEN)?;
    let run_company_id: Uuid = run
        .try_get("company_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let run_agent_id: Uuid = run
        .try_get("agent_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if run_company_id != company_id || run_agent_id != agent_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let context: Option<Value> = run
        .try_get("context_snapshot")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let task_watchdog = context
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|object| object.get("taskWatchdog"))
        .and_then(Value::as_object);
    let watched_issue_raw = task_watchdog
        .and_then(|object| object.get("watchedIssueId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| context_string(context.as_ref(), "watchedIssueId"))
        .or_else(|| context_string(context.as_ref(), "issueId"))
        .or_else(|| context_string(context.as_ref(), "taskId"));
    let watched_issue_id = watched_issue_raw
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let stop_fingerprint = task_watchdog
        .and_then(|object| object.get("stopFingerprint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let stop_fingerprint =
        stop_fingerprint.or_else(|| context_string(context.as_ref(), "stopFingerprint"));

    let watchdog = sqlx::query(
        "SELECT id, issue_id, watchdog_issue_id FROM issue_watchdogs
         WHERE company_id = $1 AND watchdog_agent_id = $2 AND status = 'active'
           AND (issue_id = $3 OR watchdog_issue_id = $3)",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(watched_issue_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::FORBIDDEN)?;
    Ok(Some(WatchdogDiscoveryScope {
        watchdog_id: watchdog
            .try_get("id")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        watched_issue_id: watchdog
            .try_get("issue_id")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        watchdog_issue_id: watchdog
            .try_get("watchdog_issue_id")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        stop_fingerprint,
    }))
}

fn append_watchdog_discovery_context(
    description: Option<String>,
    source_issue: &Issue,
    watchdog_issue: Option<&Issue>,
    evidence_markdown: Option<&str>,
    stop_fingerprint: Option<&str>,
    run_id: Uuid,
) -> String {
    let source_ref = source_issue
        .identifier
        .clone()
        .unwrap_or_else(|| source_issue.id.to_string());
    let mut lines = vec![
        "## Watchdog Discovery".to_string(),
        String::new(),
        "Kind: `product_bug`".to_string(),
        format!("Watched source issue: `{source_ref}`"),
    ];
    if let Some(issue) = watchdog_issue {
        let reference = issue
            .identifier
            .clone()
            .unwrap_or_else(|| issue.id.to_string());
        lines.push(format!("Watchdog issue: `{reference}`"));
    }
    if let Some(fingerprint) = stop_fingerprint.filter(|value| !value.is_empty()) {
        lines.push(format!("Stopped fingerprint: `{fingerprint}`"));
    }
    lines.push(format!("Watchdog run: `{run_id}`"));
    if let Some(evidence) = evidence_markdown.filter(|value| !value.trim().is_empty()) {
        lines.push(String::new());
        lines.push("Evidence:".to_string());
        lines.push(evidence.trim().to_string());
    }
    let context = lines.join("\n");
    match description.filter(|value| !value.trim().is_empty()) {
        Some(existing) => format!("{}\n\n{}", existing.trim(), context),
        None => context,
    }
}

fn parse_issue_statuses(value: Option<&str>) -> Option<Vec<IssueStatus>> {
    let values = value?.split(',').map(str::trim).filter(|v| !v.is_empty());
    let parsed: Vec<_> = values
        .filter_map(|value| {
            serde_json::from_value::<IssueStatus>(serde_json::Value::String(value.to_owned())).ok()
        })
        .collect();
    Some(parsed).filter(|values| !values.is_empty())
}

fn parse_issue_priorities(value: Option<&str>) -> Option<Vec<IssuePriority>> {
    let values = value?.split(',').map(str::trim).filter(|v| !v.is_empty());
    let parsed: Vec<_> = values
        .filter_map(|value| {
            serde_json::from_value::<IssuePriority>(serde_json::Value::String(value.to_owned()))
                .ok()
        })
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
    IssueId(issue_id): IssueId,
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
    Path((issue_id, key)): Path<(String, String)>,
) -> Result<Json<IssueDocumentResponse>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
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
    if key.is_empty()
        || key.len() > 64
        || !key
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(key)
}

/// PUT /issues/:id/documents/:key — Create or update an issue document.
async fn upsert_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let content = payload
        .get("body")
        .or_else(|| payload.get("content"))
        .and_then(|value| value.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if content.len() > 524_288 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let content_type = if payload
        .get("format")
        .and_then(|value| value.as_str())
        .unwrap_or("markdown")
        == "markdown"
    {
        "text/markdown"
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing = sqlx::query(
        "SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2 FOR UPDATE",
    )
    .bind(issue_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
        sqlx::query(
            "UPDATE documents SET content=$2, content_type=$3, updated_at=NOW() WHERE id=$1",
        )
        .bind(document_id)
        .bind(content)
        .bind(content_type)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    .bind(document_id)
    .bind(revision_number)
    .bind(content)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "id": document_id, "issueId": issue_id, "key": key, "content": content,
            "body": content, "contentType": content_type, "format": "markdown", "revisionNumber": revision_number,
        })),
    ))
}

/// GET /issues/:id/documents/:key/revisions — List issue document revisions.
async fn list_issue_document_revisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key)): Path<(String, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
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
    Path((issue_id, raw_key, revision_id)): Path<(String, String, Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    let content: String =
        sqlx::query_scalar("SELECT content FROM document_revisions WHERE id=$1 AND document_id=$2")
            .bind(revision_id)
            .bind(document_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
    let revision: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1",
    )
    .bind(document_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE documents SET content=$2, updated_at=NOW() WHERE id=$1")
        .bind(document_id)
        .bind(&content)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    .bind(document_id)
    .bind(revision)
    .bind(&content)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({"restored": true, "issueId": issue_id, "key": key, "revisionId": revision_id, "revisionNumber": revision, "content": content, "body": content}),
    ))
}

/// GET /issues/:id/documents/:key/annotations
async fn get_issue_document_annotations(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let _company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let status = params.get("status").map(|s| s.as_str()).unwrap_or("all");
    let include_comments = params
        .get("includeComments")
        .map(|s| s == "true")
        .unwrap_or(true);

    let mut query = "SELECT id, status, anchor_state, selected_text, anchor_selector, anchor_confidence, \
                     prefix_text, suffix_text, normalized_start, normalized_end, markdown_start, markdown_end, \
                     original_revision_number, current_revision_number, created_at, updated_at \
                     FROM document_annotation_threads \
                     WHERE issue_id=$1 AND document_id=$2".to_string();

    if status != "all" {
        query.push_str(&format!(" AND status='{}'", status));
    }
    query.push_str(" ORDER BY updated_at DESC");

    let rows = sqlx::query(&query)
        .bind(issue_id)
        .bind(document_id)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let thread_id: Uuid = row.get("id");
        let mut thread_json = serde_json::json!({
            "id": thread_id,
            "threadId": thread_id,
            "issueId": issue_id,
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
                "SELECT id, body, author_type, author_agent_id, author_user_id, created_at, updated_at \
                 FROM document_annotation_comments WHERE thread_id=$1 ORDER BY created_at ASC"
            )
            .bind(thread_id)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let comments_json: Vec<serde_json::Value> = comments
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.get::<Uuid, _>("id"),
                        "body": c.get::<String, _>("body"),
                        "authorType": c.get::<String, _>("author_type"),
                        "authorAgentId": c.get::<Option<Uuid>, _>("author_agent_id"),
                        "authorUserId": c.get::<Option<String>, _>("author_user_id"),
                        "createdAt": c.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                        "updatedAt": c.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                    })
                })
                .collect();

            thread_json["comments"] = serde_json::json!(comments_json);
        }

        result.push(thread_json);
    }

    Ok(Json(result))
}

/// GET /issues/:id/documents/:key/annotations/:thread_id
async fn get_issue_document_annotation_thread(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key, thread_id)): Path<(String, String, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let _company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let row = sqlx::query(
        "SELECT id, status, anchor_state, selected_text, anchor_selector, anchor_confidence, \
         prefix_text, suffix_text, normalized_start, normalized_end, markdown_start, markdown_end, \
         original_revision_number, current_revision_number, created_at, updated_at \
         FROM document_annotation_threads \
         WHERE id=$1 AND issue_id=$2 AND document_id=$3",
    )
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let comments = sqlx::query(
        "SELECT id, body, author_type, author_agent_id, author_user_id, created_at, updated_at \
         FROM document_annotation_comments WHERE thread_id=$1 ORDER BY created_at ASC",
    )
    .bind(thread_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let comments_json: Vec<serde_json::Value> = comments
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.get::<Uuid, _>("id"),
                "body": c.get::<String, _>("body"),
                "authorType": c.get::<String, _>("author_type"),
                "authorAgentId": c.get::<Option<Uuid>, _>("author_agent_id"),
                "authorUserId": c.get::<Option<String>, _>("author_user_id"),
                "createdAt": c.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": c.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "id": thread_id,
        "threadId": thread_id,
        "issueId": issue_id,
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

/// POST /issues/:id/documents/:key/annotations
async fn create_issue_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let selected_text = payload
        .get("selectedText")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let selector = payload
        .get("anchorSelector")
        .or_else(|| payload.get("selector"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let body = payload
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // 获取当前 revision number
    let revision_number: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision_number), 0) FROM document_revisions WHERE document_id=$1",
    )
    .bind(document_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let thread_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_threads \
         (company_id, issue_id, document_id, document_key, selected_text, anchor_selector, \
          original_revision_number, current_revision_number, normalized_start, normalized_end, \
          markdown_start, markdown_end, prefix_text, suffix_text) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, 0, 0, 0, 0, '', '') \
         RETURNING id",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(&key)
    .bind(selected_text)
    .bind(&selector)
    .bind(revision_number)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let author_user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(*user_id),
        _ => None,
    };

    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments \
         (company_id, thread_id, issue_id, document_id, body, author_type, author_user_id) \
         VALUES ($1, $2, $3, $4, $5, 'user', $6) \
         RETURNING id",
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(body)
    .bind(author_user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "threadId": thread_id,
            "id": thread_id,
            "issueId": issue_id,
            "documentKey": key,
            "status": "open",
            "selectedText": selected_text,
            "anchorSelector": selector,
            "comments": [{
                "id": comment_id,
                "body": body,
                "authorType": "user",
                "authorUserId": author_user_id,
            }],
        })),
    ))
}

/// POST /issues/:id/documents/:key/annotations/:thread_id/reply
async fn reply_issue_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key, thread_id)): Path<(String, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let body = payload
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let author_user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => Some(*user_id),
        _ => None,
    };

    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments \
         (company_id, thread_id, issue_id, document_id, body, author_type, author_user_id) \
         SELECT $1, $2, $3, $4, $5, 'user', $6 \
         WHERE EXISTS (SELECT 1 FROM document_annotation_threads WHERE id=$2 AND issue_id=$3 AND document_id=$4) \
         RETURNING id"
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(body)
    .bind(author_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    sqlx::query("UPDATE document_annotation_threads SET updated_at=NOW() WHERE id=$1")
        .bind(thread_id)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": comment_id,
            "threadId": thread_id,
            "issueId": issue_id,
            "documentKey": key,
            "body": body,
        })),
    ))
}

/// PATCH /issues/:id/documents/:key/annotations/:thread_id
async fn update_issue_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key, thread_id)): Path<(String, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let _company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open");
    if !matches!(status, "open" | "resolved") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let updated = sqlx::query(
        "UPDATE document_annotation_threads \
         SET status=$1, resolved_at=CASE WHEN $1='resolved' THEN NOW() ELSE NULL END, updated_at=NOW() \
         WHERE id=$2 AND issue_id=$3 AND document_id=$4 \
         RETURNING id, status, updated_at"
    )
    .bind(status)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "threadId": updated.get::<Uuid, _>("id"),
        "issueId": issue_id,
        "documentKey": key,
        "status": updated.get::<String, _>("status"),
        "updatedAt": updated.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })))
}

/// POST /issues/:id/documents/:key/lock - Lock issue document
async fn lock_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    scoped_issue_company(&state, &actor, issue_id).await?;

    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    sqlx::query(
        "UPDATE documents SET locked_by_type=$2, locked_by_id=$3, locked_at=NOW(), locked_run_id=$4 \
         WHERE id=$1 AND locked_at IS NULL"
    )
    .bind(document_id)
    .bind(payload.get("actorType").and_then(|v| v.as_str()).unwrap_or("user"))
    .bind(payload.get("actorId").and_then(|v| v.as_str()).and_then(|v| Uuid::parse_str(v).ok()))
    .bind(payload.get("runId").and_then(|v| v.as_str()).and_then(|v| Uuid::parse_str(v).ok()))
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::CONFLICT)?;

    Ok(Json(serde_json::json!({
        "issueId": issue_id,
        "key": key,
        "locked": true,
        "lockedBy": payload
    })))
}

/// POST /issues/:id/documents/:key/unlock - Unlock issue document
async fn unlock_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    scoped_issue_company(&state, &actor, issue_id).await?;

    let document_id: Uuid =
        sqlx::query_scalar("SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

    sqlx::query(
        "UPDATE documents SET locked_by_type=NULL, locked_by_id=NULL, locked_at=NULL, locked_run_id=NULL \
         WHERE id=$1"
    )
    .bind(document_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "issueId": issue_id,
        "key": key,
        "unlocked": true
    })))
}

/// DELETE /issues/:id/documents/:key - Delete issue document
async fn delete_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;

    sqlx::query(
        "DELETE FROM issue_documents WHERE issue_id=$1 AND key=$2 AND EXISTS \
         (SELECT 1 FROM issues WHERE id=$1 AND company_id=$3)",
    )
    .bind(issue_id)
    .bind(key)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

/// 解析 Issue ID：支持 UUID 或 identifier
async fn resolve_issue_id(pool: &sqlx::PgPool, reference: &str) -> Result<Uuid, StatusCode> {
    // 尝试解析为 UUID
    if let Ok(uuid) = Uuid::parse_str(reference) {
        return Ok(uuid);
    }

    // 作为 identifier 查询
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM issues WHERE identifier = $1")
        .bind(reference)
        .fetch_optional(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn issue_company_id(state: &AppState, issue_id: Uuid) -> Result<Uuid, StatusCode> {
    sqlx::query_scalar("SELECT company_id FROM issues WHERE id=$1")
        .bind(issue_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn scoped_issue_company(
    state: &AppState,
    actor: &AuthorizationActor,
    issue_id: Uuid,
) -> Result<Uuid, StatusCode> {
    // Paperclip resolves the resource first and authorizes against the
    // resource's company. This is important for local-trusted Board actors:
    // their company_id is an instance-level sentinel, not the issue's company.
    let company_id = issue_company_id(state, issue_id).await?;
    crate::routes::assert_company_access(actor, company_id, true)?;
    Ok(company_id)
}

/// GET /issues - List all issues
async fn list_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = actor.company_id().ok_or(StatusCode::FORBIDDEN)?;
    let touched_by_user_id = resolve_user_filter(query.touched_by_user_id.as_deref(), &actor)?;
    let inbox_archived_by_user_id =
        resolve_user_filter(query.inbox_archived_by_user_id.as_deref(), &actor)?;

    let filter = IssueQueryFilter {
        status: parse_issue_statuses(query.status.as_deref()),
        priority: parse_issue_priorities(query.priority.as_deref()),
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: query.q.clone().filter(|value| !value.trim().is_empty()),
        participant_agent_id: query.participant_agent_id,
        touched_by_user_id,
        inbox_archived_by_user_id,
        unread_for_user_id: query.unread_for_user_id,
        label_id: query.label_id,
        execution_workspace_id: query.execution_workspace_id,
        origin_kind: query.origin_kind.clone(),
        origin_id: query.origin_id.clone(),
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
    let local_instance_admin = matches!(
        actor,
        AuthorizationActor::Board {
            source: services::auth::ActorSource::LocalImplicit,
            is_instance_admin: true,
            ..
        }
    );
    let (query, bind_company_id) = if local_instance_admin {
        (
            "SELECT id FROM issues WHERE (id::text = $1 OR identifier = $1)",
            None,
        )
    } else {
        (
            "SELECT id FROM issues WHERE company_id = $1 AND (id::text = $2 OR identifier = $2)",
            Some(company_id),
        )
    };
    let mut issue_query = sqlx::query_scalar::<_, Uuid>(query);
    if let Some(company_id) = bind_company_id {
        issue_query = issue_query.bind(company_id);
    }
    issue_query = issue_query.bind(&reference);
    let id = issue_query
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let issue_company_id = if local_instance_admin {
        sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM issues WHERE id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        company_id
    };

    service
        .get(id, issue_company_id)
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
    let requested_discovery = input.watchdog_discovery.take();
    if let Some(discovery) = requested_discovery.as_ref() {
        let valid = discovery
            .as_object()
            .and_then(|object| object.get("kind"))
            .and_then(Value::as_str)
            == Some("product_bug");
        if !valid {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    let discovery_scope =
        resolve_watchdog_discovery_scope(&state, &actor, company_id, requested_discovery.is_some())
            .await?;
    if let (Some(discovery), Some(scope)) = (requested_discovery.as_ref(), discovery_scope.as_ref())
    {
        let source_issue = state
            .issue_service
            .get(scope.watched_issue_id, company_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        let watchdog_issue = match scope.watchdog_issue_id {
            Some(id) => state
                .issue_service
                .get(id, company_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            None => None,
        };
        let run_id = match actor {
            AuthorizationActor::Agent {
                run_id: Some(run_id),
                ..
            } => run_id,
            _ => return Err(StatusCode::FORBIDDEN),
        };
        input.parent_id = None;
        if input.project_id.is_none() {
            input.project_id = source_issue.project_id;
        }
        if input.goal_id.is_none() {
            input.goal_id = source_issue.goal_id;
        }
        if input.billing_code.is_none() {
            input.billing_code = source_issue.billing_code.clone();
        }
        input.description = Some(append_watchdog_discovery_context(
            input.description.take(),
            &source_issue,
            watchdog_issue.as_ref(),
            discovery.get("evidenceMarkdown").and_then(Value::as_str),
            scope.stop_fingerprint.as_deref(),
            run_id,
        ));
        input.origin_kind = Some(TASK_WATCHDOG_PRODUCT_BUG_ORIGIN_KIND.to_string());
        input.origin_id = Some(source_issue.id.to_string());
        input.origin_run_id = Some(run_id);
        input.origin_fingerprint = Some(format!(
            "{}:{}:{}",
            TASK_WATCHDOG_PRODUCT_BUG_ORIGIN_KIND, source_issue.id, run_id
        ));
        input.watchdog_discovery_audit = Some(models::WatchdogDiscoveryAuditInput {
            actor_id: actor.principal_id().ok_or(StatusCode::FORBIDDEN)?,
            source_issue_id: scope.watched_issue_id,
            watchdog_issue_id: scope.watchdog_issue_id,
            watchdog_id: scope.watchdog_id,
            stop_fingerprint: scope.stop_fingerprint.clone(),
        });
    }
    let requested_watchdog = input.watchdog.clone();
    if let Some(harness_kind) = input.harness_kind.as_deref() {
        if harness_kind != "skill_test" {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if let Some(work_mode) = input.work_mode.as_ref() {
            if !matches!(work_mode, models::IssueWorkMode::SkillTest) {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        } else {
            input.work_mode = Some(models::IssueWorkMode::SkillTest);
        }
    }
    if let Some(watchdog) = &requested_watchdog {
        let watchdog_agent = state
            .agent_service
            .get_by_id(watchdog.agent_id)
            .await
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
        if watchdog_agent.company_id != company_id {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }
    input.watchdog_created_by_run_id = match &actor {
        AuthorizationActor::Agent { run_id, .. } => *run_id,
        _ => None,
    };

    // ✅ Paperclip pattern: Force override creator fields from actor (issues.ts:6963-6968)
    // Sanitize: strip any createdByUserId if actor is Agent (prevents spoofing)
    if matches!(actor, AuthorizationActor::Agent { .. }) {
        input.created_by_user_id = None;
    }

    // Force set creator fields based on actor type
    match &actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => {
            input.created_by_agent_id = Some(*agent_id);
            input.origin_run_id = *run_id;
            // Set origin_kind only if not already set by watchdog discovery
            if input.origin_kind.is_none() {
                input.origin_kind = Some("agent".to_string());
            }
        }
        AuthorizationActor::Board { user_id, .. } => {
            input.created_by_user_id = Some(*user_id);
            // Set origin_kind only if not already set
            if input.origin_kind.is_none() {
                input.origin_kind = Some("manual".to_string());
            }
        }
        AuthorizationActor::None => {
            // Anonymous actor - should not reach here due to auth middleware
        }
    }

    // Paperclip takes the company scope from the URL. The body must not need
    // to repeat companyId, and the path is authoritative if it is supplied.
    input.company_id = company_id;
    let service = state.issue_service.clone();
    let created = service.create(input).await.map_err(|error| {
        tracing::error!(error = %error, company_id = %company_id, "issue creation failed");
        issue_service_status(&error)
    })?;

    // Queue issue assignment wakeup (matches paperclip: routes/issues.ts:7054-7062)
    // Skip if no assignee or backlog status
    if let Some(assignee_agent_id) = created.issue.assignee_agent_id {
        if !matches!(created.issue.status, models::IssueStatus::Backlog) {
            let wakeup_service =
                services::IssueAssignmentWakeupService::new(state.heartbeat_service.clone());

            let wakeup_input = services::issue_assignment_wakeup::QueueWakeupInput {
                company_id,
                issue_id: created.issue.id,
                assignee_agent_id: Some(assignee_agent_id),
                status: format!("{}", created.issue.status),
                reason: "issue_assigned".to_string(),
                mutation: "create".to_string(),
                context_source: "issue.create".to_string(),
                requested_by_actor_type: Some(actor.actor_type().to_string()),
                requested_by_actor_id: actor.principal_id(),
                rethrow_on_error: false, // Swallow error to avoid blocking response
            };

            if let Err(e) = wakeup_service.queue_wakeup(wakeup_input).await {
                tracing::warn!(
                    error = %e,
                    issue_id = %created.issue.id,
                    assignee_agent_id = %assignee_agent_id,
                    "Failed to wake assignee on issue creation"
                );
            }
        }
    }

    // Re-read so the response includes the persisted watchdog projection.
    let response_issue = state
        .issue_service
        .get(created.issue.id, company_id)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, company_id = %company_id, "issue creation failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(created.issue);
    Ok(Json(response_issue))
}

/// GET /companies/:companyId/issues - List issues for a company
async fn list_company_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    crate::routes::assert_company_access(&actor, company_id, true)?;
    let touched_by_user_id = resolve_user_filter(query.touched_by_user_id.as_deref(), &actor)?;
    let inbox_archived_by_user_id =
        resolve_user_filter(query.inbox_archived_by_user_id.as_deref(), &actor)?;
    let filter = IssueQueryFilter {
        status: parse_issue_statuses(query.status.as_deref()),
        priority: parse_issue_priorities(query.priority.as_deref()),
        assignee_agent_id: query.assignee_agent_id,
        assignee_user_id: query.assignee_user_id,
        project_id: query.project_id,
        parent_id: None,
        goal_id: None,
        search_query: query.q.clone().filter(|value| !value.trim().is_empty()),
        participant_agent_id: query.participant_agent_id,
        touched_by_user_id,
        inbox_archived_by_user_id,
        unread_for_user_id: query.unread_for_user_id,
        label_id: query.label_id,
        execution_workspace_id: query.execution_workspace_id,
        origin_kind: query.origin_kind.clone(),
        origin_id: query.origin_id.clone(),
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
    IssueId(id): IssueId,
    Json(mut input): Json<UpdateIssueInput>,
) -> Result<Json<Issue>, StatusCode> {
    // Paperclip first loads the issue and uses its companyId for the mutation
    // authorization check. Do the same here instead of passing the placeholder
    // nil UUID used by the older route implementations.
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    if let Some(harness_kind) = input.harness_kind.as_deref() {
        if harness_kind != "skill_test" {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if let Some(work_mode) = input.work_mode.as_ref() {
            if !matches!(work_mode, models::IssueWorkMode::SkillTest) {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        } else {
            input.work_mode = Some(models::IssueWorkMode::SkillTest);
        }
    } else if matches!(input.work_mode, Some(models::IssueWorkMode::SkillTest)) {
        input.harness_kind = Some("skill_test".to_string());
    }

    if let AuthorizationActor::Agent {
        agent_id,
        run_id: Some(run_id),
        company_id: actor_company_id,
        ..
    } = &actor
    {
        if *actor_company_id != company_id {
            return Err(StatusCode::FORBIDDEN);
        }
        let guard = DefaultCrossIssueInfluenceLimitService::new().with_pool(state.pool.clone());
        match guard
            .observe_influence(services::ObserveCrossIssueInfluenceInput {
                heartbeat_run_id: *run_id,
                company_id,
                agent_id: *agent_id,
                source_issue_id: id,
                target_issue_id: id,
                influence_kind: CrossIssueInfluenceKind::Update,
                actor_label: None,
                assignee_label: None,
                issue_identifier: None,
            })
            .await
        {
            Ok(_) => {}
            Err(services::InfluenceLimitError::LimitExceeded { .. }) => {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            Err(
                services::InfluenceLimitError::RunNotFound(_)
                | services::InfluenceLimitError::RunContextRequired,
            ) => {
                return Err(StatusCode::FORBIDDEN);
            }
            Err(services::InfluenceLimitError::DatabaseError(_)) => {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
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
            issue_service_status(&error)
        })
}

/// DELETE /issues/:id - Delete issue
async fn delete_issue(
    State(state): State<AppState>,
    IssueId(id): IssueId,
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
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    IssueId(id): IssueId,
    Json(mut input): Json<CheckoutInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    if let AuthorizationActor::Agent {
        agent_id, run_id, ..
    } = &actor
    {
        if input
            .agent_id
            .is_some_and(|requested| requested != *agent_id)
            || run_id.is_some_and(|current| current != input.checkout_run_id)
        {
            return Err(StatusCode::FORBIDDEN);
        }
        input.agent_id = Some(*agent_id);
    }
    let valid_run = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
           SELECT 1 FROM heartbeat_runs
          WHERE id = $1 AND company_id = $2 AND agent_id = $3
            AND status IN ('queued', 'running')
        )",
    )
    .bind(input.checkout_run_id)
    .bind(company_id)
    .bind(input.agent_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid_run {
        return Err(StatusCode::FORBIDDEN);
    }
    let checkout_agent_id = input.agent_id;
    let checkout_run_id = input.checkout_run_id;

    service
        .checkout(id, company_id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query(
        "UPDATE issues
            SET assignee_agent_id = $2, checkout_run_id = $3,
                execution_run_id = $3, updated_at = NOW()
          WHERE id = $1 AND company_id = $4",
    )
    .bind(id)
    .bind(checkout_agent_id)
    .bind(checkout_run_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// POST /issues/:id/release - Release issue
async fn release_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(input): Json<ReleaseInput>,
) -> Result<Json<Issue>, StatusCode> {
    let service = state.issue_service.clone();
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    if let AuthorizationActor::Agent { run_id, .. } = &actor {
        if run_id.is_some_and(|current| current != input.release_run_id) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    service
        .release(id, company_id, input)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query(
        "UPDATE issues
            SET checkout_run_id = NULL, execution_run_id = NULL,
                execution_locked_at = NULL, updated_at = NOW()
          WHERE id = $1 AND company_id = $2",
    )
    .bind(id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// POST /issues/:id/admin/force-release - Force release issue (admin only)
async fn force_release_issue(
    State(state): State<AppState>,
    IssueId(id): IssueId,
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
        .batch_update(
            company_id,
            input.issue_ids,
            input.status,
            input.priority,
            input.assignee_agent_id,
            input.assignee_user_id,
        )
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
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .get_cases(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I3: GET /issues/:id/active-run
async fn get_issue_active_run(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    crate::routes::assert_company_access(&actor, company_id, true)?;
    let issue = sqlx::query(
        "SELECT execution_run_id, assignee_agent_id, status::text AS status
           FROM issues WHERE id = $1 AND company_id = $2",
    )
    .bind(id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let execution_run_id: Option<Uuid> = issue
        .try_get("execution_run_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let assignee_agent_id: Option<Uuid> = issue
        .try_get("assignee_agent_id")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status: String = issue
        .try_get("status")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Paperclip first follows the issue's execution lock, then falls back to
    // the assignee's live run for an in-progress issue. In both cases the run
    // must still be queued/running and must point back to this issue.
    let run = if let Some(run_id) = execution_run_id {
        sqlx::query(
            "SELECT hr.id, hr.status::text AS status, hr.invocation_source,
                    hr.started_at, hr.finished_at, hr.created_at, hr.agent_id,
                    a.name AS agent_name, a.adapter_type
               FROM heartbeat_runs hr
               JOIN agents a ON a.id = hr.agent_id
              WHERE hr.id = $1 AND hr.company_id = $2
                AND hr.status IN ('queued', 'running')
                AND hr.context_snapshot->>'issueId' = $3",
        )
        .bind(run_id)
        .bind(company_id)
        .bind(id.to_string())
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        None
    };
    let run = if run.is_none() && status == "in_progress" {
        if let Some(agent_id) = assignee_agent_id {
            sqlx::query(
                "SELECT hr.id, hr.status::text AS status, hr.invocation_source,
                        hr.started_at, hr.finished_at, hr.created_at, hr.agent_id,
                        a.name AS agent_name, a.adapter_type
                   FROM heartbeat_runs hr
                   JOIN agents a ON a.id = hr.agent_id
                  WHERE hr.company_id = $1 AND hr.agent_id = $2
                    AND hr.status IN ('queued', 'running')
                    AND hr.context_snapshot->>'issueId' = $3
                  ORDER BY hr.created_at DESC LIMIT 1",
            )
            .bind(company_id)
            .bind(agent_id)
            .bind(id.to_string())
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        } else {
            None
        }
    } else {
        run
    };
    // The Paperclip contract is 200 with JSON null when the issue exists but
    // has no queued/running heartbeat run. 404 is reserved for a missing issue.
    let Some(run) = run else {
        return Ok(Json(serde_json::Value::Null));
    };
    Ok(Json(serde_json::json!({
        "id": run.try_get::<Uuid, _>("id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "status": run.try_get::<String, _>("status").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "invocationSource": run.try_get::<String, _>("invocation_source").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "startedAt": run.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "finishedAt": run.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "createdAt": run.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "agentId": run.try_get::<Uuid, _>("agent_id").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "agentName": run.try_get::<String, _>("agent_name").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "adapterType": run.try_get::<String, _>("adapter_type").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        "issueId": id,
        "outputSilence": null
    })))
}

/// I4: GET /issues/:id/live-runs
async fn get_issue_live_runs(
    State(state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .get_live_runs(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I6: GET /issues/:id/accepted-plan-decompositions
async fn list_plan_decompositions(
    State(state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .get_accepted_plan_decompositions(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I7: POST /issues/:id/accepted-plan-decompositions
async fn submit_plan_decomposition(
    State(state): State<AppState>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    let result = state
        .issue_service
        .submit_plan_decomposition(id, company_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I8: GET /issues/:id/approvals
async fn list_issue_approvals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    state
        .issue_service
        .get_approvals(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I9: POST /issues/:id/approvals
async fn create_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let result = state
        .issue_service
        .create_approval(id, company_id, payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// I10: DELETE /issues/:id/approvals/:approval_id
async fn delete_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, approval_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    state
        .issue_service
        .delete_approval(id, approval_id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I5: GET /issues/:id/work-products - List work products for issue
async fn list_work_products(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<models::issue_auxiliary::WorkProduct>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    state
        .work_product_service
        .list_work_products(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I6: POST /issues/:id/work-products - Create work product for issue
async fn create_work_product(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(input): Json<models::issue_auxiliary::CreateWorkProductInput>,
) -> Result<Json<models::issue_auxiliary::WorkProduct>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    state
        .work_product_service
        .create_work_product(id, company_id, input)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I7: GET /issues/:id/children - List child issues
async fn list_child_issues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<Issue>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    // Query child issues from database
    let children = sqlx::query_as::<_, Issue>(
        "SELECT * FROM issues WHERE parent_issue_id = $1 AND company_id = $2 ORDER BY created_at DESC"
    )
    .bind(id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(children))
}

/// I8: GET /issues/:id/transcript - Get issue transcript/conversation history
async fn get_issue_transcript(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;

    // Query activity log for transcript events
    let events = sqlx::query(
        "SELECT id, action, details, created_at, agent_id, user_id 
         FROM activity_log 
         WHERE entity_type = 'issue' AND entity_id = $1 AND company_id = $2
         AND action IN ('issue.comment', 'issue.status_changed', 'issue.assigned', 'issue.created')
         ORDER BY created_at ASC",
    )
    .bind(id.to_string())
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut transcript = Vec::new();
    for row in events {
        let action: String = row.try_get("action").unwrap_or_default();
        let details: Option<serde_json::Value> = row.try_get("details").ok();
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").unwrap_or_default();
        let agent_id: Option<Uuid> = row.try_get("agent_id").ok().flatten();
        let user_id: Option<Uuid> = row.try_get("user_id").ok().flatten();

        transcript.push(serde_json::json!({
            "action": action,
            "details": details,
            "createdAt": created_at,
            "agentId": agent_id,
            "userId": user_id,
        }));
    }

    Ok(Json(serde_json::json!({
        "issueId": id,
        "transcript": transcript,
    })))
}

/// I9: DELETE /issues/:id/relations/:relation_id - Delete specific issue relation
async fn delete_issue_relation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, relation_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;

    // Delete the relation
    let result = sqlx::query(
        "DELETE FROM issue_relations 
         WHERE id = $1 AND company_id = $2 AND (issue_id = $3 OR related_issue_id = $3)",
    )
    .bind(relation_id)
    .bind(company_id)
    .bind(issue_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// I11: POST /issues/:id/children
async fn create_child_issue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(parent_id): IssueId,
    Json(mut input): Json<CreateIssueInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let service = state.issue_service.clone();

    // ✅ Paperclip pattern: Force override creator fields from actor (issues.ts:7139-7146)
    // Sanitize: strip any createdByUserId if actor is Agent (prevents spoofing)
    if matches!(actor, AuthorizationActor::Agent { .. }) {
        input.created_by_user_id = None;
    }

    // Force set creator fields based on actor type
    match &actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => {
            input.created_by_agent_id = Some(*agent_id);
            input.origin_run_id = *run_id;
            // Set origin_kind only if not already set
            if input.origin_kind.is_none() {
                input.origin_kind = Some("agent".to_string());
            }
        }
        AuthorizationActor::Board { user_id, .. } => {
            input.created_by_user_id = Some(*user_id);
            // Set origin_kind only if not already set
            if input.origin_kind.is_none() {
                input.origin_kind = Some("manual".to_string());
            }
        }
        AuthorizationActor::None => {
            // Anonymous actor - should not reach here due to auth middleware
        }
    }

    let input_with_parent = CreateIssueInput {
        parent_id: Some(parent_id),
        ..input
    };
    let result = service
        .create(input_with_parent)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Queue issue assignment wakeup (matches Paperclip: issues.ts:7221-7229)
    if let Some(assignee_agent_id) = result.issue.assignee_agent_id {
        // Only wake if not backlog status
        if !matches!(result.issue.status, models::IssueStatus::Backlog) {
            let wakeup_service =
                services::IssueAssignmentWakeupService::new(state.heartbeat_service.clone());

            let wakeup_input = services::issue_assignment_wakeup::QueueWakeupInput {
                company_id: result.issue.company_id,
                issue_id: result.issue.id,
                assignee_agent_id: Some(assignee_agent_id),
                status: format!("{}", result.issue.status),
                reason: "issue_assigned".to_string(),
                mutation: "create".to_string(),
                context_source: "issue.child_create".to_string(),
                requested_by_actor_type: Some(actor.actor_type().to_string()),
                requested_by_actor_id: actor.principal_id(),
                rethrow_on_error: false, // Don't block child creation on wakeup failure
            };

            if let Err(e) = wakeup_service.queue_wakeup(wakeup_input).await {
                tracing::warn!(
                    error = %e,
                    issue_id = %result.issue.id,
                    parent_id = %parent_id,
                    assignee_agent_id = %assignee_agent_id,
                    "Failed to wake assignee on child issue creation"
                );
            }
        }
    }

    Ok((StatusCode::CREATED, Json(result.issue)))
}

/// I12: POST /issues/:id/read
async fn mark_issue_read(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
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
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .unmark_read(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I14: POST /issues/:id/inbox-archive
async fn archive_issue_inbox(
    State(state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .archive_inbox(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I15: DELETE /issues/:id/inbox-archive
async fn unarchive_issue_inbox(
    State(state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .unarchive_inbox(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I16: POST /issues/:id/monitor/check-now
async fn monitor_check_now(
    State(_state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(
        serde_json::json!({"issueId": id, "monitorCheckTriggered": true}),
    ))
}

/// I17: POST /issues/:id/scheduled-retry/retry-now
async fn scheduled_retry_now(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let row = sqlx::query(
        "SELECT r.agent_id, r.scheduled_retry_at, r.scheduled_retry_attempt,
                r.scheduled_retry_reason, r.error, a.name AS agent_name
         FROM heartbeat_runs r
         JOIN agents a ON a.id = r.agent_id AND a.company_id = r.company_id
         WHERE r.company_id = $1
           AND r.status = 'scheduled_retry'
           AND (r.context_snapshot->>'issueId' = $2 OR r.context_snapshot->>'taskId' = $2)
         ORDER BY r.scheduled_retry_at ASC NULLS LAST, r.created_at ASC
         LIMIT 1",
    )
    .bind(company_id)
    .bind(id.to_string())
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to find scheduled retry");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some(row) = row else {
        return Ok(Json(json!({
            "outcome": "no_scheduled_retry",
            "message": "No scheduled retry is waiting for this issue.",
            "scheduledRetry": null,
        })));
    };

    let agent_id = row.get::<Uuid, _>("agent_id");
    let scheduled_retry = json!({
        "status": "scheduled_retry",
        "agentId": agent_id,
        "agentName": row.get::<String, _>("agent_name"),
        "retryOfRunId": Value::Null,
        "scheduledRetryAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("scheduled_retry_at"),
        "scheduledRetryAttempt": row.get::<Option<i32>, _>("scheduled_retry_attempt").unwrap_or(0),
        "scheduledRetryReason": row.get::<Option<String>, _>("scheduled_retry_reason"),
        "error": row.get::<Option<String>, _>("error"),
    });

    let cancelled = state
        .heartbeat_service
        .cancel_scheduled_retry(
            agent_id,
            id,
            company_id,
            "Scheduled retry promoted by operator",
        )
        .await
        .map_err(|error| {
            tracing::error!(error = %error, issue_id = %id, agent_id = %agent_id, "failed to cancel scheduled retry");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !cancelled {
        return Ok(Json(json!({
            "outcome": "no_scheduled_retry",
            "message": "Scheduled retry was already handled.",
            "scheduledRetry": null,
        })));
    }
    state
        .heartbeat_service
        .wakeup(agent_id, id, company_id)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, issue_id = %id, agent_id = %agent_id, "failed to wake promoted retry");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "outcome": "promoted",
        "message": "Scheduled retry promoted and queued now.",
        "scheduledRetry": scheduled_retry,
    })))
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
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(
        serde_json::json!({"issueId": id, "externalObjectCount": 0}),
    ))
}

/// I20: POST /issues/:id/external-objects/refresh
async fn refresh_external_objects(
    State(_state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(
        serde_json::json!({"issueId": id, "refreshTriggered": true}),
    ))
}

/// I21: GET /issues/:id/file-resources/list
/// POST /issues/:id/file-resources/availability
async fn check_file_resource_availability(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(issue_id): IssueId,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let queries = payload
        .get("queries")
        .and_then(Value::as_array)
        .ok_or(StatusCode::BAD_REQUEST)?;
    if queries.len() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let project_id: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM issues WHERE id = $1 AND company_id = $2")
            .bind(issue_id)
            .bind(company_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut results = Vec::with_capacity(queries.len());
    for query in queries {
        let path = query.get("path").and_then(Value::as_str).unwrap_or("");
        let workspace = query
            .get("workspace")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let project_filter = query
            .get("projectId")
            .and_then(Value::as_str)
            .and_then(|v| Uuid::parse_str(v).ok());
        let workspace_filter = query
            .get("workspaceId")
            .and_then(Value::as_str)
            .and_then(|v| Uuid::parse_str(v).ok());
        let normalized_query = json!({
            "projectId": project_filter,
            "workspaceId": workspace_filter,
            "path": path,
            "workspace": workspace,
        });

        let invalid_path = path.is_empty()
            || path.starts_with('/')
            || path.starts_with('\\')
            || path.split(['/', '\\']).any(|part| {
                part == ".." || part.contains('\0') || part.chars().any(|c| c.is_control())
            });
        if invalid_path || !matches!(workspace, "auto" | "execution" | "project") {
            results.push(json!({
                "query": normalized_query,
                "openable": false,
                "unavailableReason": "invalid_path",
                "resource": null,
            }));
            continue;
        }

        let row = if workspace == "project" {
            None
        } else {
            sqlx::query(
            "SELECT 'execution_workspace' AS workspace_kind, ew.id, ew.project_id, ew.cwd, ew.name, ew.provider_type, p.name AS project_name
             FROM execution_workspaces ew
             JOIN projects p ON p.id = ew.project_id
             WHERE ew.company_id = $1
               AND ($2::uuid IS NULL OR ew.project_id = $2)
               AND ($3::uuid IS NULL OR ew.id = $3)
               AND $4 <> 'project'
               AND ew.status NOT IN ('closed', 'cleaned')
             UNION ALL
             SELECT 'project_workspace' AS workspace_kind, pw.id, pw.project_id, pw.config->>'cwd', pw.name, 'local_fs', p.name AS project_name
             FROM project_workspaces pw
             JOIN projects p ON p.id = pw.project_id
             WHERE p.company_id = $1
               AND ($2::uuid IS NULL OR pw.project_id = $2)
               AND ($3::uuid IS NULL OR pw.id = $3)
               AND $4 <> 'execution'
             ORDER BY id
             LIMIT 20",
        )
        .bind(company_id)
        .bind(project_filter.or(project_id))
        .bind(workspace_filter)
        .bind(workspace)
            .fetch_all(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .into_iter()
            .next()
        };

        let Some(row) = row else {
            results.push(json!({
                "query": normalized_query,
                "openable": false,
                "unavailableReason": "no_workspace",
                "resource": null,
            }));
            continue;
        };

        let workspace_id: Uuid = row.get("id");
        let root: Option<String> = row.try_get("cwd").ok().flatten();
        let Some(root) = root else {
            results.push(json!({ "query": normalized_query, "openable": false, "unavailableReason": "no_workspace", "resource": null }));
            continue;
        };
        let root_path = tokio::fs::canonicalize(&root).await.ok();
        let Some(root_path) = root_path else {
            results.push(json!({ "query": normalized_query, "openable": false, "unavailableReason": "workspace_unavailable", "resource": null }));
            continue;
        };
        let candidate = root_path.join(path);
        let canonical = tokio::fs::canonicalize(&candidate).await.ok();
        let Some(canonical) = canonical else {
            results.push(json!({ "query": normalized_query, "openable": false, "unavailableReason": "not_found", "resource": null }));
            continue;
        };
        if !canonical.starts_with(&root_path)
            || canonical
                .components()
                .any(|part| part.as_os_str() == ".git" || part.as_os_str() == ".env")
        {
            results.push(json!({ "query": normalized_query, "openable": false, "unavailableReason": "permission_denied", "resource": null }));
            continue;
        }
        let metadata = tokio::fs::metadata(&canonical)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let is_dir = metadata.is_dir();
        let extension = canonical.extension().and_then(|v| v.to_str()).unwrap_or("");
        let preview_kind = if is_dir {
            "unsupported"
        } else if ["png", "jpg", "jpeg", "gif", "webp"].contains(&extension) {
            "image"
        } else {
            "text"
        };
        let project_name: String = row.get("project_name");
        let resource = json!({
            "kind": if is_dir { "directory" } else { "file" },
            "provider": row.get::<String, _>("provider_type"),
            "title": canonical.file_name().and_then(|v| v.to_str()).unwrap_or(path),
            "displayPath": path,
            "workspaceLabel": row.get::<String, _>("name"),
            "workspaceKind": row.get::<String, _>("workspace_kind"),
            "workspaceId": workspace_id,
            "projectId": row.get::<Uuid, _>("project_id"),
            "projectName": project_name,
            "contentType": if is_dir { Value::Null } else { json!("application/octet-stream") },
            "byteSize": if is_dir { Value::Null } else { json!(metadata.len()) },
            "previewKind": preview_kind,
            "denialReason": null,
            "capabilities": { "preview": !is_dir, "download": !is_dir, "listChildren": is_dir },
        });
        results.push(json!({ "query": normalized_query, "openable": true, "resource": resource }));
    }

    Ok(Json(
        json!({ "kind": "workspace_file_availability", "results": results }),
    ))
}

async fn list_file_resources(
    State(_state): State<AppState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// I22: GET /issues/:id/file-resources/resolve
async fn resolve_file_resource(
    State(_state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "resolved": []})))
}

/// I23: GET /issues/:id/file-resources/content
async fn get_file_resource_content(
    State(_state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "content": ""})))
}

/// I24: GET /issues/:id/feedback-votes
async fn list_feedback_votes(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let user_id = match &actor {
        AuthorizationActor::Board { user_id, .. } => *user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT id, company_id, issue_id, target_type, target_id, author_user_id, vote, reason,
                shared_with_labs, created_at, updated_at
         FROM feedback_votes
         WHERE company_id = $1 AND issue_id = $2 AND author_user_id = $3
         ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id.to_string())
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list issue feedback votes");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.into_iter().map(feedback_vote_json).collect()))
}

/// I25: POST /issues/:id/feedback-votes
async fn create_feedback_vote(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    let company_id = sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM issues WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let target_type = payload
        .get("targetType")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let target_id = payload
        .get("targetId")
        .and_then(Value::as_str)
        .filter(|value| Uuid::parse_str(value).is_ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let vote = payload
        .get("vote")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "up" | "down"))
        .ok_or(StatusCode::BAD_REQUEST)?;
    let reason = payload.get("reason").and_then(Value::as_str);
    if reason.is_some_and(|value| value.chars().count() > 1000) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let allow_sharing = payload
        .get("allowSharing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let author_user_id = user_id.to_string();
    let row = sqlx::query(
        "INSERT INTO feedback_votes
            (company_id, issue_id, target_type, target_id, author_user_id, voter_id, voter_type,
             vote, reason, shared_with_labs)
         VALUES ($1, $2, $3, $4, $5, $6, 'user', $7, $8, $9)
         ON CONFLICT (company_id, target_type, target_id, author_user_id)
         DO UPDATE SET vote = EXCLUDED.vote, reason = EXCLUDED.reason,
                       shared_with_labs = EXCLUDED.shared_with_labs, updated_at = NOW()
         RETURNING id, company_id, issue_id, target_type, target_id, author_user_id, vote, reason,
                   shared_with_labs, created_at, updated_at",
    )
    .bind(company_id)
    .bind(id)
    .bind(target_type)
    .bind(target_id)
    .bind(&author_user_id)
    .bind(user_id)
    .bind(vote)
    .bind(reason)
    .bind(allow_sharing)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to save issue feedback vote");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let vote_id = row.get::<Uuid, _>("id");
    sqlx::query(
        "INSERT INTO feedback_traces
            (company_id, issue_id, vote_id, target_type, target_id, payload, status, shared_with_labs)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(company_id)
    .bind(id)
    .bind(vote_id)
    .bind(target_type)
    .bind(Uuid::parse_str(target_id).ok())
    .bind(json!({
        "targetType": target_type,
        "targetId": target_id,
        "vote": vote,
        "reason": reason,
    }))
    .bind(if allow_sharing { "pending" } else { "local_only" })
    .bind(allow_sharing)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, vote_id = %vote_id, "failed to persist feedback trace");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((StatusCode::CREATED, Json(feedback_vote_json(row))))
}

/// I26: GET /issues/:id/feedback-traces
async fn list_feedback_traces(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    if !matches!(actor, AuthorizationActor::Board { .. }) {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = sqlx::query(
        "SELECT id, company_id, issue_id, vote_id, target_type, target_id, payload, status,
                failure_reason, shared_with_labs, created_at, updated_at
         FROM feedback_traces
         WHERE company_id = $1 AND issue_id = $2
         ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(feedback_trace_json).collect()))
}

fn feedback_vote_json(row: sqlx::postgres::PgRow) -> serde_json::Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "issueId": row.get::<Uuid, _>("issue_id"),
        "targetType": row.get::<String, _>("target_type"),
        "targetId": row.get::<String, _>("target_id"),
        "authorUserId": row.get::<String, _>("author_user_id"),
        "vote": row.get::<String, _>("vote"),
        "reason": row.get::<Option<String>, _>("reason"),
        "sharedWithLabs": row.get::<bool, _>("shared_with_labs"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

fn feedback_trace_json(row: sqlx::postgres::PgRow) -> serde_json::Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "issueId": row.get::<Uuid, _>("issue_id"),
        "voteId": row.get::<Uuid, _>("vote_id"),
        "targetType": row.get::<String, _>("target_type"),
        "targetId": row.get::<Option<Uuid>, _>("target_id"),
        "payload": row.get::<serde_json::Value, _>("payload"),
        "status": row.get::<String, _>("status"),
        "failureReason": row.get::<Option<String>, _>("failure_reason"),
        "sharedWithLabs": row.get::<bool, _>("shared_with_labs"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
}

/// I27: GET /issues/:id/recovery-actions
async fn list_recovery_actions(
    State(state): State<AppState>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    state
        .issue_service
        .get_recovery_actions(id, company_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// I28: POST /issues/:id/recovery-actions/resolve
async fn resolve_recovery_action(
    State(state): State<AppState>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let company_id = issue_company_id(&state, id).await?;
    let action_id = payload
        .get("actionId")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    state
        .issue_service
        .resolve_recovery_action(id, company_id, action_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Create issue routes
pub fn issue_routes() -> Router<AppState> {
    Router::new()
        .route("/issues", get(list_issues))
        .route(
            "/issues/:id",
            get(get_issue).patch(update_issue).delete(delete_issue),
        )
        .route(
            "/companies/:companyId/issues",
            get(list_company_issues).post(create_issue),
        )
        .route("/companies/:companyId/issues/count", get(count_issues))
        .route("/companies/:companyId/issues/search", get(search_issues))
        .route("/issues/:id/checkout", post(checkout_issue))
        .route("/issues/:id/release", post(release_issue))
        .route("/issues/:id/admin/force-release", post(force_release_issue))
        .route(
            "/companies/:companyId/issues/batch-update",
            post(batch_update_issues),
        )
        .route("/issues/:id/heartbeat-context", get(get_heartbeat_context))
        // --- P1: Issue 子资源补齐 (I1-I44) ---
        .route("/issues/:id/cases", get(get_issue_cases))
        .route("/issues/:id/active-run", get(get_issue_active_run))
        .route("/issues/:id/live-runs", get(get_issue_live_runs))
        .route(
            "/issues/:id/work-products",
            get(list_work_products).post(create_work_product),
        )
        .route(
            "/issues/:id/children",
            get(list_child_issues).post(create_child_issue),
        )
        .route("/issues/:id/transcript", get(get_issue_transcript))
        .route(
            "/issues/:id/read",
            post(mark_issue_read).delete(unmark_issue_read),
        )
        .route(
            "/issues/:id/inbox-archive",
            post(archive_issue_inbox).delete(unarchive_issue_inbox),
        )
        .route("/issues/:id/monitor/check-now", post(monitor_check_now))
        .route(
            "/issues/:id/scheduled-retry/retry-now",
            post(scheduled_retry_now),
        )
        .route("/issues/:id/external-objects", get(list_external_objects))
        .route(
            "/issues/:id/external-object-summary",
            get(get_external_object_summary),
        )
        .route(
            "/issues/:id/external-objects/refresh",
            post(refresh_external_objects),
        )
        .route("/issues/:id/documents", get(list_issue_documents))
        .route(
            "/issues/:id/documents/:key",
            get(get_issue_document)
                .put(upsert_issue_document)
                .delete(delete_issue_document),
        )
        .route(
            "/issues/:id/documents/:key/revisions",
            get(list_issue_document_revisions),
        )
        .route(
            "/issues/:id/documents/:key/revisions/:revision_id/restore",
            post(restore_issue_document_revision),
        )
        .route(
            "/issues/:id/documents/:key/annotations",
            get(get_issue_document_annotations).post(create_issue_document_annotation),
        )
        .route(
            "/issues/:id/documents/:key/annotations/:thread_id",
            get(get_issue_document_annotation_thread).patch(update_issue_document_annotation),
        )
        .route(
            "/issues/:id/documents/:key/annotations/:thread_id/reply",
            post(reply_issue_document_annotation),
        )
        .route("/issues/:id/documents/:key/lock", post(lock_issue_document))
        .route(
            "/issues/:id/documents/:key/unlock",
            post(unlock_issue_document),
        )
        .route("/issues/:id/file-resources/list", get(list_file_resources))
        .route(
            "/issues/:id/file-resources/availability",
            post(check_file_resource_availability),
        )
        .route(
            "/issues/:id/file-resources/resolve",
            get(resolve_file_resource),
        )
        .route(
            "/issues/:id/file-resources/content",
            get(get_file_resource_content),
        )
        .route(
            "/issues/:id/feedback-votes",
            get(list_feedback_votes).post(create_feedback_vote),
        )
        .route("/issues/:id/feedback-traces", get(list_feedback_traces))
        .route("/issues/:id/recovery-actions", get(list_recovery_actions))
        .route(
            "/issues/:id/recovery-actions/resolve",
            post(resolve_recovery_action),
        )
        .route(
            "/issues/:id/accepted-plan-decompositions",
            get(list_plan_decompositions).post(submit_plan_decomposition),
        )
        .route(
            "/issues/:id/approvals",
            get(list_issue_approvals).post(create_issue_approval),
        )
        .route(
            "/issues/:id/approvals/:approval_id",
            axum::routing::delete(delete_issue_approval),
        )
}

#[cfg(test)]
mod user_filter_tests {
    use super::resolve_user_filter;
    use axum::http::StatusCode;
    use services::auth::AuthorizationActor;
    use uuid::Uuid;

    #[test]
    fn resolves_me_to_board_user() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(user_id, company_id);

        assert_eq!(resolve_user_filter(Some("me"), &actor), Ok(Some(user_id)));
        assert_eq!(resolve_user_filter(Some("ME"), &actor), Ok(Some(user_id)));
    }

    #[test]
    fn accepts_uuid_and_rejects_invalid_user_filter() {
        let actor = AuthorizationActor::board(Uuid::new_v4(), Uuid::new_v4());
        let user_id = Uuid::new_v4();

        assert_eq!(
            resolve_user_filter(Some(&user_id.to_string()), &actor),
            Ok(Some(user_id))
        );
        assert_eq!(
            resolve_user_filter(Some("not-a-uuid"), &actor),
            Err(StatusCode::BAD_REQUEST)
        );
    }

    #[test]
    fn rejects_me_for_agent_without_user_delegation() {
        let actor = AuthorizationActor::agent(Uuid::new_v4(), Uuid::new_v4(), None);

        assert_eq!(
            resolve_user_filter(Some("me"), &actor),
            Err(StatusCode::FORBIDDEN)
        );
    }
}
