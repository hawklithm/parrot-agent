use crate::app_state::AppState;
use crate::extractors::IssueId;
use crate::routes::log_activity;
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
use std::path::{Path as FsPath, PathBuf};

use models::event_bus::{EventMetadata, IssueEvent, SystemEvent, SystemEventPayload};
use models::{CreateIssueInput, Issue, IssuePriority, IssueStatus, UpdateIssueInput};
use services::auth::AuthorizationActor;
use services::{
    CheckoutInput, CrossIssueInfluenceKind, CrossIssueInfluenceLimitService,
    DefaultCrossIssueInfluenceLimitService, HeartbeatWakeupOptions, IssueQueryFilter, Pagination,
    ReleaseInput,
};

async fn publish_issue_event(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    payload: IssueEvent,
) {
    let event = SystemEvent::new(
        EventMetadata {
            event_id: Uuid::new_v4(),
            correlation_id: None,
            causation_id: None,
            actor_type: actor.actor_type().to_string(),
            actor_id: actor.principal_id().unwrap_or(Uuid::nil()),
            company_id,
        },
        SystemEventPayload::Issue(payload),
    );
    if let Err(error) = state.event_bus.publish(Box::new(event)).await {
        tracing::warn!(%error, company_id = %company_id, "failed to publish issue event");
    }
}

// Issue relations handlers
mod issue_relations_handlers {
    use super::*;

    /// GET /issues/:issue_id/relations
    pub async fn get_issue_relations(
        State(state): State<AppState>,
        Extension(actor): Extension<AuthorizationActor>,
        Path(issue_id): Path<Uuid>,
    ) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
        let company_id = super::scoped_issue_company(&state, &actor, issue_id)
            .await
            .map_err(|status| (status, "Issue is outside the actor company".to_string()))?;
        let rows = sqlx::query(
            r#"
            SELECT
                ir.issue_id,
                ir.related_issue_id,
                ir.type,
                CASE WHEN ir.related_issue_id = $2 THEN 'blocked_by' ELSE 'blocks' END AS relation_kind,
                i.identifier,
                i.title,
                i.status
            FROM issue_relations ir
            JOIN issues i ON i.id = CASE WHEN ir.related_issue_id = $2 THEN ir.issue_id ELSE ir.related_issue_id END
            WHERE ir.company_id = $1
              AND (ir.issue_id = $2 OR ir.related_issue_id = $2)
              AND ir.type = 'blocks'
            ORDER BY i.created_at DESC
            "#,
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let mut blocked_by = Vec::new();
        let mut blocks = Vec::new();
        for row in rows {
            let relation = serde_json::json!({
                "id": if row.get::<String, _>("relation_kind") == "blocked_by" {
                    row.get::<Uuid, _>("issue_id")
                } else {
                    row.get::<Uuid, _>("related_issue_id")
                },
                "identifier": row.get::<Option<String>, _>("identifier"),
                "title": row.get::<String, _>("title"),
                "status": row.get::<String, _>("status"),
                "relationType": row.get::<String, _>("relation_kind"),
            });
            if relation["relationType"] == "blocked_by" {
                blocked_by.push(relation);
            } else {
                blocks.push(relation);
            }
        }

        Ok(Json(serde_json::json!({
            "blockedBy": blocked_by,
            "blocks": blocks,
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
        Extension(actor): Extension<AuthorizationActor>,
        Path(issue_id): Path<Uuid>,
        Json(input): Json<UpdateBlockedByInput>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let company_id = super::scoped_issue_company(&state, &actor, issue_id)
            .await
            .map_err(|status| (status, "Issue is outside the actor company".to_string()))?;
        let mut blocker_ids = input.blocked_by_issue_ids;
        blocker_ids.sort_unstable();
        blocker_ids.dedup();
        if blocker_ids.iter().any(|blocker_id| *blocker_id == issue_id) {
            return Err((StatusCode::BAD_REQUEST, "An issue cannot block itself".to_string()));
        }

        let visible_blockers: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE company_id = $1 AND id = ANY($2)",
        )
        .bind(company_id)
        .bind(&blocker_ids)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if visible_blockers != blocker_ids.len() as i64 {
            return Err((
                StatusCode::BAD_REQUEST,
                "All blocker issues must belong to the same company".to_string(),
            ));
        }

        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        sqlx::query(
            "DELETE FROM issue_relations WHERE company_id = $1 AND related_issue_id = $2 AND type = 'blocks'",
        )
        .bind(company_id)
        .bind(issue_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        for blocker_id in &blocker_ids {
            sqlx::query(
                "INSERT INTO issue_relations (company_id, issue_id, related_issue_id, type, created_by_agent_id, created_by_user_id) VALUES ($1, $2, $3, 'blocks', $4, $5) ON CONFLICT (company_id, issue_id, related_issue_id, type) DO NOTHING",
            )
            .bind(company_id)
            .bind(blocker_id)
            .bind(issue_id)
            .bind(if actor.is_agent() { actor.principal_id() } else { None })
            .bind(if actor.is_board() { actor.principal_id() } else { None })
            .execute(&mut *tx)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        sqlx::query(
            "INSERT INTO activity_logs (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata) VALUES ($1, 'issue_blockers_updated', $2, $3, 'issue', $4, $5)",
        )
        .bind(company_id)
        .bind(actor.actor_type())
        .bind(actor.principal_id().unwrap_or_else(Uuid::nil))
        .bind(issue_id)
        .bind(serde_json::json!({ "blockedByIssueIds": blocker_ids }))
        .execute(&mut *tx)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tx.commit()
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileResourceQuery {
    path: String,
    workspace: Option<String>,
    project_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileResourceListQuery {
    workspace: Option<String>,
    project_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
    path: Option<String>,
    mode: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

struct WorkspaceFileRoot {
    workspace_kind: String,
    workspace_id: Uuid,
    project_id: Uuid,
    cwd: PathBuf,
    name: String,
    provider: String,
    project_name: String,
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

fn document_revision_creator(actor: &AuthorizationActor) -> (Option<&'static str>, Option<Uuid>) {
    match actor {
        AuthorizationActor::Board { user_id, .. } => (Some("user"), Some(*user_id)),
        AuthorizationActor::Agent { agent_id, .. } => (Some("agent"), Some(*agent_id)),
        AuthorizationActor::None => (None, None),
    }
}

fn document_database_error(error: sqlx::Error, operation: &str, issue_id: Uuid) -> StatusCode {
    tracing::error!(error = %error, %issue_id, operation, "issue document database operation failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

/// PUT /issues/:id/documents/:key — Create or update an issue document.
async fn upsert_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key)): Path<(String, String)>,
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
    let title = payload
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&key);
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
        .map_err(|error| document_database_error(error, "begin document upsert transaction", issue_id))?;
    // A missing issue_documents row cannot itself be locked by PostgreSQL.
    // Serialize first-create and existing-document writers on the logical
    // issue/key so concurrent PUTs cannot create duplicate documents.
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2, 0))",
    )
    .bind(issue_id)
    .bind(&key)
    .execute(&mut *tx)
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
        let locked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT locked_at FROM documents WHERE id=$1 FOR UPDATE",
        )
        .bind(document_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if locked_at.is_some() {
            return Err(StatusCode::CONFLICT);
        }
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
        sqlx::query("UPDATE issue_documents SET updated_at=NOW() WHERE issue_id=$1 AND key=$2")
            .bind(issue_id)
            .bind(&key)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        (document_id, revision)
    } else {
        if base_revision_id.is_some() {
            return Err(StatusCode::CONFLICT);
        }
        let document_id: Uuid = sqlx::query_scalar("INSERT INTO documents (company_id, title, content, content_type) VALUES ($1,$2,$3,$4) RETURNING id")
            .bind(company_id).bind(title).bind(content).bind(content_type).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        sqlx::query("INSERT INTO issue_documents (company_id, issue_id, document_id, key) VALUES ($1,$2,$3,$4)")
            .bind(company_id).bind(issue_id).bind(document_id).bind(&key).execute(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
        (document_id, 1)
    };
    let (created_by_type, created_by_id) = document_revision_creator(&actor);
    sqlx::query(
        "INSERT INTO document_revisions
           (document_id, company_id, revision_number, content, created_by_type, created_by_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(document_id)
    .bind(company_id)
    .bind(revision_number)
    .bind(content)
    .bind(created_by_type)
    .bind(created_by_id)
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
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| document_database_error(error, "begin document restore transaction", issue_id))?;
    // Serialize all writers for this issue/key before reading the source and
    // allocating the next revision number. Without this lock two restores can
    // both observe the same MAX(revision_number) and race on the unique key.
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2 FOR UPDATE",
    )
    .bind(issue_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "lock document for restore", issue_id))?
    .ok_or(StatusCode::NOT_FOUND)?;
    let locked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT locked_at FROM documents WHERE id=$1 FOR UPDATE",
    )
    .bind(document_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "check document lock for restore", issue_id))?;
    if locked_at.is_some() {
        return Err(StatusCode::CONFLICT);
    }
    let (content, source_revision_number): (String, i32) = sqlx::query_as(
        "SELECT content, revision_number FROM document_revisions WHERE id=$1 AND document_id=$2",
    )
    .bind(revision_id)
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "read restore source revision", issue_id))?
    .ok_or(StatusCode::NOT_FOUND)?;
    let revision: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision_number),0)+1 FROM document_revisions WHERE document_id=$1",
    )
    .bind(document_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "create restore revision", issue_id))?;
    sqlx::query("UPDATE documents SET content=$2, updated_at=NOW() WHERE id=$1")
        .bind(document_id)
        .bind(&content)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (created_by_type, created_by_id) = document_revision_creator(&actor);
    sqlx::query(
        "INSERT INTO document_revisions
           (document_id, company_id, revision_number, content, created_by_type, created_by_id)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(document_id)
    .bind(company_id)
    .bind(revision)
    .bind(&content)
    .bind(created_by_type)
    .bind(created_by_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({
            "restored": true,
            "issueId": issue_id,
            "key": key,
            "revisionId": revision_id,
            "revisionNumber": revision,
            "restoredFromRevisionId": revision_id,
            "restoredFromRevisionNumber": source_revision_number,
            "content": content,
            "body": content
        }),
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
    let key = validate_issue_document_key(&key)?;
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

    if !matches!(status, "all" | "open" | "resolved") {
        return Err(StatusCode::BAD_REQUEST);
    }
    if status != "all" {
        // The value is restricted to the two enum values above before it is
        // embedded in this query.
        query.push_str(" AND status='");
        query.push_str(status);
        query.push('\'');
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
    let key = validate_issue_document_key(&key)?;
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
    Path((issue_id, raw_key)): Path<(String, String)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;

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

    let base_revision_id = payload
        .get("baseRevisionId")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let base_revision_number = payload
        .get("baseRevisionNumber")
        .and_then(Value::as_i64)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if base_revision_id.is_some() != base_revision_number.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| document_database_error(error, "begin annotation transaction", issue_id))?;
    let (document_id, document_content): (Uuid, String) = sqlx::query_as(
        "SELECT d.id, d.content
         FROM issue_documents l
         JOIN documents d ON d.id=l.document_id
         WHERE l.issue_id=$1 AND l.key=$2
         FOR UPDATE OF l, d",
    )
    .bind(issue_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "lock annotation document", issue_id))?
    .ok_or(StatusCode::NOT_FOUND)?;
    let (current_revision_id, revision_number): (Option<Uuid>, i32) = sqlx::query_as(
        "SELECT id, revision_number
         FROM document_revisions
         WHERE document_id=$1
         ORDER BY revision_number DESC
         LIMIT 1
         FOR UPDATE",
    )
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "read annotation revision", issue_id))?
    .map(|(id, number): (Uuid, i32)| (Some(id), number))
    .unwrap_or((None, 0));
    if let (Some(expected_id), Some(expected_number)) = (base_revision_id, base_revision_number) {
        if current_revision_id != Some(expected_id) || revision_number != expected_number {
            return Err(StatusCode::CONFLICT);
        }
    }
    if !selected_text.is_empty() && !document_content.contains(selected_text) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

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

    let thread_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_threads \
         (company_id, issue_id, document_id, document_key, selected_text, anchor_selector, \
          original_revision_id, original_revision_number, current_revision_id, current_revision_number, \
          normalized_start, normalized_end, markdown_start, markdown_end, prefix_text, suffix_text, \
          created_by_agent_id, created_by_user_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $7, $8, 0, 0, 0, 0, '', '', $9, $10) \
         RETURNING id",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(&key)
    .bind(selected_text)
    .bind(&selector)
    .bind(current_revision_id)
    .bind(revision_number)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "create annotation thread", issue_id))?;

    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments \
         (company_id, thread_id, issue_id, document_id, body, author_type, author_agent_id, author_user_id, created_by_run_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id",
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(body)
    .bind(author_type)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .bind(created_by_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| document_database_error(error, "create annotation comment", issue_id))?;

    tx.commit()
        .await
        .map_err(|error| document_database_error(error, "commit annotation transaction", issue_id))?;

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
                "authorType": author_type,
                "authorAgentId": author_agent_id,
                "authorUserId": author_user_id,
                "createdByRunId": created_by_run_id,
            }],
        })),
    ))
}

/// POST /issues/:id/documents/:key/annotations/:thread_id/reply
async fn reply_issue_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key, thread_id)): Path<(String, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;

    let body = payload
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2 FOR UPDATE",
    )
    .bind(issue_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM document_annotation_threads
         WHERE id=$1 AND issue_id=$2 AND document_id=$3
         FOR UPDATE",
    )
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

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

    let comment_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_annotation_comments \
         (company_id, thread_id, issue_id, document_id, body, author_type, author_agent_id, author_user_id, created_by_run_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id"
    )
    .bind(company_id)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(body)
    .bind(author_type)
    .bind(author_agent_id)
    .bind(author_user_id.clone())
    .bind(created_by_run_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("UPDATE document_annotation_threads SET updated_at=NOW() WHERE id=$1")
        .bind(thread_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
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
            "authorType": author_type,
            "authorAgentId": author_agent_id,
            "authorUserId": author_user_id,
            "createdByRunId": created_by_run_id,
        })),
    ))
}

/// PATCH /issues/:id/documents/:key/annotations/:thread_id
async fn update_issue_document_annotation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, raw_key, thread_id)): Path<(String, String, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    let _company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&raw_key)?;

    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open");
    if !matches!(status, "open" | "resolved") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let document_id: Uuid = sqlx::query_scalar(
        "SELECT document_id FROM issue_documents WHERE issue_id=$1 AND key=$2 FOR UPDATE",
    )
    .bind(issue_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let (resolved_by_agent_id, resolved_by_user_id) = match &actor {
        AuthorizationActor::Board { user_id, .. } => (None, Some(user_id.to_string())),
        AuthorizationActor::Agent { agent_id, .. } => (Some(*agent_id), None),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let updated = sqlx::query(
        "UPDATE document_annotation_threads \
         SET status=$1,
             resolved_by_agent_id=CASE WHEN $1='resolved' THEN $5 ELSE NULL END,
             resolved_by_user_id=CASE WHEN $1='resolved' THEN $6 ELSE NULL END,
             resolved_at=CASE WHEN $1='resolved' THEN NOW() ELSE NULL END,
             updated_at=NOW() \
         WHERE id=$2 AND issue_id=$3 AND document_id=$4 \
         RETURNING id, status, updated_at"
    )
    .bind(status)
    .bind(thread_id)
    .bind(issue_id)
    .bind(document_id)
    .bind(resolved_by_agent_id)
    .bind(resolved_by_user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
)
    -> Result<Json<IssueDocumentResponse>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    if !actor.is_board() {
        return Err(StatusCode::FORBIDDEN);
    }
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&key)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let select = "SELECT d.id, d.company_id, l.issue_id, l.key, d.content, d.content_type, \
                         d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, \
                         d.created_at, d.updated_at \
                  FROM issue_documents l JOIN documents d ON d.id=l.document_id \
                  WHERE l.issue_id=$1 AND l.key=$2 FOR UPDATE OF l, d";
    let existing = sqlx::query_as::<_, IssueDocumentResponse>(select)
        .bind(issue_id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.locked_at.is_some() {
        tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(existing));
    }

    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    sqlx::query(
        "UPDATE documents SET locked_by_type='user', locked_by_id=$2, locked_at=NOW(), locked_run_id=NULL, updated_at=NOW() WHERE id=$1",
    )
    .bind(existing.id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE issue_documents SET updated_at=NOW() WHERE issue_id=$1 AND key=$2")
        .bind(issue_id)
        .bind(&key)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = sqlx::query_as::<_, IssueDocumentResponse>(select)
        .bind(issue_id)
        .bind(&key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    log_activity(
        &state.pool,
        company_id,
        "issue.document_locked",
        &actor,
        "issue",
        issue_id,
        serde_json::json!({"key": key, "documentId": updated.id}),
    )
    .await;
    Ok(Json(updated))
}

/// POST /issues/:id/documents/:key/unlock - Unlock issue document
async fn unlock_issue_document(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((issue_id, key)): Path<(String, String)>,
) -> Result<Json<IssueDocumentResponse>, StatusCode> {
    let issue_id = resolve_issue_id(&state.pool, &issue_id).await?;
    if !actor.is_board() {
        return Err(StatusCode::FORBIDDEN);
    }
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let key = validate_issue_document_key(&key)?;
    let mut tx = state.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let select = "SELECT d.id, d.company_id, l.issue_id, l.key, d.content, d.content_type, \
                         d.locked_by_type, d.locked_by_id, d.locked_at, d.locked_run_id, \
                         d.created_at, d.updated_at \
                  FROM issue_documents l JOIN documents d ON d.id=l.document_id \
                  WHERE l.issue_id=$1 AND l.key=$2 FOR UPDATE OF l, d";
    let existing = sqlx::query_as::<_, IssueDocumentResponse>(select)
        .bind(issue_id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.locked_at.is_none() {
        tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(Json(existing));
    }
    sqlx::query(
        "UPDATE documents SET locked_by_type=NULL, locked_by_id=NULL, locked_at=NULL, locked_run_id=NULL, updated_at=NOW() WHERE id=$1",
    )
    .bind(existing.id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE issue_documents SET updated_at=NOW() WHERE issue_id=$1 AND key=$2")
        .bind(issue_id)
        .bind(&key)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = sqlx::query_as::<_, IssueDocumentResponse>(select)
        .bind(issue_id)
        .bind(&key)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    log_activity(
        &state.pool,
        company_id,
        "issue.document_unlocked",
        &actor,
        "issue",
        issue_id,
        serde_json::json!({"key": key, "documentId": updated.id}),
    )
    .await;
    Ok(Json(updated))
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
    let idempotency_key = input
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned);
    if idempotency_key.as_ref().is_some_and(|key| key.len() > 255) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut title_connection = if idempotency_key.is_none() && !input.allow_duplicate {
        let mut connection = state
            .pool
            .acquire()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let title_lock_key = format!(
            "issue-create-title:{company_id}:{}:{}",
            input.parent_id.map(|parent| parent.to_string()).unwrap_or_else(|| "root".to_string()),
            input.title.trim()
        );
        sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 0))")
            .bind(&title_lock_key)
            .execute(&mut *connection)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let duplicate_issue_id: Option<Uuid> = match sqlx::query_scalar(
            "SELECT id
             FROM issues
             WHERE company_id = $1
               AND (($2::uuid IS NULL AND parent_id IS NULL) OR parent_id = $2)
               AND hidden_at IS NULL
               AND status::text NOT IN ('done', 'cancelled')
               AND created_at >= NOW() - INTERVAL '48 hours'
               AND lower(regexp_replace(btrim(title), '\\s+', ' ', 'g')) = lower(regexp_replace(btrim($3), '\\s+', ' ', 'g'))
             ORDER BY created_at ASC, id ASC
             LIMIT 1",
        )
        .bind(company_id)
        .bind(input.parent_id)
        .bind(&input.title)
        .fetch_optional(&mut *connection)
        .await
        {
            Ok(issue_id) => issue_id,
            Err(error) => {
                let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                    .bind(&title_lock_key)
                    .execute(&mut *connection)
                    .await;
                tracing::error!(error = %error, company_id = %company_id, "failed to check duplicate issue title");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
        if let Some(duplicate_issue_id) = duplicate_issue_id {
            let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                .bind(&title_lock_key)
                .execute(&mut *connection)
                .await;
            let duplicate_issue = state
                .issue_service
                .get(duplicate_issue_id, company_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            return Ok(Json(duplicate_issue));
        }
        Some((connection, title_lock_key))
    } else {
        None
    };
    // Keep a session-level advisory lock on a checked-out connection while the
    // service creates the Issue, then persist the key. This closes the race
    // between replay lookup and creation without changing the repository API.
    let mut idempotency_connection = if let Some(key) = idempotency_key.as_deref() {
        let mut connection = state
            .pool
            .acquire()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let lock_key = format!("issue-create:{company_id}:{key}");
        sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *connection)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let cleanup_result = sqlx::query(
            "DELETE FROM issue_create_idempotency_keys
             WHERE id IN (
                 SELECT id
                 FROM issue_create_idempotency_keys
                 WHERE company_id = $1
                   AND created_at < NOW() - INTERVAL '7 days'
                 ORDER BY created_at ASC, id ASC
                 LIMIT 500
             )",
        )
        .bind(company_id)
        .execute(&mut *connection)
        .await;
        if cleanup_result.is_err() {
            let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                .bind(&lock_key)
                .execute(&mut *connection)
                .await;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        let existing_issue_id: Option<Uuid> = match sqlx::query_scalar(
            "SELECT issue_id FROM issue_create_idempotency_keys WHERE company_id = $1 AND idempotency_key = $2",
        )
        .bind(company_id)
        .bind(key)
        .fetch_optional(&mut *connection)
        .await
        {
            Ok(issue_id) => issue_id,
            Err(_) => {
                let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                    .bind(&lock_key)
                    .execute(&mut *connection)
                    .await;
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };
        if let Some(existing_issue_id) = existing_issue_id {
            let existing_issue = match state
                .issue_service
                .get(existing_issue_id, company_id)
                .await
            {
                Ok(Some(issue)) => issue,
                Ok(None) => {
                    let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                        .bind(&lock_key)
                        .execute(&mut *connection)
                        .await;
                    return Err(StatusCode::NOT_FOUND);
                }
                Err(_) => {
                    let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                        .bind(&lock_key)
                        .execute(&mut *connection)
                        .await;
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                .bind(&lock_key)
                .execute(&mut *connection)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            return Ok(Json(existing_issue));
        }
        Some((connection, lock_key))
    } else {
        None
    };
    let service = state.issue_service.clone();
    let created = match service.create(input).await {
        Ok(created) => created,
        Err(error) => {
            if let Some((mut connection, title_lock_key)) = title_connection.take() {
                let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                    .bind(&title_lock_key)
                    .execute(&mut *connection)
                    .await;
            }
            if let Some((mut connection, lock_key)) = idempotency_connection.take() {
                let _ = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
                    .bind(&lock_key)
                    .execute(&mut *connection)
                    .await;
            }
            tracing::error!(error = %error, company_id = %company_id, "issue creation failed");
            return Err(issue_service_status(&error));
        }
    };
    if let Some((mut connection, title_lock_key)) = title_connection.take() {
        let unlock_result = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
            .bind(&title_lock_key)
            .execute(&mut *connection)
            .await;
        if unlock_result.is_err() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    if let Some((mut connection, lock_key)) = idempotency_connection.take() {
        let insert_result = sqlx::query(
            "INSERT INTO issue_create_idempotency_keys (company_id, idempotency_key, issue_id) VALUES ($1, $2, $3)",
        )
        .bind(company_id)
        .bind(idempotency_key.as_deref().expect("idempotency key is present"))
        .bind(created.issue.id)
        .execute(&mut *connection)
        .await;
        let unlock_result = sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *connection)
            .await;
        if insert_result.is_err() || unlock_result.is_err() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

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
                idempotency_key: None,
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

    if state
        .issue_tree_control_service
        .get_pause_state(id)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, issue_id = %id, "failed to evaluate issue tree pause hold");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .is_some()
    {
        let interaction_allowed = if let Some(agent_id) = checkout_agent_id
        {
            let context: Option<serde_json::Value> = sqlx::query_scalar(
                "SELECT context_snapshot
                 FROM heartbeat_runs
                 WHERE id = $1 AND company_id = $2 AND agent_id = $3",
            )
            .bind(checkout_run_id)
            .bind(company_id)
            .bind(agent_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let Some(context) = context else {
                return Err(StatusCode::CONFLICT);
            };
            let reason = context.get("wakeReason").and_then(|value| value.as_str());
            let source = context.get("source").and_then(|value| value.as_str());
            let comment_id = context
                .get("commentId")
                .and_then(|value| value.as_str())
                .and_then(|value| uuid::Uuid::parse_str(value).ok());
            let expected_source = match reason {
                Some("issue_commented") => Some("issue.comment"),
                Some("issue_reopened_via_comment") => Some("issue.comment.reopen"),
                Some("issue_comment_mentioned") => Some("comment.mention"),
                _ => None,
            };
            if source != expected_source || comment_id.is_none() {
                false
            } else {
                let requested_type = context
                    .get("requestedByActorType")
                    .and_then(|value| value.as_str());
                let requested_id = context
                    .get("requestedByActorId")
                    .and_then(|value| value.as_str())
                    .and_then(|value| uuid::Uuid::parse_str(value).ok());
                let comment = sqlx::query_as::<_, (String, Option<uuid::Uuid>)>(
                    "SELECT actor_type::text, actor_id
                     FROM issue_comments
                     WHERE id = $1 AND company_id = $2 AND issue_id = $3",
                )
                .bind(comment_id)
                .bind(company_id)
                .bind(id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                comment.is_some_and(|(actor_type, actor_id)| {
                    Some(actor_type) == requested_type.map(str::to_string)
                        && actor_id.is_some()
                        && actor_id == requested_id
                })
            }
        } else {
            false
        };
        if !interaction_allowed {
            return Err(StatusCode::CONFLICT);
        }
    }

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

    log_activity(
        &state.pool,
        company_id,
        "issue.checked_out",
        &actor,
        "issue",
        id,
        json!({ "agentId": checkout_agent_id, "runId": checkout_run_id }),
    )
    .await;

    if let Some(agent_id) = checkout_agent_id {
        publish_issue_event(
            &state,
            &actor,
            company_id,
            IssueEvent::CheckedOut {
                issue_id: id,
                company_id,
                agent_id,
                checked_out_by: actor.principal_id().unwrap_or(agent_id),
            },
        )
        .await;
    }

    // Paperclip wakes the assignee after a successful checkout so the agent
    // can continue with the newly-owned execution context.
    if let Some(assignee_agent_id) = checkout_agent_id {
        let wakeup_service =
            services::IssueAssignmentWakeupService::new(state.heartbeat_service.clone());
        let wakeup_input = services::issue_assignment_wakeup::QueueWakeupInput {
            company_id,
            issue_id: id,
            assignee_agent_id: Some(assignee_agent_id),
            status: "in_progress".to_string(),
            reason: "issue_checked_out".to_string(),
            mutation: "checkout".to_string(),
            context_source: "issue.checkout".to_string(),
            requested_by_actor_type: Some(actor.actor_type().to_string()),
            requested_by_actor_id: actor.principal_id(),
            idempotency_key: None,
            rethrow_on_error: false,
        };
        if let Err(error) = wakeup_service.queue_wakeup(wakeup_input).await {
            tracing::warn!(
                error = %error,
                issue_id = %id,
                assignee_agent_id = %assignee_agent_id,
                "Failed to wake assignee after issue checkout"
            );
        }
    }
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
    let release_run_id = input.release_run_id;
    let resolves_blocker = input.result.as_deref() == Some("success")
        || input.target_status.as_deref() == Some("done");
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
    log_activity(
        &state.pool,
        company_id,
        "issue.released",
        &actor,
        "issue",
        id,
        json!({ "releaseRunId": release_run_id }),
    )
    .await;

    if resolves_blocker {
        let dependent_rows = sqlx::query(
            "SELECT DISTINCT dependent.id, dependent.assignee_agent_id
             FROM issue_relations relation
             JOIN issues dependent ON dependent.id = relation.related_issue_id
             WHERE relation.company_id = $1
               AND relation.issue_id = $2
               AND relation.type = 'blocks'
               AND dependent.status = 'blocked'
               AND dependent.assignee_agent_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1
                   FROM issue_relations remaining
                   JOIN issues blocker ON blocker.id = remaining.issue_id
                   WHERE remaining.company_id = $1
                     AND remaining.related_issue_id = dependent.id
                     AND remaining.type = 'blocks'
                     AND blocker.status <> 'done'
               )",
        )
        .bind(company_id)
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let wakeup_service =
            services::IssueAssignmentWakeupService::new(state.heartbeat_service.clone());
        for row in dependent_rows {
            let dependent_id: Uuid = row.get("id");
            let Some(assignee_agent_id) = row.get::<Option<Uuid>, _>("assignee_agent_id") else {
                continue;
            };
            let wakeup_input = services::issue_assignment_wakeup::QueueWakeupInput {
                company_id,
                issue_id: dependent_id,
                assignee_agent_id: Some(assignee_agent_id),
                status: "blocked".to_string(),
                reason: "issue_blockers_resolved".to_string(),
                mutation: "blockers_resolved".to_string(),
                context_source: "issue.release".to_string(),
                requested_by_actor_type: Some(actor.actor_type().to_string()),
                requested_by_actor_id: actor.principal_id(),
                idempotency_key: Some(format!(
                    "issue_blockers_resolved:{}:{}",
                    dependent_id, id
                )),
                rethrow_on_error: false,
            };
            if let Err(error) = wakeup_service.queue_wakeup(wakeup_input).await {
                tracing::warn!(
                    error = %error,
                    issue_id = %dependent_id,
                    blocker_issue_id = %id,
                    "Failed to wake dependent issue after blocker release"
                );
            }
        }
    }
    let released_issue = service
        .get(id, company_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let actor_id = actor.principal_id().unwrap_or(Uuid::nil());
    let release_event = if released_issue.status == IssueStatus::Done {
        IssueEvent::Completed {
            issue_id: id,
            company_id,
            completed_by: actor_id,
            resolution: None,
        }
    } else {
        IssueEvent::Released {
            issue_id: id,
            company_id,
            released_by: actor_id,
        }
    };
    publish_issue_event(&state, &actor, company_id, release_event).await;
    Ok(Json(released_issue))
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
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT c.id, c.company_id, c.project_id, c.case_number, c.identifier,
                c.case_type, c.key, c.title, c.summary, c.status::text AS status,
                c.fields, c.parent_case_id, c.created_at, c.updated_at,
                l.id AS link_id, l.role::text AS link_role, l.created_at AS linked_at
         FROM case_issue_links l
         JOIN cases c ON c.id = l.case_id AND c.company_id = l.company_id
         WHERE l.issue_id = $1 AND l.company_id = $2
         ORDER BY l.created_at DESC",
    )
    .bind(id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list issue cases");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "companyId": row.get::<Uuid, _>("company_id"),
                    "projectId": row.get::<Option<Uuid>, _>("project_id"),
                    "caseNumber": row.get::<i32, _>("case_number"),
                    "identifier": row.get::<String, _>("identifier"),
                    "caseType": row.get::<String, _>("case_type"),
                    "key": row.get::<Option<String>, _>("key"),
                    "title": row.get::<String, _>("title"),
                    "summary": row.get::<Option<String>, _>("summary"),
                    "status": row.get::<String, _>("status"),
                    "fields": row.get::<Value, _>("fields"),
                    "parentCaseId": row.get::<Option<Uuid>, _>("parent_case_id"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                    "linkId": row.get::<Uuid, _>("link_id"),
                    "linkRole": row.get::<String, _>("link_role"),
                    "linkedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("linked_at"),
                })
            })
            .collect(),
    ))
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
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT hr.id, hr.status::text AS status, hr.invocation_source,
                hr.started_at, hr.finished_at, hr.created_at, hr.agent_id,
                a.name AS agent_name, a.adapter_type
         FROM heartbeat_runs hr
         JOIN agents a ON a.id = hr.agent_id AND a.company_id = hr.company_id
         WHERE hr.company_id = $1
           AND hr.status IN ('queued', 'running')
           AND (hr.context_snapshot->>'issueId' = $2 OR hr.context_snapshot->>'taskId' = $2)
         ORDER BY hr.created_at DESC",
    )
    .bind(company_id)
    .bind(id.to_string())
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list live issue runs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "status": row.get::<String, _>("status"),
                    "invocationSource": row.get::<String, _>("invocation_source"),
                    "startedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
                    "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("finished_at"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "agentId": row.get::<Uuid, _>("agent_id"),
                    "agentName": row.get::<String, _>("agent_name"),
                    "adapterType": row.get::<String, _>("adapter_type"),
                    "issueId": id,
                    "outputSilence": null,
                })
            })
            .collect(),
    ))
}

/// I6: GET /issues/:id/accepted-plan-decompositions
async fn list_plan_decompositions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT id, company_id, issue_id, plan, accepted_at,
                accepted_by_type, accepted_by_id, created_at, updated_at
         FROM plan_decompositions
         WHERE company_id = $1 AND issue_id = $2 AND accepted_at IS NOT NULL
         ORDER BY accepted_at DESC, created_at DESC",
    )
    .bind(company_id)
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list accepted plan decompositions");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "companyId": row.get::<Uuid, _>("company_id"),
                    "issueId": row.get::<Uuid, _>("issue_id"),
                    "plan": row.get::<Value, _>("plan"),
                    "acceptedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("accepted_at"),
                    "acceptedByType": row.get::<Option<String>, _>("accepted_by_type"),
                    "acceptedById": row.get::<Option<Uuid>, _>("accepted_by_id"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

/// I7: POST /issues/:id/accepted-plan-decompositions
async fn submit_plan_decomposition(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let revision_id = payload
        .get("acceptedPlanRevisionId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let children = payload
        .get("children")
        .and_then(Value::as_array)
        .ok_or(StatusCode::BAD_REQUEST)?;
    if children.is_empty() || children.len() > 25 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let plan = json!({
        "acceptedPlanRevisionId": revision_id,
        "children": children,
    });
    let (accepted_by_type, accepted_by_id) = match actor {
        AuthorizationActor::Board { user_id, .. } => ("user", user_id),
        AuthorizationActor::Agent { agent_id, .. } => ("agent", agent_id),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let row = sqlx::query(
        "INSERT INTO plan_decompositions
             (company_id, issue_id, plan, accepted_at, accepted_by_type, accepted_by_id)
         VALUES ($1, $2, $3, NOW(), $4, $5)
         RETURNING id, company_id, issue_id, plan, accepted_at,
                   accepted_by_type, accepted_by_id, created_at, updated_at",
    )
    .bind(company_id)
    .bind(id)
    .bind(plan)
    .bind(accepted_by_type)
    .bind(accepted_by_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to create plan decomposition");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "issueId": row.get::<Uuid, _>("issue_id"),
            "plan": row.get::<Value, _>("plan"),
            "acceptedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("accepted_at"),
            "acceptedByType": row.get::<Option<String>, _>("accepted_by_type"),
            "acceptedById": row.get::<Option<Uuid>, _>("accepted_by_id"),
            "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
        })),
    ))
}

/// I8: GET /issues/:id/approvals
async fn list_issue_approvals(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT a.id, a.company_id, a.approval_type::text AS approval_type,
                a.requested_by_agent_id, a.requested_by_user_id, a.status::text AS status,
                a.payload, a.decision_note, a.decided_by_user_id, a.decided_at,
                a.created_at, a.updated_at
         FROM approvals a
         JOIN issue_approvals ia ON ia.approval_id = a.id
         WHERE ia.issue_id = $1 AND a.company_id = $2
         ORDER BY a.created_at DESC",
    )
    .bind(id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list issue approvals");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows.into_iter().map(approval_json).collect()))
}

/// I9: POST /issues/:id/approvals
async fn create_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let approval_type = payload
        .get("type")
        .or_else(|| payload.get("approvalType"))
        .cloned()
        .ok_or(StatusCode::BAD_REQUEST)
        .and_then(|value| {
            serde_json::from_value::<models::approval::ApprovalType>(value)
                .map_err(|_| StatusCode::BAD_REQUEST)
        })?;
    let approval_payload = payload
        .get("payload")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    let (requested_by_agent_id, requested_by_user_id) = match actor {
        AuthorizationActor::Board { user_id, .. } => (None, Some(user_id)),
        AuthorizationActor::Agent { agent_id, .. } => (Some(agent_id), None),
        AuthorizationActor::None => return Err(StatusCode::UNAUTHORIZED),
    };
    let mut transaction = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let approval = sqlx::query(
        "INSERT INTO approvals
             (company_id, approval_type, requested_by_agent_id, requested_by_user_id,
              status, payload)
         VALUES ($1, $2, $3, $4, 'pending', $5)
         RETURNING id, company_id, approval_type::text AS approval_type,
                   requested_by_agent_id, requested_by_user_id, status::text AS status,
                   payload, decision_note, decided_by_user_id, decided_at,
                   created_at, updated_at",
    )
    .bind(company_id)
    .bind(approval_type)
    .bind(requested_by_agent_id)
    .bind(requested_by_user_id)
    .bind(approval_payload)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to create issue approval");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO issue_approvals (approval_id, issue_id, company_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (approval_id, issue_id) DO NOTHING",
    )
    .bind(approval.get::<Uuid, _>("id"))
    .bind(id)
    .bind(company_id)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to link issue approval");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    transaction
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(approval_json(approval))))
}

/// I10: DELETE /issues/:id/approvals/:approval_id
async fn delete_issue_approval(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((id, approval_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let result = sqlx::query(
        "DELETE FROM issue_approvals ia
         USING approvals a
         WHERE ia.approval_id = $1 AND ia.issue_id = $2
           AND ia.company_id = $3 AND a.id = ia.approval_id
           AND a.company_id = $3",
    )
    .bind(approval_id)
    .bind(id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, approval_id = %approval_id, "failed to delete issue approval link");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

fn approval_json(row: sqlx::postgres::PgRow) -> serde_json::Value {
    json!({
        "id": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "type": row.get::<String, _>("approval_type"),
        "requestedByAgentId": row.get::<Option<Uuid>, _>("requested_by_agent_id"),
        "requestedByUserId": row.get::<Option<Uuid>, _>("requested_by_user_id"),
        "status": row.get::<String, _>("status"),
        "payload": row.get::<Value, _>("payload"),
        "decisionNote": row.get::<Option<String>, _>("decision_note"),
        "decidedByUserId": row.get::<Option<Uuid>, _>("decided_by_user_id"),
        "decidedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    })
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

    // Transcript events are stored in the canonical activity_logs table.
    let events = sqlx::query(
        "SELECT id, event_type, actor_type, actor_id, metadata,
                created_at, agent_id
         FROM activity_logs
         WHERE resource_type = 'issue' AND resource_id = $1 AND company_id = $2
         ORDER BY created_at ASC",
    )
    .bind(id)
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut transcript = Vec::new();
    for row in events {
        let action: String = row.try_get("event_type").unwrap_or_default();
        let actor_type: String = row.try_get("actor_type").unwrap_or_default();
        let actor_id: Uuid = row
            .try_get("actor_id")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let details: serde_json::Value = row
            .try_get("metadata")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at")
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let agent_id: Option<Uuid> = row.try_get("agent_id").ok().flatten();

        transcript.push(serde_json::json!({
            "action": action,
            "details": details,
            "createdAt": created_at,
            "actorType": actor_type,
            "actorId": actor_id,
            "agentId": agent_id,
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
    let parent_company_id = scoped_issue_company(&state, &actor, parent_id).await?;
    input.company_id = parent_company_id;

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
                idempotency_key: None,
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
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    sqlx::query(
        "DELETE FROM issue_read_status WHERE company_id = $1 AND issue_id = $2 AND user_id = $3",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I14: POST /issues/:id/inbox-archive
async fn archive_issue_inbox(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    sqlx::query(
        "INSERT INTO issue_inbox_archives (company_id, issue_id, user_id)
         VALUES ($1, $2, $3)
         ON CONFLICT (issue_id, user_id) DO UPDATE
         SET archived_at = NOW(), updated_at = NOW()",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I15: DELETE /issues/:id/inbox-archive
async fn unarchive_issue_inbox(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<StatusCode, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(StatusCode::FORBIDDEN),
    };
    sqlx::query(
        "DELETE FROM issue_inbox_archives
         WHERE company_id = $1 AND issue_id = $2 AND user_id = $3",
    )
    .bind(company_id)
    .bind(id)
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

/// I16: POST /issues/:id/monitor/check-now
async fn monitor_check_now(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let scheduled_by = if actor.is_board() { "board" } else { "assignee" };
    let row = sqlx::query(
        "UPDATE issues
         SET monitor_next_check_at = NOW(),
             monitor_notes = 'manual monitor check requested',
             monitor_scheduled_by = $3::issue_monitor_scheduled_by,
             updated_at = NOW()
         WHERE id = $1 AND company_id = $2
         RETURNING monitor_next_check_at, monitor_attempt_count",
    )
    .bind(id)
    .bind(company_id)
    .bind(scheduled_by)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to schedule manual monitor check");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let actor_id = actor.principal_id().ok_or(StatusCode::UNAUTHORIZED)?;
    sqlx::query(
        "INSERT INTO activity_logs
             (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, agent_id)
         VALUES ($1, 'monitor_check_requested', $2, $3, 'issue', $4, $5, $6)",
    )
    .bind(company_id)
    .bind(actor.actor_type())
    .bind(actor_id)
    .bind(id)
    .bind(json!({ "scheduledBy": scheduled_by }))
    .bind(match actor {
        AuthorizationActor::Agent { agent_id, .. } => Some(agent_id),
        _ => None,
    })
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to record monitor check request");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "ok": true,
        "issueId": id,
        "monitorCheckTriggered": true,
        "monitorNextCheckAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("monitor_next_check_at"),
        "monitorAttemptCount": row.get::<i32, _>("monitor_attempt_count"),
    })))
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
        .wakeup_with_options(
            agent_id,
            id,
            company_id,
            HeartbeatWakeupOptions {
                source: Some("on_demand".to_string()),
                trigger_detail: Some("system".to_string()),
                reason: Some("scheduled_retry_retry_now".to_string()),
                payload: Some(json!({
                    "issueId": id,
                    "mutation": "scheduled_retry_retry_now",
                })),
                context_snapshot: Some(json!({
                    "issueId": id,
                    "source": "issue.retry_now",
                })),
                ..Default::default()
            },
        )
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let _company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT id, object_type, object_id, summary, created_at, updated_at
         FROM issue_external_objects
         WHERE issue_id = $1
         ORDER BY created_at ASC, id ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to list issue external objects");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                json!({
                    "id": row.get::<Uuid, _>("id"),
                    "objectType": row.get::<String, _>("object_type"),
                    "objectId": row.get::<String, _>("object_id"),
                    "summary": row.get::<Option<Value>, _>("summary"),
                    "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                    "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
                })
            })
            .collect(),
    ))
}

/// I19: GET /issues/:id/external-object-summary
async fn get_external_object_summary(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let _company_id = scoped_issue_company(&state, &actor, id).await?;
    let rows = sqlx::query(
        "SELECT object_type, COUNT(*)::bigint AS object_count
         FROM issue_external_objects
         WHERE issue_id = $1
         GROUP BY object_type
         ORDER BY object_type ASC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to summarize issue external objects");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let mut by_type = serde_json::Map::new();
    let mut total = 0_i64;
    for row in rows {
        let object_type = row.get::<String, _>("object_type");
        let count = row.get::<i64, _>("object_count");
        total += count;
        by_type.insert(object_type, json!(count));
    }
    Ok(Json(json!({
        "issueId": id,
        "externalObjectCount": total,
        "byObjectType": Value::Object(by_type),
    })))
}

/// I20: POST /issues/:id/external-objects/refresh
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefreshExternalObjectsInput {
    #[serde(default)]
    object_ids: Option<Vec<Uuid>>,
}

async fn refresh_external_objects(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Json(input): Json<RefreshExternalObjectsInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    if input.object_ids.as_ref().is_some_and(|ids| ids.len() > 50) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = sqlx::query(
        "UPDATE external_objects AS object
         SET next_refresh_at = NOW(), refresh_started_at = NULL, updated_at = NOW()
         WHERE object.company_id = $1
           AND object.id IN (
               SELECT mention.object_id
               FROM external_object_mentions AS mention
               WHERE mention.company_id = $1
                 AND mention.source_issue_id = $2
                 AND mention.object_id IS NOT NULL
           )
           AND ($3::uuid[] IS NULL OR object.id = ANY($3))
         RETURNING object.id, object.object_type, object.external_id, object.next_refresh_at",
    )
    .bind(company_id)
    .bind(id)
    .bind(input.object_ids.as_deref())
    .fetch_all(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to schedule external object refresh");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let object_ids: Vec<Uuid> = rows.iter().map(|row| row.get("id")).collect();
    sqlx::query(
        "INSERT INTO activity_logs (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata)
         VALUES ($1, 'external_object_refresh_requested', $2, $3, 'issue', $4, $5)",
    )
    .bind(company_id)
    .bind(actor.actor_type())
    .bind(actor.principal_id().unwrap_or_else(Uuid::nil))
    .bind(id)
    .bind(json!({ "objectIds": object_ids }))
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, issue_id = %id, "failed to record external object refresh request");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({
        "issueId": id,
        "refreshed": [],
        "scheduled": rows.into_iter().map(|row| json!({
            "id": row.get::<Uuid, _>("id"),
            "objectType": row.get::<String, _>("object_type"),
            "objectId": row.get::<String, _>("external_id"),
            "nextRefreshAt": row.get::<chrono::DateTime<chrono::Utc>, _>("next_refresh_at"),
        })).collect::<Vec<_>>(),
    })))
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
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(issue_id): IssueId,
    Query(query): Query<FileResourceListQuery>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, issue_id).await?;
    let root = workspace_file_root(
        &state,
        company_id,
        issue_id,
        query.workspace.as_deref().unwrap_or("auto"),
        query.project_id,
        query.workspace_id,
    )
    .await?;
    if let Some(mode) = query.mode.as_deref() {
        if !matches!(mode, "all" | "recent" | "changed") {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let relative = query.path.as_deref().unwrap_or("");
    validate_workspace_file_path(relative)?;
    let target = canonical_workspace_path(&root.cwd, relative).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !metadata.is_dir() {
        return Ok(Json(vec![workspace_file_payload(&root, &target, relative, &metadata)]));
    }

    let mut entries = tokio::fs::read_dir(&target)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let needle = query.q.as_deref().map(str::to_ascii_lowercase);
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(200);
    let mut matched = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == ".env" || name.starts_with('.') {
            continue;
        }
        if let Some(needle) = needle.as_deref() {
            if !name.to_ascii_lowercase().contains(needle) {
                continue;
            }
        }
        let entry_path = entry.path();
        let entry_metadata = tokio::fs::metadata(&entry_path)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        matched.push((name, entry_path, entry_metadata));
    }
    matched.sort_by(|left, right| left.0.cmp(&right.0));
    let items = matched
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(name, path, metadata)| {
            let child_relative = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            workspace_file_payload(&root, &path, &child_relative, &metadata)
        })
        .collect::<Vec<_>>();
    Ok(Json(items))
}

/// I22: GET /issues/:id/file-resources/resolve
async fn resolve_file_resource(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Query(query): Query<FileResourceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let root = workspace_file_root(
        &state,
        company_id,
        id,
        query.workspace.as_deref().unwrap_or("auto"),
        query.project_id,
        query.workspace_id,
    )
    .await?;
    validate_workspace_file_path(&query.path)?;
    let target = canonical_workspace_path(&root.cwd, &query.path).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(workspace_file_payload(
        &root,
        &target,
        &query.path,
        &metadata,
    )))
}

/// I23: GET /issues/:id/file-resources/content
async fn get_file_resource_content(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    IssueId(id): IssueId,
    Query(query): Query<FileResourceQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let company_id = scoped_issue_company(&state, &actor, id).await?;
    let root = workspace_file_root(
        &state,
        company_id,
        id,
        query.workspace.as_deref().unwrap_or("auto"),
        query.project_id,
        query.workspace_id,
    )
    .await?;
    validate_workspace_file_path(&query.path)?;
    let target = canonical_workspace_path(&root.cwd, &query.path).await?;
    let metadata = tokio::fs::metadata(&target)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if metadata.is_dir() || metadata.len() > 1_048_576 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let content = tokio::fs::read_to_string(&target)
        .await
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    Ok(Json(json!({
        "resource": workspace_file_payload(&root, &target, &query.path, &metadata),
        "content": content,
        "truncated": false,
    })))
}

async fn workspace_file_root(
    state: &AppState,
    company_id: Uuid,
    issue_id: Uuid,
    workspace: &str,
    project_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
) -> Result<WorkspaceFileRoot, StatusCode> {
    if !matches!(workspace, "auto" | "execution" | "project") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let issue_project_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT project_id FROM issues WHERE id = $1 AND company_id = $2",
    )
    .bind(issue_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query(
        "SELECT workspace_kind, id, project_id, cwd, name, provider_type, project_name
         FROM (
           SELECT 'execution_workspace' AS workspace_kind, ew.id, ew.project_id,
                  ew.cwd, ew.name, ew.provider_type, p.name AS project_name
           FROM execution_workspaces ew
           JOIN projects p ON p.id = ew.project_id
           WHERE ew.company_id = $1
             AND ($2::uuid IS NULL OR ew.project_id = $2)
             AND ($3::uuid IS NULL OR ew.id = $3)
             AND $4 <> 'project'
             AND ew.status NOT IN ('closed', 'cleaned')
           UNION ALL
           SELECT 'project_workspace' AS workspace_kind, pw.id, pw.project_id,
                  pw.config->>'cwd', pw.name, 'local_fs', p.name AS project_name
           FROM project_workspaces pw
           JOIN projects p ON p.id = pw.project_id
           WHERE p.company_id = $1
             AND ($2::uuid IS NULL OR pw.project_id = $2)
             AND ($3::uuid IS NULL OR pw.id = $3)
             AND $4 <> 'execution'
         ) candidates
         WHERE cwd IS NOT NULL
         ORDER BY CASE WHEN workspace_kind = 'execution_workspace' THEN 0 ELSE 1 END, id
         LIMIT 1",
    )
    .bind(company_id)
    .bind(project_id.or(issue_project_id))
    .bind(workspace_id)
    .bind(workspace)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let cwd = row.get::<String, _>("cwd");
    let cwd = tokio::fs::canonicalize(cwd)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(WorkspaceFileRoot {
        workspace_kind: row.get("workspace_kind"),
        workspace_id: row.get("id"),
        project_id: row.get("project_id"),
        cwd,
        name: row.get("name"),
        provider: row.get("provider_type"),
        project_name: row.get("project_name"),
    })
}

fn validate_workspace_file_path(path: &str) -> Result<(), StatusCode> {
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.split(['/', '\\']).any(|part| {
            part == ".." || part.contains('\0') || part.chars().any(|c| c.is_control())
        })
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

async fn canonical_workspace_path(root: &FsPath, relative: &str) -> Result<PathBuf, StatusCode> {
    let candidate = tokio::fs::canonicalize(root.join(relative))
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !candidate.starts_with(root)
        || candidate
            .components()
            .any(|part| part.as_os_str() == ".git" || part.as_os_str() == ".env")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(candidate)
}

fn workspace_file_payload(
    root: &WorkspaceFileRoot,
    path: &FsPath,
    relative: &str,
    metadata: &std::fs::Metadata,
) -> serde_json::Value {
    let is_dir = metadata.is_dir();
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
    let preview_kind = if is_dir {
        "unsupported"
    } else if ["png", "jpg", "jpeg", "gif", "webp"].contains(&extension) {
        "image"
    } else {
        "text"
    };
    json!({
        "kind": if is_dir { "directory" } else { "file" },
        "provider": root.provider,
        "title": path.file_name().and_then(|value| value.to_str()).unwrap_or(relative),
        "displayPath": relative,
        "workspaceLabel": root.name,
        "workspaceKind": root.workspace_kind,
        "workspaceId": root.workspace_id,
        "projectId": root.project_id,
        "projectName": root.project_name,
        "contentType": if is_dir { Value::Null } else { json!("application/octet-stream") },
        "byteSize": if is_dir { Value::Null } else { json!(metadata.len()) },
        "previewKind": preview_kind,
        "capabilities": {
            "preview": !is_dir,
            "download": !is_dir,
            "listChildren": is_dir,
        },
    })
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
        .route("/issues/:id/relations", get(get_issue_relations))
        .route(
            "/issues/:id/relations/blocked-by",
            post(update_blocked_by_relations),
        )
        .route(
            "/issues/:id/relations/:relation_id",
            delete(delete_issue_relation),
        )
        .route("/issues/:id/cases", get(get_issue_cases))
        .route("/issues/:id/active-run", get(get_issue_active_run))
        .route("/issues/:id/live-runs", get(get_issue_live_runs))
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
