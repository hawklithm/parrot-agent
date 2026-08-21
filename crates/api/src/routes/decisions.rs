//! P0.1 Decision / Attention / Triage / Retention / Training routes.
//!
//! 对齐 Paperclip:
//!   - `server/src/routes/attention.ts`        → GET /companies/:company_id/attention
//!   - `server/src/routes/decisions.ts`        → decisions CRUD + decide/dismiss/cancel/stats
//!   - `server/src/routes/decision-queues.ts`  → queues / items / triage / retention
//!   - `server/src/routes/decision-training.ts`→ training snapshot / list / export
//!
//! 路由路径遵循 Parrot 约定使用 `:company_id`（Paperclip 为 `:companyId`）。
//! 所有查询均为针对 `state.pool` 的真实 sqlx 查询，无 mock。

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Json, Router,
};
use base64::Engine as _;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::errors::AppError;
use services::auth::{decide_access, AuthorizationAction, AuthorizationActor};
use services::decision_training_service::{
    DecisionTrainingSourceKind, PersistSnapshotInput, TrainingError,
};

// ---------------------------------------------------------------------------
// Constants (ported verbatim from Paperclip)
// ---------------------------------------------------------------------------

/// `packages/shared/src/types/attention.ts` → ATTENTION_SOURCE_KINDS
const ATTENTION_SOURCE_KINDS: [&str; 11] = [
    "approval",
    "decision",
    "issue_thread_interaction",
    "join_request",
    "recovery_action",
    "productivity_review",
    "blocker_attention",
    "review",
    "failed_run",
    "budget_alert",
    "agent_error_alert",
];

/// `server/src/services/decision-retention.ts`
const DEFAULT_DECISION_SHELF_DAYS: i64 = 30;
#[allow(dead_code)]
const DEFAULT_DECISION_ARCHIVE_DAYS: i64 = 90;

/// `server/src/services/decisions.ts` → PAPERCLIP_DECISIONS_OPEN_CAP default
const OPEN_DECISION_CAP: i64 = 50;
const DETAIL_EXCERPT_LENGTH: usize = 160;
const ATTENTION_PAGE_DEFAULT_LIMIT: usize = 50;
const ATTENTION_PAGE_MAX_LIMIT: usize = 100;
const ATTENTION_SOURCE_SCAN_LIMIT: i64 = 500;

/// `server/src/services/decision-training.ts` → DECISION_TRAINING_RETENTION_POLICY
const DECISION_TRAINING_RETENTION_POLICY: &str = "scrub_deleted_comments_v1";

/// `server/src/services/decision-queues.ts` → DECISION_QUEUE_SEEDS
fn decision_queue_seeds() -> Value {
    json!([
        {
            "key": "prs",
            "title": "PRs",
            "description": "Issues with an open pull-request work product waiting on review.",
            "rules": [{ "key": "issue-pull-request-work-product", "signal": "issue_has_pull_request_work_product" }]
        },
        {
            "key": "plans",
            "title": "Plans",
            "description": "Plan documents waiting on a board confirmation.",
            "rules": [{ "key": "plan-document-confirmation", "signal": "plan_document_confirmation" }]
        },
        {
            "key": "questions",
            "title": "Questions",
            "description": "Agents asking the board a direct question.",
            "rules": [{ "key": "ask-user-questions", "signal": "ask_user_questions" }]
        }
    ])
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn decision_routes() -> Router<AppState> {
    Router::new()
        // ---- Attention feed -------------------------------------------------
        .route("/companies/:company_id/attention", get(get_attention_feed))
        // ---- Decisions ------------------------------------------------------
        .route(
            "/companies/:company_id/decisions",
            get(list_decisions).post(create_decision),
        )
        .route(
            "/companies/:company_id/decisions/stats",
            get(decision_stats),
        )
        .route(
            "/companies/:company_id/decision-bundles",
            post(create_decision_bundle),
        )
        .route(
            "/companies/:company_id/decision-archive-proposals",
            post(create_decision_archive_proposal),
        )
        .route("/decisions/:decision_id", get(get_decision))
        .route("/decisions/:decision_id/decide", post(decide_decision))
        .route("/decisions/:decision_id/dismiss", post(dismiss_decision))
        .route("/decisions/:decision_id/cancel", post(cancel_decision))
        // ---- Decision queues ------------------------------------------------
        .route(
            "/companies/:company_id/decision-queue-seed-rules",
            get(get_decision_queue_seed_rules),
        )
        .route(
            "/companies/:company_id/decision-queues",
            get(list_decision_queues).post(create_decision_queue),
        )
        .route(
            "/companies/:company_id/decision-queues/:key",
            patch(update_decision_queue),
        )
        .route(
            "/companies/:company_id/decision-queues/:key/items",
            get(list_decision_queue_items).post(add_decision_queue_item),
        )
        .route(
            "/companies/:company_id/decision-queues/:key/items/:source_kind/:source_id",
            delete(remove_decision_queue_item),
        )
        // ---- Triage ---------------------------------------------------------
        .route(
            "/companies/:company_id/decision-triage/:source_kind/:source_id",
            get(get_decision_triage).put(put_decision_triage),
        )
        // ---- Retention ------------------------------------------------------
        .route(
            "/companies/:company_id/decision-retention/:source_kind/:source_id",
            patch(patch_decision_retention),
        )
        .route(
            "/companies/:company_id/decision-retention/:source_kind/:source_id/archive",
            post(archive_decision_retention),
        )
        .route(
            "/companies/:company_id/decision-retention/:source_kind/:source_id/revive",
            post(revive_decision_retention),
        )
        // ---- Training -------------------------------------------------------
        .route(
            "/companies/:company_id/decision-training",
            get(list_decision_training).post(create_decision_training),
        )
        .route(
            "/companies/:company_id/decision-training/preview",
            post(preview_decision_training),
        )
        .route(
            "/companies/:company_id/decision-training/export.jsonl",
            get(export_decision_training),
        )
        .route(
            "/decision-training/:example_id",
            get(get_decision_training)
                .patch(update_decision_training)
                .delete(delete_decision_training),
        )
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Map a sqlx error to the correct HTTP status (404/409/400/500) instead of
/// collapsing everything to 500. See `impl From<sqlx::Error> for AppError`.
fn db_err(e: sqlx::Error) -> AppError {
    e.into()
}

fn training_service_err(error: TrainingError) -> AppError {
    match error {
        TrainingError::InvalidSnapshot(message) => AppError::Validation(message),
        TrainingError::ExampleNotFound(_) | TrainingError::SourceNotFound { .. } => {
            training_not_found()
        }
        TrainingError::DatabaseError(message) | TrainingError::SerializationError(message) => {
            AppError::InternalServerError(message)
        }
    }
}

fn training_source_kind(value: &str) -> Result<DecisionTrainingSourceKind, AppError> {
    match value {
        "interaction" => Ok(DecisionTrainingSourceKind::IssueThreadInteraction),
        "approval" => Ok(DecisionTrainingSourceKind::IssueApproval),
        "execution_decision" => Ok(DecisionTrainingSourceKind::IssueExecutionDecision),
        _ => Err(AppError::Validation(format!(
            "sourceKind must be one of {TRAINING_SOURCE_KINDS:?}"
        ))),
    }
}

fn forbid(msg: &str) -> AppError {
    AppError::Forbidden(msg.to_string())
}

fn require_company_access(
    actor: &AuthorizationActor,
    company_id: Uuid,
    read_only: bool,
) -> Result<(), AppError> {
    crate::routes::assert_company_access(actor, company_id, read_only)
        .map_err(|_| forbid("Company access denied"))
}

fn require_board(actor: &AuthorizationActor) -> Result<(), AppError> {
    crate::routes::assert_board(actor).map_err(|_| forbid("Board session required"))
}

fn iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn iso_opt(dt: Option<DateTime<Utc>>) -> Value {
    match dt {
        Some(v) => Value::String(iso(v)),
        None => Value::Null,
    }
}

fn parse_iso(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Paperclip `excerpt()` — trims + truncates to DETAIL_EXCERPT_LENGTH with an ellipsis.
fn excerpt(value: Option<&str>) -> Value {
    let cleaned = match value.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => return Value::Null,
    };
    if cleaned.chars().count() <= DETAIL_EXCERPT_LENGTH {
        return Value::String(cleaned.to_string());
    }
    let head: String = cleaned.chars().take(DETAIL_EXCERPT_LENGTH - 1).collect();
    Value::String(format!("{}...", head.trim_end()))
}

fn json_str(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn verb(id: &str, label: &str, description: &str) -> Value {
    json!({ "id": id, "label": label, "description": description })
}

fn is_valid_source_kind(kind: &str) -> bool {
    ATTENTION_SOURCE_KINDS.contains(&kind)
}

/// Match Paperclip's `canReadDecisionSource`: retention mutations must not
/// manufacture state for a missing attention source, and company membership
/// alone is not enough for issue/agent-scoped sources.
async fn require_decision_source_read(
    pool: &PgPool,
    actor: &AuthorizationActor,
    company_id: Uuid,
    source_kind: &str,
    source_id: &str,
) -> Result<(), AppError> {
    let source_uuid = Uuid::parse_str(source_id)
        .map_err(|_| AppError::NotFound("Attention source not found".to_string()))?;

    let source = match source_kind {
        "approval" => sqlx::query(
            "SELECT ia.issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM approvals a
               LEFT JOIN issue_approvals ia
                 ON ia.approval_id = a.id AND ia.company_id = $1
              WHERE a.company_id = $1 AND a.id = $2
              LIMIT 1",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "decision" => sqlx::query(
            "SELECT origin_issue_id AS issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM decisions
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "issue_thread_interaction" => sqlx::query(
            "SELECT issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM issue_thread_interactions
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "recovery_action" => sqlx::query(
            "SELECT issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM recovery_actions
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "productivity_review" | "blocker_attention" | "review" => sqlx::query(
            "SELECT id AS issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM issues
              WHERE company_id = $1 AND id = $2 AND hidden_at IS NULL",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "failed_run" => sqlx::query(
            "SELECT NULL::uuid AS issue_id, agent_id, context_snapshot
               FROM heartbeat_runs
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "agent_error_alert" => sqlx::query(
            "SELECT NULL::uuid AS issue_id, id AS agent_id, NULL::jsonb AS context_snapshot
               FROM agents
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "join_request" => sqlx::query(
            "SELECT NULL::uuid AS issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM join_requests
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        "budget_alert" => sqlx::query(
            "SELECT NULL::uuid AS issue_id, NULL::uuid AS agent_id, NULL::jsonb AS context_snapshot
               FROM budget_incidents
              WHERE company_id = $1 AND id = $2",
        )
        .bind(company_id)
        .bind(source_uuid)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?,
        _ => None,
    };

    let Some(source) = source else {
        return Err(AppError::NotFound("Attention source not found".to_string()));
    };
    let context_snapshot: Option<Value> = source.try_get("context_snapshot").map_err(db_err)?;
    let issue_id: Option<Uuid> = source
        .try_get::<Option<Uuid>, _>("issue_id")
        .map_err(db_err)?
        .or_else(|| {
            context_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.get("issueId").or_else(|| snapshot.get("taskId")))
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
        });
    if let Some(issue_id) = issue_id {
        let issue_exists = sqlx::query(
            "SELECT 1 FROM issues WHERE company_id = $1 AND id = $2 AND hidden_at IS NULL",
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?
        .is_some();
        if !issue_exists
            || !decide_access(
                pool,
                actor,
                &AuthorizationAction::IssueRead { issue_id },
                Some(company_id),
            )
            .await
        {
            return Err(AppError::NotFound("Attention source not found".to_string()));
        }
        return Ok(());
    }

    let agent_id: Option<Uuid> = source.try_get("agent_id").map_err(db_err)?;
    if source_kind == "agent_error_alert" {
        let Some(agent_id) = agent_id else {
            return Err(AppError::NotFound("Attention source not found".to_string()));
        };
        if !decide_access(
            pool,
            actor,
            &AuthorizationAction::AgentRead { agent_id },
            Some(company_id),
        )
        .await
        {
            return Err(AppError::NotFound("Attention source not found".to_string()));
        }
        return Ok(());
    }

    if source_kind == "failed_run" {
        let Some(owner_agent_id) = agent_id else {
            return Err(AppError::NotFound("Attention source not found".to_string()));
        };
        if let AuthorizationActor::Agent { agent_id, .. } = actor {
            if *agent_id != owner_agent_id {
                return Err(AppError::NotFound("Attention source not found".to_string()));
            }
            return Ok(());
        }
        if !actor.is_board() {
            return Err(AppError::NotFound("Attention source not found".to_string()));
        }
        return Ok(());
    }

    if !actor.is_board() {
        return Err(AppError::NotFound("Attention source not found".to_string()));
    }
    Ok(())
}

/// Actor attribution used by every `*_by_type` / `*_by_agent_id` / `*_by_user_id` column trio.
struct Attribution {
    actor_type: &'static str,
    agent_id: Option<Uuid>,
    user_id: Option<String>,
    run_id: Option<Uuid>,
    api_key_id: Option<Uuid>,
    responsible_user_id: Option<String>,
}

fn attribution(actor: &AuthorizationActor) -> Attribution {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Attribution {
            actor_type: "user",
            agent_id: None,
            user_id: Some(user_id.to_string()),
            run_id: None,
            api_key_id: None,
            responsible_user_id: Some(user_id.to_string()),
        },
        AuthorizationActor::Agent {
            agent_id,
            run_id,
            key_id,
            responsible_user_id,
            ..
        } => Attribution {
            actor_type: "agent",
            agent_id: Some(*agent_id),
            user_id: None,
            run_id: *run_id,
            api_key_id: *key_id,
            responsible_user_id: responsible_user_id.as_ref().map(|u| u.to_string()),
        },
        AuthorizationActor::None => Attribution {
            actor_type: "system",
            agent_id: None,
            user_id: None,
            run_id: None,
            api_key_id: None,
            responsible_user_id: None,
        },
    }
}

/// `(agent_id, run_id)` for the endpoints Paperclip restricts to an agent run context.
fn agent_context(actor: &AuthorizationActor) -> Result<(Uuid, Option<Uuid>), AppError> {
    match actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => Ok((*agent_id, *run_id)),
        _ => Err(forbid("Agent run context required")),
    }
}

async fn log_activity(
    pool: &PgPool,
    company_id: Uuid,
    actor: &AuthorizationActor,
    event_type: &str,
    resource_type: &str,
    resource_id: Uuid,
    metadata: Value,
) {
    // `activity_logs.actor_id` is NOT NULL — only anonymous/system actors are skipped.
    let Some(actor_id) = actor.principal_id() else {
        return;
    };
    let _ = sqlx::query(
        "INSERT INTO activity_logs (company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(company_id)
    .bind(event_type)
    .bind(actor.actor_type())
    .bind(actor_id)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .execute(pool)
    .await;
}

// ---------------------------------------------------------------------------
// Attention aggregation
// ---------------------------------------------------------------------------

fn severity_rank(severity: &str) -> i32 {
    match severity {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        _ => 3,
    }
}

fn source_rank(kind: &str) -> i32 {
    match kind {
        "failed_run" => 0,
        "recovery_action" => 1,
        "blocker_attention" => 2,
        "budget_alert" => 3,
        "agent_error_alert" => 4,
        "approval" => 5,
        "decision" => 6,
        "issue_thread_interaction" => 7,
        "review" => 8,
        "productivity_review" => 9,
        "join_request" => 10,
        _ => 99,
    }
}

/// Paperclip `createItem()` — fills every optional field with its documented default.
#[allow(clippy::too_many_arguments)]
fn create_item(
    company_id: Uuid,
    source_kind: &str,
    subject: Value,
    why_now: &str,
    decision_verbs: Vec<Value>,
    inline_resolvable: bool,
    entry_rule: &str,
    exit_rule: &str,
    dedup_key: String,
    severity: &str,
    activity_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    related_issue: Value,
    project: Value,
    workspace: Value,
    expires_at: Value,
    rule_key: Value,
    detail: Value,
) -> Value {
    json!({
        "id": format!("{source_kind}:{dedup_key}"),
        "companyId": company_id,
        "sourceKind": source_kind,
        "subject": subject,
        "whyNow": why_now,
        "decisionVerbs": decision_verbs,
        "inlineResolvable": inline_resolvable,
        "entryRule": entry_rule,
        "exitRule": exit_rule,
        "dedupKey": dedup_key,
        "dismissalKey": format!("attention:{dedup_key}"),
        "dismissal": Value::Null,
        "severity": severity,
        "rank": 0,
        "activityAt": iso(activity_at),
        "createdAt": iso(created_at),
        "updatedAt": iso(updated_at),
        "relatedIssue": related_issue,
        "project": project,
        "workspace": workspace,
        "expiresAt": expires_at,
        "ruleKey": rule_key,
        "originAgentName": Value::Null,
        "queues": Value::Array(vec![]),
        "shelf": false,
        "retentionDays": DEFAULT_DECISION_SHELF_DAYS,
        "keep": false,
        "archivedAt": Value::Null,
        "retentionVersion": 0,
        "decideBy": Value::Null,
        "decideByAttribution": Value::Null,
        "snoozedUntil": Value::Null,
        "detail": detail,
        "trainingExampleId": Value::Null,
    })
}

struct IssueRef {
    subject: Value,
    project: Value,
}

/// Batch-load the issues referenced by attention items (+ their project refs).
async fn load_issue_refs(
    pool: &PgPool,
    company_id: Uuid,
    issue_ids: &[Uuid],
) -> Result<HashMap<Uuid, IssueRef>, AppError> {
    let mut map = HashMap::new();
    if issue_ids.is_empty() {
        return Ok(map);
    }
    let rows = sqlx::query(
        "SELECT i.id, i.company_id, i.title, i.identifier, i.status::text AS status, \
                i.priority::text AS priority, i.assignee_agent_id, i.assignee_user_id, \
                i.project_id, p.name AS project_name, p.color AS project_color, p.icon AS project_icon \
           FROM issues i \
           LEFT JOIN projects p ON p.id = i.project_id \
          WHERE i.company_id = $1 AND i.id = ANY($2)",
    )
    .bind(company_id)
    .bind(issue_ids)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    for r in &rows {
        let id: Uuid = r.get("id");
        let identifier: Option<String> = r.try_get("identifier").unwrap_or(None);
        let href = format!(
            "/{}/issues/{}",
            company_id,
            identifier.clone().unwrap_or_else(|| id.to_string())
        );
        let subject = json!({
            "kind": "issue",
            "id": id,
            "companyId": r.get::<Uuid, _>("company_id"),
            "title": r.try_get::<Option<String>, _>("title").unwrap_or(None),
            "identifier": identifier,
            "status": r.try_get::<Option<String>, _>("status").unwrap_or(None),
            "href": href,
            "metadata": {
                "priority": r.try_get::<Option<String>, _>("priority").unwrap_or(None),
                "assigneeAgentId": r.try_get::<Option<Uuid>, _>("assignee_agent_id").unwrap_or(None),
                "assigneeUserId": r.try_get::<Option<Uuid>, _>("assignee_user_id").unwrap_or(None),
            },
        });
        let project = match r.try_get::<Option<Uuid>, _>("project_id").unwrap_or(None) {
            // Parrot has no `projects.url_key`; the project id doubles as the url key.
            Some(project_id) => json!({
                "id": project_id,
                "name": r.try_get::<Option<String>, _>("project_name").unwrap_or(None),
                "urlKey": project_id.to_string(),
                "color": r.try_get::<Option<String>, _>("project_color").unwrap_or(None),
                "icon": r.try_get::<Option<String>, _>("project_icon").unwrap_or(None),
            }),
            None => Value::Null,
        };
        map.insert(id, IssueRef { subject, project });
    }
    Ok(map)
}

/// Build the raw (un-enriched) attention items for a company from live tables.
async fn collect_attention_items(pool: &PgPool, company_id: Uuid) -> Result<Vec<Value>, AppError> {
    let mut items: Vec<Value> = Vec::new();
    let mut issue_ids: HashSet<Uuid> = HashSet::new();

    // -- approvals ----------------------------------------------------------
    let approval_rows = sqlx::query(
        "SELECT a.id, a.approval_type::text AS approval_type, a.status::text AS status, \
                a.requested_by_agent_id, a.requested_by_user_id, a.payload, \
                a.created_at, a.updated_at, \
                (SELECT ia.issue_id FROM issue_approvals ia WHERE ia.approval_id = a.id LIMIT 1) AS issue_id \
           FROM approvals a \
          WHERE a.company_id = $1 AND a.status = 'pending' \
          ORDER BY a.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- issue thread interactions -----------------------------------------
    let interaction_rows = sqlx::query(
        "SELECT i.id, i.issue_id, i.kind, i.status::text AS status, i.title, i.summary, \
                i.payload, i.created_by_agent_id, i.expires_at, i.created_at, i.updated_at \
           FROM issue_thread_interactions i \
          WHERE i.company_id = $1 AND i.status = 'pending' \
          ORDER BY i.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- open decisions -----------------------------------------------------
    let decision_rows = sqlx::query(
        "SELECT d.id, d.title, d.body, d.status, d.rule_key, d.bundle_id, d.origin_agent_id, \
                d.origin_issue_id, d.expires_at, d.created_at, d.updated_at, \
                b.title AS bundle_title \
           FROM decisions d \
           LEFT JOIN decision_bundles b ON b.id = d.bundle_id \
          WHERE d.company_id = $1 AND d.status = 'open' AND d.expires_at > NOW() \
          ORDER BY d.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- join requests ------------------------------------------------------
    let join_rows = sqlx::query(
        "SELECT j.id, j.status::text AS status, j.requester_user_id, j.message, \
                j.created_at, j.updated_at \
           FROM join_requests j \
          WHERE j.company_id = $1 AND j.status = 'pending_approval' \
          ORDER BY j.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- recovery actions ---------------------------------------------------
    let recovery_rows = sqlx::query(
        "SELECT r.id, r.issue_id, r.action_type, r.status, r.description, r.metadata, \
                r.triggered_by_issue_id, r.created_at, r.updated_at \
           FROM recovery_actions r \
          WHERE r.company_id = $1 AND r.status IN ('pending', 'in_progress') \
          ORDER BY r.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- failed runs --------------------------------------------------------
    let run_rows = sqlx::query(
        "SELECT h.id, h.agent_id, h.status::text AS status, h.error, h.context_snapshot, \
                h.finished_at, h.created_at, h.updated_at, ag.name AS agent_name \
           FROM heartbeat_runs h \
           LEFT JOIN agents ag ON ag.id = h.agent_id \
          WHERE h.company_id = $1 AND h.status IN ('failed', 'timed_out') \
          ORDER BY h.created_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // -- budget incidents ---------------------------------------------------
    let budget_rows = sqlx::query(
        "SELECT b.id, b.policy_id, b.scope_type::text AS scope_type, b.scope_id, \
                b.threshold_type::text AS threshold_type, b.amount_limit, b.amount_observed, \
                b.status::text AS status, b.approval_id, b.window_start, b.created_at, b.updated_at \
           FROM budget_incidents b \
          WHERE b.company_id = $1 AND b.status = 'open' \
          ORDER BY b.updated_at DESC LIMIT $2",
    )
    .bind(company_id)
    .bind(ATTENTION_SOURCE_SCAN_LIMIT)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    for r in &approval_rows {
        if let Some(issue_id) = r.try_get::<Option<Uuid>, _>("issue_id").unwrap_or(None) {
            issue_ids.insert(issue_id);
        }
    }
    for r in &interaction_rows {
        issue_ids.insert(r.get::<Uuid, _>("issue_id"));
    }
    for r in &decision_rows {
        issue_ids.insert(r.get::<Uuid, _>("origin_issue_id"));
    }
    for r in &recovery_rows {
        issue_ids.insert(r.get::<Uuid, _>("issue_id"));
    }
    for r in &run_rows {
        let snapshot: Option<Value> = r.try_get("context_snapshot").unwrap_or(None);
        if let Some(id) = snapshot
            .as_ref()
            .and_then(|s| s.get("issueId"))
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
        {
            issue_ids.insert(id);
        }
    }

    let issue_ref_ids: Vec<Uuid> = issue_ids.into_iter().collect();
    let issue_refs = load_issue_refs(pool, company_id, &issue_ref_ids).await?;

    // ---- approvals --------------------------------------------------------
    for r in &approval_rows {
        let id: Uuid = r.get("id");
        let approval_type: String = r.get("approval_type");
        let payload: Value = r.try_get("payload").unwrap_or(Value::Null);
        let title = json_str(&payload, "title")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| approval_type.replace('_', " "));
        let issue_id: Option<Uuid> = r.try_get("issue_id").unwrap_or(None);
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        items.push(create_item(
            company_id,
            "approval",
            json!({
                "kind": "approval",
                "id": id,
                "companyId": company_id,
                "title": title,
                "identifier": Value::Null,
                "status": r.get::<String, _>("status"),
                "href": format!("/{company_id}/approvals/{id}"),
                "metadata": {
                    "type": approval_type,
                    "requestedByAgentId": r.try_get::<Option<Uuid>, _>("requested_by_agent_id").unwrap_or(None),
                    "requestedByUserId": r.try_get::<Option<Uuid>, _>("requested_by_user_id").unwrap_or(None),
                    "issueId": issue_id,
                },
            }),
            "Approval is pending a board decision.",
            vec![
                verb("approve", "Approve", "Approve the request."),
                verb("reject", "Reject", "Reject the request."),
                verb(
                    "request_revision",
                    "Request revision",
                    "Send the request back for changes.",
                ),
            ],
            approval_type != "request_board_approval",
            "approvals.status = 'pending'",
            "Approval leaves pending status.",
            format!("approval:{id}"),
            "medium",
            updated_at,
            created_at,
            updated_at,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            json!({
                "kind": "approval",
                "approvalType": approval_type,
                "summaryExcerpt": excerpt(
                    json_str(&payload, "summary")
                        .or_else(|| json_str(&payload, "title"))
                        .or_else(|| json_str(&payload, "recommendedAction"))
                        .as_deref(),
                ),
                "images": [],
            }),
        ));
    }

    // ---- issue thread interactions ---------------------------------------
    for r in &interaction_rows {
        let id: Uuid = r.get("id");
        let issue_id: Uuid = r.get("issue_id");
        let kind: String = r.get("kind");
        let title: Option<String> = r.try_get("title").unwrap_or(None);
        let summary: Option<String> = r.try_get("summary").unwrap_or(None);
        let issue = issue_refs.get(&issue_id);
        let label = match kind.as_str() {
            "approval" => "Approval request",
            "review" => "Review request",
            _ => "Question",
        };
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let href = issue
            .map(|i| {
                json!(format!(
                    "{}#interaction-{}",
                    i.subject
                        .get("href")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default(),
                    id
                ))
            })
            .unwrap_or(Value::Null);
        let verbs = match kind.as_str() {
            "approval" => vec![
                verb("approve", "Approve", "Approve this request."),
                verb("reject", "Reject", "Reject this request."),
            ],
            "review" => vec![
                verb("accept", "Accept", "Accept the reviewed work."),
                verb(
                    "request_changes",
                    "Request changes",
                    "Send it back for changes.",
                ),
            ],
            _ => vec![verb("answer", "Answer", "Answer the agent's question.")],
        };
        items.push(create_item(
            company_id,
            "issue_thread_interaction",
            json!({
                "kind": "interaction",
                "id": id,
                "companyId": company_id,
                "title": title.clone().or_else(|| summary.clone()).unwrap_or_else(|| label.to_string()),
                "identifier": Value::Null,
                "status": r.get::<String, _>("status"),
                "href": href,
                "metadata": {
                    "kind": kind,
                    "issueId": issue_id,
                    "createdByAgentId": r.try_get::<Option<Uuid>, _>("created_by_agent_id").unwrap_or(None),
                    "isPlanTarget": false,
                    "targetDocumentKey": Value::Null,
                },
            }),
            &format!("{label} on an issue thread."),
            verbs,
            true,
            "issue_thread_interactions.status = 'pending'",
            "Interaction resolves, expires, fails, or is cancelled.",
            format!("interaction:{id}"),
            "medium",
            updated_at,
            created_at,
            updated_at,
            issue.map(|i| i.subject.clone()).unwrap_or(Value::Null),
            issue.map(|i| i.project.clone()).unwrap_or(Value::Null),
            Value::Null,
            iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("expires_at").unwrap_or(None)),
            Value::Null,
            json!({
                "kind": "generic",
                "summaryExcerpt": excerpt(summary.as_deref().or(title.as_deref())),
                "images": [],
            }),
        ));
    }

    // ---- decisions --------------------------------------------------------
    for r in &decision_rows {
        let id: Uuid = r.get("id");
        let origin_issue_id: Uuid = r.get("origin_issue_id");
        let issue = issue_refs.get(&origin_issue_id);
        let body: String = r.get("body");
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        items.push(create_item(
            company_id,
            "decision",
            json!({
                "kind": "decision",
                "id": id,
                "companyId": company_id,
                "title": r.get::<String, _>("title"),
                "identifier": Value::Null,
                "status": r.get::<String, _>("status"),
                "href": format!("/{company_id}/decisions?decisionId={id}"),
                "metadata": {
                    "originIssueId": origin_issue_id,
                    "originAgentId": r.get::<Uuid, _>("origin_agent_id"),
                    "bundleId": r.try_get::<Option<Uuid>, _>("bundle_id").unwrap_or(None),
                    "bundleTitle": r.try_get::<Option<String>, _>("bundle_title").unwrap_or(None),
                },
            }),
            "An agent decision is waiting for a board response.",
            vec![verb("decide", "Review", "Review and choose an option.")],
            true,
            "decisions.status = 'open'",
            "Decision is decided, expired, or cancelled.",
            format!("decision:{id}"),
            "medium",
            updated_at,
            created_at,
            updated_at,
            issue.map(|i| i.subject.clone()).unwrap_or(Value::Null),
            issue.map(|i| i.project.clone()).unwrap_or(Value::Null),
            Value::Null,
            iso_opt(Some(r.get::<DateTime<Utc>, _>("expires_at"))),
            r.try_get::<Option<String>, _>("rule_key")
                .unwrap_or(None)
                .map(Value::String)
                .unwrap_or(Value::Null),
            json!({
                "kind": "generic",
                "summaryExcerpt": excerpt(Some(body.as_str())),
                "images": [],
            }),
        ));
    }

    // ---- join requests ----------------------------------------------------
    for r in &join_rows {
        let id: Uuid = r.get("id");
        let requester: Uuid = r.get("requester_user_id");
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let message: Option<String> = r.try_get("message").unwrap_or(None);
        let label = format!("Join request from {requester}");
        items.push(create_item(
            company_id,
            "join_request",
            json!({
                "kind": "join_request",
                "id": id,
                "companyId": company_id,
                "title": label,
                "identifier": Value::Null,
                "status": r.get::<String, _>("status"),
                "href": format!("/{company_id}/settings/access"),
                "metadata": {
                    // Parrot's join_requests only models human membership requests.
                    "requestType": "membership",
                    "requestingUserId": requester,
                    "adapterType": Value::Null,
                },
            }),
            "Join request is pending approval.",
            vec![
                verb("approve", "Approve", "Approve this join request."),
                verb("reject", "Reject", "Reject this join request."),
            ],
            true,
            "join_requests.status = 'pending_approval'",
            "Join request is approved or rejected.",
            format!("join_request:{id}"),
            "medium",
            updated_at,
            created_at,
            updated_at,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            json!({
                "kind": "generic",
                "summaryExcerpt": excerpt(message.as_deref()),
                "images": [],
            }),
        ));
    }

    // ---- recovery actions -------------------------------------------------
    for r in &recovery_rows {
        let id: Uuid = r.get("id");
        let issue_id: Uuid = r.get("issue_id");
        let issue = issue_refs.get(&issue_id);
        let status: String = r.get("status");
        let action_type: String = r.get("action_type");
        let description: Option<String> = r.try_get("description").unwrap_or(None);
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let severity = if status == "in_progress" {
            "high"
        } else {
            "medium"
        };
        items.push(create_item(
            company_id,
            "recovery_action",
            json!({
                "kind": "recovery_action",
                "id": id,
                "companyId": company_id,
                "title": description.clone().unwrap_or_else(|| action_type.clone()),
                "identifier": Value::Null,
                "status": status,
                "href": issue.and_then(|i| i.subject.get("href").cloned()).unwrap_or(Value::Null),
                "metadata": {
                    "kind": action_type,
                    "cause": r.try_get::<Option<Value>, _>("metadata").unwrap_or(None),
                    "ownerType": "user",
                    "ownerUserId": Value::Null,
                    "sourceIssueId": r.try_get::<Option<Uuid>, _>("triggered_by_issue_id").unwrap_or(None),
                    "recoveryIssueId": issue_id,
                },
            }),
            if status == "in_progress" {
                "Recovery action escalated to a human owner."
            } else {
                "Recovery action is assigned to a human owner."
            },
            vec![
                verb("resolve", "Resolve", "Record the recovery outcome."),
                verb("reassign", "Reassign", "Move the recovery to another owner."),
                verb("cancel", "Cancel", "Cancel the recovery action."),
            ],
            false,
            "recovery_actions.status in ('pending','in_progress')",
            "Recovery action resolves, is cancelled, or moves back to an agent/system owner.",
            format!("recovery_action:{id}"),
            severity,
            updated_at,
            created_at,
            updated_at,
            issue.map(|i| i.subject.clone()).unwrap_or(Value::Null),
            issue.map(|i| i.project.clone()).unwrap_or(Value::Null),
            Value::Null,
            Value::Null,
            Value::Null,
            json!({
                "kind": "generic",
                "summaryExcerpt": excerpt(description.as_deref()),
                "images": [],
            }),
        ));
    }

    // ---- failed runs ------------------------------------------------------
    for r in &run_rows {
        let id: Uuid = r.get("id");
        let agent_id: Uuid = r.get("agent_id");
        let agent_name: Option<String> = r.try_get("agent_name").unwrap_or(None);
        let status: String = r.get("status");
        let error: Option<String> = r.try_get("error").unwrap_or(None);
        let snapshot: Option<Value> = r.try_get("context_snapshot").unwrap_or(None);
        let issue_id = snapshot
            .as_ref()
            .and_then(|s| s.get("issueId"))
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok());
        let issue = issue_id.and_then(|iid| issue_refs.get(&iid));
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let finished_at: Option<DateTime<Utc>> = r.try_get("finished_at").unwrap_or(None);
        let display_name = agent_name.clone().unwrap_or_else(|| agent_id.to_string());
        items.push(create_item(
            company_id,
            "failed_run",
            json!({
                "kind": "run",
                "id": id,
                "companyId": company_id,
                "title": format!("{display_name} run {status}"),
                "identifier": Value::Null,
                "status": status,
                "href": format!("/{company_id}/agents/{agent_id}/runs/{id}"),
                "metadata": {
                    "agentId": agent_id,
                    "agentName": agent_name,
                    "issueId": issue_id,
                    "errorCode": Value::Null,
                    "error": error,
                    "retryExhaustedReason": Value::Null,
                },
            }),
            "Run failed after automatic retries were exhausted.",
            vec![
                verb("retry", "Retry", "Retry the failed run or issue."),
                verb("reassign", "Reassign", "Move the work to another owner."),
                verb(
                    "dismiss",
                    "Dismiss",
                    "Dismiss this failed-run attention row.",
                ),
            ],
            true,
            "heartbeat_runs.status in ('failed','timed_out')",
            "A newer run exists for the same issue/agent pair or the row is dismissed.",
            format!("failed_run:{id}"),
            "high",
            finished_at.unwrap_or(updated_at),
            created_at,
            updated_at,
            issue.map(|i| i.subject.clone()).unwrap_or(Value::Null),
            issue.map(|i| i.project.clone()).unwrap_or(Value::Null),
            Value::Null,
            Value::Null,
            Value::Null,
            json!({
                "kind": "failed_run",
                "agentName": agent_name,
                "failureReasonExcerpt": excerpt(error.as_deref()),
                "images": [],
            }),
        ));
    }

    // ---- budget alerts ----------------------------------------------------
    for r in &budget_rows {
        let id: Uuid = r.get("id");
        let policy_id: Uuid = r.get("policy_id");
        let threshold_type: String = r.get("threshold_type");
        let amount_limit: i64 = r.get("amount_limit");
        let amount_observed: i64 = r.get("amount_observed");
        let observed_percent = if amount_limit > 0 {
            ((amount_observed as f64 / amount_limit as f64) * 100.0).round() as i64
        } else {
            0
        };
        // Paperclip filters soft incidents below 85% of the limit.
        if threshold_type != "hard" && observed_percent < 85 {
            continue;
        }
        let window_start: DateTime<Utc> = r.get("window_start");
        let created_at: DateTime<Utc> = r.get("created_at");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let scope_type: String = r.get("scope_type");
        let hard = threshold_type == "hard";
        items.push(create_item(
            company_id,
            "budget_alert",
            json!({
                "kind": "budget_incident",
                "id": id,
                "companyId": company_id,
                "title": format!("{scope_type} budget {}", if hard { "hard stop" } else { "warning" }),
                "identifier": Value::Null,
                "status": r.get::<String, _>("status"),
                "href": format!("/{company_id}/costs"),
                "metadata": {
                    "policyId": policy_id,
                    "scopeType": scope_type,
                    "scopeId": r.get::<Uuid, _>("scope_id"),
                    "thresholdType": threshold_type,
                    "amountObserved": amount_observed,
                    "amountLimit": amount_limit,
                    "observedPercent": observed_percent,
                    "approvalId": r.try_get::<Option<Uuid>, _>("approval_id").unwrap_or(None),
                    "approvalStatus": Value::Null,
                },
            }),
            if hard {
                "Budget hard stop was reached."
            } else {
                "Budget crossed the 85% warning threshold."
            },
            vec![
                verb(
                    "raise_budget_and_resume",
                    "Raise budget",
                    "Raise the budget and resume paused work.",
                ),
                verb(
                    "keep_paused",
                    "Keep paused",
                    "Dismiss or keep the budget stop in place.",
                ),
            ],
            true,
            "open budget incident is hard, or soft with observed spend >= 85% of limit.",
            "Budget incident is resolved or dismissed.",
            format!("budget:{policy_id}:{}:{threshold_type}", iso(window_start)),
            if hard { "high" } else { "medium" },
            updated_at,
            created_at,
            updated_at,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            json!({
                "kind": "budget",
                "observedPercent": observed_percent,
                "amountObserved": amount_observed,
                "amountLimit": amount_limit,
                "images": [],
            }),
        ));
    }

    Ok(items)
}

fn item_source_key(item: &Value) -> String {
    format!(
        "{}:{}",
        item.get("sourceKind")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        item.get("subject")
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
    )
}

fn item_subject_id(item: &Value) -> String {
    item.get("subject")
        .and_then(|s| s.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn item_metadata_agent_id(item: &Value) -> Option<Uuid> {
    let meta = item.get("subject")?.get("metadata")?;
    for key in [
        "originAgentId",
        "agentId",
        "createdByAgentId",
        "requestedByAgentId",
    ] {
        if let Some(v) = meta.get(key).and_then(|v| v.as_str()) {
            if let Ok(id) = Uuid::parse_str(v) {
                return Some(id);
            }
        }
    }
    None
}

/// Paperclip `enrichAttentionItems()` + `decisionRetentionService.syncItems()`.
async fn enrich_attention_items(
    pool: &PgPool,
    company_id: Uuid,
    items: &mut [Value],
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    if items.is_empty() {
        return Ok(());
    }
    let source_ids: Vec<String> = items
        .iter()
        .map(item_subject_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // -- queues -------------------------------------------------------------
    let queue_rows = sqlx::query(
        "SELECT qi.source_kind, qi.source_id, q.key, q.title, q.retention_days \
           FROM decision_queue_items qi \
           JOIN decision_queues q ON q.id = qi.queue_id AND q.company_id = $1 \
          WHERE qi.company_id = $1 AND qi.source_id = ANY($2) \
          ORDER BY q.title ASC, q.key ASC",
    )
    .bind(company_id)
    .bind(&source_ids)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut queues_by_source: HashMap<String, Vec<Value>> = HashMap::new();
    let mut retention_days_by_source: HashMap<String, Vec<i64>> = HashMap::new();
    for r in &queue_rows {
        let key = format!(
            "{}:{}",
            r.get::<String, _>("source_kind"),
            r.get::<String, _>("source_id")
        );
        queues_by_source
            .entry(key.clone())
            .or_default()
            .push(json!({
                "key": r.get::<String, _>("key"),
                "title": r.get::<String, _>("title"),
            }));
        if let Some(days) = r
            .try_get::<Option<i32>, _>("retention_days")
            .unwrap_or(None)
        {
            retention_days_by_source
                .entry(key)
                .or_default()
                .push(days as i64);
        }
    }

    // -- triage -------------------------------------------------------------
    let triage_rows = sqlx::query(
        "SELECT source_kind, source_id, decide_by, decide_by_date, snoozed_until, set_by_type, \
                set_by_agent_id, set_by_user_id, set_by_run_id, responsible_user_id, updated_at \
           FROM decision_triage \
          WHERE company_id = $1 AND source_id = ANY($2)",
    )
    .bind(company_id)
    .bind(&source_ids)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut triage_by_source: HashMap<String, &PgRow> = HashMap::new();
    for r in &triage_rows {
        triage_by_source.insert(
            format!(
                "{}:{}",
                r.get::<String, _>("source_kind"),
                r.get::<String, _>("source_id")
            ),
            r,
        );
    }

    // -- agent names --------------------------------------------------------
    let mut agent_ids: HashSet<Uuid> = items.iter().filter_map(item_metadata_agent_id).collect();
    for r in &triage_rows {
        if let Some(id) = r
            .try_get::<Option<Uuid>, _>("set_by_agent_id")
            .unwrap_or(None)
        {
            agent_ids.insert(id);
        }
    }
    let agent_id_list: Vec<Uuid> = agent_ids.into_iter().collect();
    let mut agent_names: HashMap<Uuid, String> = HashMap::new();
    if !agent_id_list.is_empty() {
        let rows =
            sqlx::query("SELECT id, name FROM agents WHERE company_id = $1 AND id = ANY($2)")
                .bind(company_id)
                .bind(&agent_id_list)
                .fetch_all(pool)
                .await
                .map_err(db_err)?;
        for r in &rows {
            agent_names.insert(r.get("id"), r.get("name"));
        }
    }

    // -- retention sync (upsert one row per source, bump version on activity) --
    for item in items.iter() {
        let source_kind = item
            .get("sourceKind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let source_id = item_subject_id(item);
        let activity_at = item
            .get("activityAt")
            .and_then(|v| v.as_str())
            .and_then(parse_iso)
            .unwrap_or(now);
        sqlx::query(
            "INSERT INTO decision_retention (company_id, source_kind, source_id, source_activity_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (company_id, source_kind, source_id) DO UPDATE \
                SET source_activity_at = EXCLUDED.source_activity_at, \
                    version = CASE WHEN decision_retention.source_activity_at <> EXCLUDED.source_activity_at \
                                   THEN decision_retention.version + 1 ELSE decision_retention.version END, \
                    updated_at = NOW() \
              WHERE decision_retention.source_activity_at IS DISTINCT FROM EXCLUDED.source_activity_at",
        )
        .bind(company_id)
        .bind(&source_kind)
        .bind(&source_id)
        .bind(activity_at)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }

    let retention_rows = sqlx::query(
        "SELECT source_kind, source_id, keep, archived_at, version \
           FROM decision_retention WHERE company_id = $1 AND source_id = ANY($2)",
    )
    .bind(company_id)
    .bind(&source_ids)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let mut retention_by_source: HashMap<String, &PgRow> = HashMap::new();
    for r in &retention_rows {
        retention_by_source.insert(
            format!(
                "{}:{}",
                r.get::<String, _>("source_kind"),
                r.get::<String, _>("source_id")
            ),
            r,
        );
    }

    // -- training example ids ----------------------------------------------
    let training_rows = sqlx::query(
        "SELECT id, source_kind, source_id::text AS source_id \
           FROM decision_training_examples WHERE company_id = $1",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let mut training_by_source: HashMap<String, Uuid> = HashMap::new();
    for r in &training_rows {
        // Paperclip maps attention source kinds onto training source kinds.
        let attention_kind = match r.get::<String, _>("source_kind").as_str() {
            "interaction" => "issue_thread_interaction",
            "approval" => "approval",
            other => {
                let _ = other;
                continue;
            }
        };
        training_by_source.insert(
            format!("{attention_kind}:{}", r.get::<String, _>("source_id")),
            r.get("id"),
        );
    }

    // -- apply --------------------------------------------------------------
    for item in items.iter_mut() {
        let key = item_source_key(item);
        let origin_agent_name = item_metadata_agent_id(item)
            .and_then(|id| agent_names.get(&id).cloned())
            .map(Value::String)
            .unwrap_or(Value::Null);

        let (decide_by, attribution_json, snoozed_until) = match triage_by_source.get(&key) {
            Some(r) => {
                let raw_decide_by: Option<String> = r.try_get("decide_by").unwrap_or(None);
                let decide_by_date: Option<NaiveDate> = r.try_get("decide_by_date").unwrap_or(None);
                let decide_by = match raw_decide_by.as_deref() {
                    Some("date") => decide_by_date.map(|d| d.format("%Y-%m-%d").to_string()),
                    other => other.map(str::to_string),
                };
                let set_by_agent_id: Option<Uuid> = r.try_get("set_by_agent_id").unwrap_or(None);
                let attribution = json!({
                    "type": r.get::<String, _>("set_by_type"),
                    "agentId": set_by_agent_id,
                    "agentName": set_by_agent_id.and_then(|id| agent_names.get(&id).cloned()),
                    "userId": r.try_get::<Option<String>, _>("set_by_user_id").unwrap_or(None),
                    "runId": r.try_get::<Option<Uuid>, _>("set_by_run_id").unwrap_or(None),
                    "responsibleUserId": r.try_get::<Option<String>, _>("responsible_user_id").unwrap_or(None),
                    "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
                });
                let snoozed = iso_opt(
                    r.try_get::<Option<DateTime<Utc>>, _>("snoozed_until")
                        .unwrap_or(None),
                );
                (
                    decide_by.map(Value::String).unwrap_or(Value::Null),
                    attribution,
                    snoozed,
                )
            }
            None => (Value::Null, Value::Null, Value::Null),
        };

        let retention_days = retention_days_by_source
            .get(&key)
            .and_then(|days| days.iter().min().copied())
            .unwrap_or(DEFAULT_DECISION_SHELF_DAYS);
        let activity_at = item
            .get("activityAt")
            .and_then(|v| v.as_str())
            .and_then(parse_iso)
            .unwrap_or(now);
        let shelf = activity_at <= now - Duration::days(retention_days);

        let (keep, archived_at, retention_version) = match retention_by_source.get(&key) {
            Some(r) => (
                r.try_get::<bool, _>("keep").unwrap_or(false),
                iso_opt(
                    r.try_get::<Option<DateTime<Utc>>, _>("archived_at")
                        .unwrap_or(None),
                ),
                r.try_get::<i32, _>("version").unwrap_or(0) as i64,
            ),
            None => (false, Value::Null, 0),
        };

        let obj = item.as_object_mut().expect("attention item is an object");
        obj.insert("originAgentName".into(), origin_agent_name);
        obj.insert(
            "queues".into(),
            Value::Array(queues_by_source.get(&key).cloned().unwrap_or_default()),
        );
        obj.insert("decideBy".into(), decide_by);
        obj.insert("decideByAttribution".into(), attribution_json);
        obj.insert("snoozedUntil".into(), snoozed_until);
        obj.insert("shelf".into(), Value::Bool(shelf));
        obj.insert("retentionDays".into(), json!(retention_days));
        obj.insert("keep".into(), Value::Bool(keep));
        obj.insert("archivedAt".into(), archived_at);
        obj.insert("retentionVersion".into(), json!(retention_version));
        obj.insert(
            "trainingExampleId".into(),
            training_by_source
                .get(&key)
                .map(|id| json!(id))
                .unwrap_or(Value::Null),
        );
    }

    Ok(())
}

fn start_of_utc_day(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|d| DateTime::<Utc>::from_naive_utc_and_offset(d, Utc))
        .unwrap_or(now)
}

fn end_of_utc_day(now: DateTime<Utc>) -> DateTime<Utc> {
    start_of_utc_day(now) + Duration::days(1) - Duration::milliseconds(1)
}

fn end_of_utc_week(now: DateTime<Utc>) -> DateTime<Utc> {
    let start = start_of_utc_day(now);
    // ISO-style Monday-Sunday week; Sunday is already the last day.
    let weekday = start.weekday().num_days_from_sunday();
    let days_until_sunday = if weekday == 0 { 0 } else { 7 - weekday } as i64;
    start + Duration::days(days_until_sunday + 1) - Duration::milliseconds(1)
}

/// Paperclip `decideOrder()` → `(bucket, deadlineMillis)`.
fn decide_order(item: &Value, now: DateTime<Utc>) -> (i32, i64) {
    let decide_by = item.get("decideBy").and_then(|v| v.as_str());
    match decide_by {
        Some("today") => (0, end_of_utc_day(now).timestamp_millis()),
        Some("this_week") => (0, end_of_utc_week(now).timestamp_millis()),
        Some("whenever") => (1, i64::MAX),
        Some(date) if NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok() => {
            let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
            let deadline = parsed
                .and_hms_milli_opt(23, 59, 59, 999)
                .map(|d| DateTime::<Utc>::from_naive_utc_and_offset(d, Utc).timestamp_millis())
                .unwrap_or(i64::MAX);
            (0, deadline)
        }
        _ => (2, i64::MAX),
    }
}

fn is_decide_now(item: &Value, now: DateTime<Utc>) -> bool {
    let (bucket, deadline) = decide_order(item, now);
    bucket == 0 && deadline <= end_of_utc_day(now).timestamp_millis()
}

fn is_new_today(item: &Value, now: DateTime<Utc>) -> bool {
    item.get("createdAt")
        .and_then(|v| v.as_str())
        .and_then(parse_iso)
        .is_some_and(|created| created >= start_of_utc_day(now))
}

fn item_millis(item: &Value, key: &str) -> i64 {
    item.get(key)
        .and_then(|v| v.as_str())
        .and_then(parse_iso)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// Paperclip `compareAttentionItems()`.
fn compare_attention_items(left: &Value, right: &Value) -> std::cmp::Ordering {
    let time_diff = item_millis(right, "activityAt").cmp(&item_millis(left, "activityAt"));
    if time_diff != std::cmp::Ordering::Equal {
        return time_diff;
    }
    let sev = severity_rank(
        left.get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("low"),
    )
    .cmp(&severity_rank(
        right
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("low"),
    ));
    if sev != std::cmp::Ordering::Equal {
        return sev;
    }
    let src = source_rank(
        left.get("sourceKind")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .cmp(&source_rank(
        right
            .get("sourceKind")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    ));
    if src != std::cmp::Ordering::Equal {
        return src;
    }
    left.get("dedupKey")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .cmp(right.get("dedupKey").and_then(|v| v.as_str()).unwrap_or(""))
}

/// Paperclip `compareDecideItems()`.
fn compare_decide_items(left: &Value, right: &Value, now: DateTime<Utc>) -> std::cmp::Ordering {
    let (lb, ld) = decide_order(left, now);
    let (rb, rd) = decide_order(right, now);
    if lb != rb {
        return lb.cmp(&rb);
    }
    if ld != rd {
        return ld.cmp(&rd);
    }
    let le = left
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .and_then(parse_iso)
        .map(|d| d.timestamp_millis())
        .unwrap_or(i64::MAX);
    let re = right
        .get("expiresAt")
        .and_then(|v| v.as_str())
        .and_then(parse_iso)
        .map(|d| d.timestamp_millis())
        .unwrap_or(i64::MAX);
    if le != re {
        return le.cmp(&re);
    }
    let sev = severity_rank(
        left.get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("low"),
    )
    .cmp(&severity_rank(
        right
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("low"),
    ));
    if sev != std::cmp::Ordering::Equal {
        return sev;
    }
    compare_attention_items(left, right)
}

fn encode_cursor(sort: &str, item: &Value) -> String {
    let payload = json!({
        "v": 1,
        "sort": sort,
        "id": item.get("id").and_then(|v| v.as_str()).unwrap_or(""),
    })
    .to_string();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload)
}

fn decode_cursor(cursor: &str, sort: &str) -> Result<String, AppError> {
    let bad = || AppError::BadRequest("Invalid attention cursor".to_string());
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| bad())?;
    let decoded: Value = serde_json::from_slice(&raw).map_err(|_| bad())?;
    if decoded.get("v").and_then(|v| v.as_i64()) != Some(1)
        || decoded.get("sort").and_then(|v| v.as_str()) != Some(sort)
    {
        return Err(bad());
    }
    decoded
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(bad)
}

#[derive(Debug, Default, Deserialize)]
pub struct AttentionQuery {
    #[serde(default, rename = "includeDismissed")]
    pub include_dismissed: Option<String>,
    pub archived: Option<String>,
    pub all: Option<String>,
    #[serde(rename = "activitySince")]
    pub activity_since: Option<String>,
    #[serde(rename = "activityUntil")]
    pub activity_until: Option<String>,
    pub queue: Option<String>,
    pub sort: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

fn truthy(value: &Option<String>) -> bool {
    matches!(value.as_deref(), Some("true") | Some("1") | Some(""))
}

/// GET /companies/:company_id/attention
async fn get_attention_feed(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<AttentionQuery>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    require_board(&actor)?;

    let pool = &state.pool;
    let now = Utc::now();
    let sort = match q.sort.as_deref() {
        Some("decide") => "decide",
        Some("activity") | None => "activity",
        Some(other) => {
            return Err(AppError::BadRequest(format!(
                "sort must be 'activity' or 'decide' (received '{other}')"
            )))
        }
    };

    let activity_since = match q.activity_since.as_deref() {
        Some(v) => Some(parse_iso(v).ok_or_else(|| {
            AppError::BadRequest("activitySince must be an ISO timestamp".to_string())
        })?),
        None => None,
    };
    let activity_until = match q.activity_until.as_deref() {
        Some(v) => Some(parse_iso(v).ok_or_else(|| {
            AppError::BadRequest("activityUntil must be an ISO timestamp".to_string())
        })?),
        None => None,
    };

    let mut items = collect_attention_items(pool, company_id).await?;
    enrich_attention_items(pool, company_id, &mut items, now).await?;

    let want_archived = truthy(&q.archived);
    let include_dismissed = truthy(&q.include_dismissed);
    let return_all = truthy(&q.all);

    items.retain(|item| {
        let archived = !matches!(item.get("archivedAt"), Some(Value::Null) | None);
        if archived != want_archived {
            return false;
        }
        if !include_dismissed {
            // Snoozed rows are hidden until the snooze elapses.
            if let Some(snoozed) = item
                .get("snoozedUntil")
                .and_then(|v| v.as_str())
                .and_then(parse_iso)
            {
                if snoozed > now {
                    return false;
                }
            }
        }
        if let Some(queue) = q.queue.as_deref() {
            let matched = item
                .get("queues")
                .and_then(|v| v.as_array())
                .is_some_and(|queues| {
                    queues
                        .iter()
                        .any(|entry| entry.get("key").and_then(|k| k.as_str()) == Some(queue))
                });
            if !matched {
                return false;
            }
        }
        let activity_at = item
            .get("activityAt")
            .and_then(|v| v.as_str())
            .and_then(parse_iso);
        if let (Some(since), Some(at)) = (activity_since, activity_at) {
            if at < since {
                return false;
            }
        }
        if let (Some(until), Some(at)) = (activity_until, activity_at) {
            if at > until {
                return false;
            }
        }
        true
    });

    if sort == "decide" {
        items.sort_by(|a, b| compare_decide_items(a, b, now));
    } else {
        items.sort_by(compare_attention_items);
    }
    for (index, item) in items.iter_mut().enumerate() {
        if let Some(obj) = item.as_object_mut() {
            obj.insert("rank".into(), json!(index));
        }
    }

    let total_count = items.len();
    let desk_badge_count = items
        .iter()
        .filter(|item| is_new_today(item, now) || is_decide_now(item, now))
        .count();

    let mut counts: BTreeMap<&str, usize> =
        ATTENTION_SOURCE_KINDS.iter().map(|k| (*k, 0)).collect();
    for item in &items {
        if let Some(kind) = item.get("sourceKind").and_then(|v| v.as_str()) {
            if let Some(slot) = counts.get_mut(kind) {
                *slot += 1;
            }
        }
    }

    let (page, next_cursor) = if return_all {
        (items.clone(), Value::Null)
    } else {
        let limit = q
            .limit
            .unwrap_or(ATTENTION_PAGE_DEFAULT_LIMIT)
            .clamp(1, ATTENTION_PAGE_MAX_LIMIT);
        let mut start = 0usize;
        if let Some(cursor) = q.cursor.as_deref() {
            let cursor_id = decode_cursor(cursor, sort)?;
            match items.iter().position(|item| {
                item.get("id").and_then(|v| v.as_str()) == Some(cursor_id.as_str())
            }) {
                Some(index) => start = index + 1,
                None => start = items.len(),
            }
        }
        let page: Vec<Value> = items.iter().skip(start).take(limit).cloned().collect();
        let has_next = start + page.len() < items.len();
        let cursor = match (has_next, page.last()) {
            (true, Some(last)) => Value::String(encode_cursor(sort, last)),
            _ => Value::Null,
        };
        (page, cursor)
    };

    Ok(Json(json!({
        "companyId": company_id,
        "generatedAt": iso(now),
        "totalCount": total_count,
        "deskBadgeCount": desk_badge_count,
        "nextCursor": next_cursor,
        "countsBySourceKind": counts
            .into_iter()
            .map(|(kind, count)| (kind.to_string(), json!(count)))
            .collect::<Map<String, Value>>(),
        "items": page,
    })))
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

fn decision_to_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "bundleId": r.try_get::<Option<Uuid>, _>("bundle_id").unwrap_or(None),
        "originAgentId": r.get::<Uuid, _>("origin_agent_id"),
        "originIssueId": r.get::<Uuid, _>("origin_issue_id"),
        "originRunId": r.get::<Uuid, _>("origin_run_id"),
        "ruleKey": r.try_get::<Option<String>, _>("rule_key").unwrap_or(None),
        "title": r.get::<String, _>("title"),
        "body": r.get::<String, _>("body"),
        "options": r.try_get::<Value, _>("options").unwrap_or(Value::Null),
        "inputs": r.try_get::<Option<Value>, _>("inputs").unwrap_or(None),
        "status": r.get::<String, _>("status"),
        "executionStatus": r.try_get::<Option<String>, _>("execution_status").unwrap_or(None),
        "chosenOptionId": r.try_get::<Option<String>, _>("chosen_option_id").unwrap_or(None),
        "inputValues": r.try_get::<Option<Value>, _>("input_values").unwrap_or(None),
        "decidedByUserId": r.try_get::<Option<String>, _>("decided_by_user_id").unwrap_or(None),
        "decidedAt": iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("decided_at").unwrap_or(None)),
        "expiresAt": iso(r.get::<DateTime<Utc>, _>("expires_at")),
        "idempotencyKey": r.try_get::<Option<String>, _>("idempotency_key").unwrap_or(None),
        "signedSpec": r.get::<String, _>("signed_spec"),
        "targetSnapshots": r.try_get::<Value, _>("target_snapshots").unwrap_or(Value::Null),
        "continuationPolicy": r.get::<String, _>("continuation_policy"),
        "metadata": r.try_get::<Value, _>("metadata").unwrap_or(Value::Null),
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
    })
}

const DECISION_SELECT: &str =
    "SELECT id, company_id, bundle_id, origin_agent_id, origin_issue_id, \
     origin_run_id, rule_key, title, body, options, inputs, status, execution_status, \
     chosen_option_id, input_values, decided_by_user_id, decided_at, expires_at, idempotency_key, \
     signed_spec, target_snapshots, continuation_policy, metadata, created_at, updated_at \
     FROM decisions";

fn effect_execution_to_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "decisionId": r.get::<Uuid, _>("decision_id"),
        "effectIndex": r.get::<i32, _>("effect_index"),
        "effectType": r.get::<String, _>("effect_type"),
        "targetIssueId": r.try_get::<Option<Uuid>, _>("target_issue_id").unwrap_or(None),
        "status": r.get::<String, _>("status"),
        "result": r.try_get::<Option<Value>, _>("result").unwrap_or(None),
        "error": r.try_get::<Option<String>, _>("error").unwrap_or(None),
        "activityLogId": r.try_get::<Option<Uuid>, _>("activity_log_id").unwrap_or(None),
        "executedAt": iso(r.get::<DateTime<Utc>, _>("executed_at")),
    })
}

/// Paperclip `svc.outcome(decisionId)` — decision + ordered effect executions.
async fn decision_outcome(pool: &PgPool, decision_row: &PgRow) -> Result<Value, AppError> {
    let decision_id: Uuid = decision_row.get("id");
    let rows = sqlx::query(
        "SELECT id, decision_id, effect_index, effect_type, target_issue_id, status, result, \
                error, activity_log_id, executed_at \
           FROM decision_effect_executions WHERE decision_id = $1 ORDER BY effect_index ASC",
    )
    .bind(decision_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(json!({
        "decision": decision_to_json(decision_row),
        "executions": rows.iter().map(effect_execution_to_json).collect::<Vec<_>>(),
    }))
}

async fn load_decision(pool: &PgPool, decision_id: Uuid) -> Result<PgRow, AppError> {
    sqlx::query(&format!("{DECISION_SELECT} WHERE id = $1"))
        .bind(decision_id)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::NotFound("Decision not found".to_string()))
}

/// `packages/shared/src/validators/decision.ts` → decisionOptionsSchema
fn validate_options(options: &Value) -> Result<(), AppError> {
    let arr = options
        .as_array()
        .ok_or_else(|| AppError::Validation("options must be an array".to_string()))?;
    if arr.is_empty() || arr.len() > 8 {
        return Err(AppError::Validation(
            "options must contain between 1 and 8 entries".to_string(),
        ));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for option in arr {
        let id = option
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() <= 120)
            .ok_or_else(|| {
                AppError::Validation("each option needs an id of 1-120 characters".to_string())
            })?;
        option
            .get("label")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && s.len() <= 240)
            .ok_or_else(|| {
                AppError::Validation("each option needs a label of 1-240 characters".to_string())
            })?;
        if let Some(effects) = option.get("effects") {
            let effects = effects.as_array().ok_or_else(|| {
                AppError::Validation("option.effects must be an array".to_string())
            })?;
            if effects.len() > 10 {
                return Err(AppError::Validation(
                    "option.effects may not exceed 10 entries".to_string(),
                ));
            }
        }
        if !seen.insert(id) {
            return Err(AppError::Validation(
                "option ids must be unique".to_string(),
            ));
        }
    }
    Ok(())
}

/// `packages/shared/src/validators/decision.ts` → decisionInputsSchema
fn validate_inputs(inputs: &Value) -> Result<(), AppError> {
    let arr = inputs
        .as_array()
        .ok_or_else(|| AppError::Validation("inputs must be an array".to_string()))?;
    if arr.len() > 4 {
        return Err(AppError::Validation(
            "inputs may not exceed 4 entries".to_string(),
        ));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for input in arr {
        let id = input
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Validation("each input needs an id".to_string()))?;
        input
            .get("label")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::Validation("each input needs a label".to_string()))?;
        if !seen.insert(id) {
            return Err(AppError::Validation("input ids must be unique".to_string()));
        }
    }
    Ok(())
}

/// Deterministic spec digest. Paperclip signs the spec with an Ed25519 key
/// (`signDecisionSpec`); Parrot stores a SHA-256 digest of the same canonical
/// fields so tampering is still detectable at decide-time.
fn sign_decision_spec(
    company_id: Uuid,
    origin_agent_id: Uuid,
    origin_issue_id: Uuid,
    title: &str,
    options: &Value,
    inputs: &Value,
    expires_at: DateTime<Utc>,
) -> String {
    let canonical = format!(
        "{company_id}|{origin_agent_id}|{origin_issue_id}|{title}|{}|{}|{}",
        options,
        inputs,
        iso(expires_at)
    );
    let digest = Sha256::digest(canonical.as_bytes());
    format!("sha256:{}", hex::encode(digest))
}

#[derive(Debug, Deserialize)]
pub struct CreateDecisionInput {
    pub title: String,
    pub body: String,
    #[serde(rename = "ruleKey")]
    pub rule_key: Option<String>,
    pub options: Value,
    pub inputs: Option<Value>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: Option<String>,
    #[serde(rename = "continuationPolicy")]
    pub continuation_policy: Option<String>,
    pub metadata: Option<Value>,
    #[serde(rename = "bundleId")]
    pub bundle_id: Option<Uuid>,
    #[serde(rename = "issueId")]
    pub issue_id: Option<Uuid>,
}

struct DecisionOrigin {
    agent_id: Uuid,
    issue_id: Uuid,
    run_id: Uuid,
}

/// Resolve `(originAgentId, originIssueId, originRunId)` from the agent run context.
async fn resolve_decision_origin(
    pool: &PgPool,
    company_id: Uuid,
    actor: &AuthorizationActor,
    explicit_issue_id: Option<Uuid>,
) -> Result<DecisionOrigin, AppError> {
    let (agent_id, run_id) = agent_context(actor)?;
    let run_id = run_id.ok_or_else(|| forbid("Agent run context required"))?;

    let run = sqlx::query(
        "SELECT context_snapshot FROM heartbeat_runs WHERE id = $1 AND company_id = $2",
    )
    .bind(run_id)
    .bind(company_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::NotFound("Origin run not found".to_string()))?;

    let issue_id = match explicit_issue_id {
        Some(id) => id,
        None => run
            .try_get::<Option<Value>, _>("context_snapshot")
            .unwrap_or(None)
            .as_ref()
            .and_then(|s| s.get("issueId"))
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(|| {
                AppError::Validation(
                    "issueId is required when the origin run has no issue context".to_string(),
                )
            })?,
    };

    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM issues WHERE id = $1 AND company_id = $2")
            .bind(issue_id)
            .bind(company_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
    if exists.is_none() {
        return Err(AppError::NotFound("Origin issue not found".to_string()));
    }

    Ok(DecisionOrigin {
        agent_id,
        issue_id,
        run_id,
    })
}

/// Snapshot the decision's target issues (Paperclip `snapshots()`).
async fn build_target_snapshots(
    pool: &PgPool,
    company_id: Uuid,
    issue_ids: &[Uuid],
) -> Result<Value, AppError> {
    let mut map = Map::new();
    if issue_ids.is_empty() {
        return Ok(Value::Object(map));
    }
    let rows = sqlx::query(
        "SELECT id, status::text AS status, assignee_agent_id, assignee_user_id, updated_at \
           FROM issues WHERE company_id = $1 AND id = ANY($2)",
    )
    .bind(company_id)
    .bind(issue_ids)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    for r in &rows {
        map.insert(
            r.get::<Uuid, _>("id").to_string(),
            json!({
                "status": r.try_get::<Option<String>, _>("status").unwrap_or(None),
                "assigneeAgentId": r.try_get::<Option<Uuid>, _>("assignee_agent_id").unwrap_or(None),
                "assigneeUserId": r.try_get::<Option<Uuid>, _>("assignee_user_id").unwrap_or(None),
                "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
            }),
        );
    }
    Ok(Value::Object(map))
}

/// Collect the issue ids referenced by option effects (`effect.issueId` / `targetIssueId`).
fn option_target_issue_ids(options: &Value) -> Vec<Uuid> {
    let mut ids = Vec::new();
    let Some(arr) = options.as_array() else {
        return ids;
    };
    for option in arr {
        let Some(effects) = option.get("effects").and_then(|v| v.as_array()) else {
            continue;
        };
        for effect in effects {
            for key in ["issueId", "targetIssueId"] {
                if let Some(id) = effect
                    .get(key)
                    .and_then(|v| v.as_str())
                    .and_then(|v| Uuid::parse_str(v).ok())
                {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids
}

/// Shared insert used by both `POST /decisions` and the archive-proposal endpoint.
#[allow(clippy::too_many_arguments)]
async fn insert_decision(
    pool: &PgPool,
    company_id: Uuid,
    origin: &DecisionOrigin,
    input: CreateDecisionInput,
) -> Result<PgRow, AppError> {
    let title = input.title.trim().to_string();
    if title.is_empty() || title.len() > 240 {
        return Err(AppError::Validation(
            "title must be 1-240 characters".to_string(),
        ));
    }
    if input.body.trim().is_empty() {
        return Err(AppError::Validation("body is required".to_string()));
    }
    validate_options(&input.options)?;
    let inputs = input.inputs.unwrap_or_else(|| json!([]));
    validate_inputs(&inputs)?;

    let continuation_policy = input
        .continuation_policy
        .unwrap_or_else(|| "none".to_string());
    if !matches!(continuation_policy.as_str(), "none" | "wake_origin_agent") {
        return Err(AppError::Validation(
            "continuationPolicy must be 'none' or 'wake_origin_agent'".to_string(),
        ));
    }

    let now = Utc::now();
    let expires_at = match input.expires_at.as_deref() {
        Some(value) => parse_iso(value).ok_or_else(|| {
            AppError::Validation("expiresAt must be an ISO timestamp".to_string())
        })?,
        None => now + Duration::days(7),
    };
    if expires_at < now + Duration::days(1) || expires_at > now + Duration::days(30) {
        return Err(AppError::Validation(
            "expiresAt must be between 1 and 30 days from now".to_string(),
        ));
    }

    // Idempotency replay: the unique index is (company_id, idempotency_key).
    if let Some(key) = input.idempotency_key.as_deref() {
        let existing = sqlx::query(&format!(
            "{DECISION_SELECT} WHERE company_id = $1 AND idempotency_key = $2"
        ))
        .bind(company_id)
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?;
        if let Some(row) = existing {
            return Ok(row);
        }
    }

    let open_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM decisions WHERE company_id = $1 AND status = 'open'",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    if open_count >= OPEN_DECISION_CAP {
        return Err(AppError::Conflict(format!(
            "Company already has {open_count} open decisions (cap {OPEN_DECISION_CAP})"
        )));
    }

    let mut target_issue_ids = option_target_issue_ids(&input.options);
    if !target_issue_ids.contains(&origin.issue_id) {
        target_issue_ids.push(origin.issue_id);
    }
    let target_snapshots = build_target_snapshots(pool, company_id, &target_issue_ids).await?;
    let signed_spec = sign_decision_spec(
        company_id,
        origin.agent_id,
        origin.issue_id,
        &title,
        &input.options,
        &inputs,
        expires_at,
    );

    let decision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO decisions (company_id, bundle_id, origin_agent_id, origin_issue_id, \
             origin_run_id, rule_key, title, body, options, inputs, expires_at, idempotency_key, \
             signed_spec, target_snapshots, continuation_policy, metadata) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         RETURNING id",
    )
    .bind(company_id)
    .bind(input.bundle_id)
    .bind(origin.agent_id)
    .bind(origin.issue_id)
    .bind(origin.run_id)
    .bind(&input.rule_key)
    .bind(&title)
    .bind(&input.body)
    .bind(&input.options)
    .bind(&inputs)
    .bind(expires_at)
    .bind(&input.idempotency_key)
    .bind(&signed_spec)
    .bind(&target_snapshots)
    .bind(&continuation_policy)
    .bind(input.metadata.unwrap_or_else(|| json!({})))
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    for issue_id in &target_issue_ids {
        sqlx::query(
            "INSERT INTO decision_target_issues (decision_id, issue_id, company_id) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(decision_id)
        .bind(issue_id)
        .bind(company_id)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }

    load_decision(pool, decision_id).await
}

/// POST /companies/:company_id/decisions
async fn create_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateDecisionInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    let pool = &state.pool;
    let origin = resolve_decision_origin(pool, company_id, &actor, input.issue_id).await?;
    let row = insert_decision(pool, company_id, &origin, input).await?;
    let decision = decision_to_json(&row);
    log_activity(
        pool,
        company_id,
        &actor,
        "decision.created",
        "decision",
        row.get::<Uuid, _>("id"),
        json!({ "title": decision.get("title") }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(decision)))
}

#[derive(Debug, Default, Deserialize)]
pub struct DecisionListQuery {
    pub status: Option<String>,
    #[serde(rename = "bundleId")]
    pub bundle_id: Option<Uuid>,
    #[serde(rename = "targetIssueId")]
    pub target_issue_id: Option<Uuid>,
    #[serde(rename = "originAgentId")]
    pub origin_agent_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// GET /companies/:company_id/decisions
async fn list_decisions(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<DecisionListQuery>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    require_board(&actor)?;
    let pool = &state.pool;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query(
        "SELECT d.id, d.company_id, d.bundle_id, d.origin_agent_id, d.origin_issue_id, \
                d.origin_run_id, d.rule_key, d.title, d.body, d.options, d.inputs, d.status, \
                d.execution_status, d.chosen_option_id, d.input_values, d.decided_by_user_id, \
                d.decided_at, d.expires_at, d.idempotency_key, d.signed_spec, d.target_snapshots, \
                d.continuation_policy, d.metadata, d.created_at, d.updated_at \
           FROM decisions d \
          WHERE d.company_id = $1 \
            AND ($2::text IS NULL OR d.status = $2) \
            AND ($3::uuid IS NULL OR d.bundle_id = $3) \
            AND ($4::uuid IS NULL OR d.origin_agent_id = $4) \
            AND ($5::uuid IS NULL OR EXISTS ( \
                  SELECT 1 FROM decision_target_issues t \
                   WHERE t.decision_id = d.id AND t.issue_id = $5)) \
          ORDER BY d.created_at DESC LIMIT $6",
    )
    .bind(company_id)
    .bind(&q.status)
    .bind(q.bundle_id)
    .bind(q.origin_agent_id)
    .bind(q.target_issue_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    // `targetChanged`: has any snapshotted target issue drifted since capture?
    let decision_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let mut current_issue_state: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
    if !decision_ids.is_empty() {
        let issue_rows = sqlx::query(
            "SELECT DISTINCT i.id, i.updated_at \
               FROM decision_target_issues t \
               JOIN issues i ON i.id = t.issue_id \
              WHERE t.decision_id = ANY($1)",
        )
        .bind(&decision_ids)
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
        for r in &issue_rows {
            current_issue_state.insert(r.get("id"), r.get("updated_at"));
        }
    }

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let mut value = decision_to_json(r);
        let snapshots: Value = r.try_get("target_snapshots").unwrap_or(Value::Null);
        let mut target_changed = Map::new();
        if let Some(map) = snapshots.as_object() {
            for (issue_id, snapshot) in map {
                let changed = Uuid::parse_str(issue_id)
                    .ok()
                    .and_then(|id| current_issue_state.get(&id).copied())
                    .map(|updated| {
                        snapshot
                            .get("updatedAt")
                            .and_then(|v| v.as_str())
                            .and_then(parse_iso)
                            != Some(updated)
                    })
                    .unwrap_or(false);
                target_changed.insert(issue_id.clone(), Value::Bool(changed));
            }
        }
        if let Some(obj) = value.as_object_mut() {
            obj.insert("targetChanged".into(), Value::Object(target_changed));
        }

        // Terminal decisions carry their effect executions inline (Paperclip parity).
        if r.get::<String, _>("status") != "open" {
            let executions = sqlx::query(
                "SELECT id, decision_id, effect_index, effect_type, target_issue_id, status, \
                        result, error, activity_log_id, executed_at \
                   FROM decision_effect_executions WHERE decision_id = $1 ORDER BY effect_index ASC",
            )
            .bind(r.get::<Uuid, _>("id"))
            .fetch_all(pool)
            .await
            .map_err(db_err)?;
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "executions".into(),
                    Value::Array(executions.iter().map(effect_execution_to_json).collect()),
                );
            }
        }
        out.push(value);
    }

    Ok(Json(Value::Array(out)))
}

#[derive(Debug, Default, Deserialize)]
pub struct DecisionStatsQuery {
    #[serde(rename = "groupBy")]
    pub group_by: Option<String>,
    #[serde(rename = "originAgentId")]
    pub origin_agent_id: Option<Uuid>,
    pub since: Option<String>,
}

/// GET /companies/:company_id/decisions/stats
async fn decision_stats(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(q): Query<DecisionStatsQuery>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    crate::routes::assert_board_or_agent(&actor)
        .map_err(|_| forbid("Board or agent session required"))?;
    if let Some(group_by) = q.group_by.as_deref() {
        if group_by != "ruleKey" {
            return Err(AppError::BadRequest(
                "groupBy only supports 'ruleKey'".to_string(),
            ));
        }
    }
    let since =
        match q.since.as_deref() {
            Some(value) => Some(parse_iso(value).ok_or_else(|| {
                AppError::BadRequest("since must be an ISO timestamp".to_string())
            })?),
            None => None,
        };
    let pool = &state.pool;

    let rows = sqlx::query(
        "SELECT rule_key, status, chosen_option_id, metadata FROM decisions \
          WHERE company_id = $1 \
            AND ($2::uuid IS NULL OR origin_agent_id = $2) \
            AND ($3::timestamptz IS NULL OR created_at >= $3)",
    )
    .bind(company_id)
    .bind(q.origin_agent_id)
    .bind(since)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    #[derive(Default)]
    struct Bucket {
        proposed: i64,
        accepted: i64,
        rejected: i64,
        expired: i64,
        chosen: BTreeMap<String, i64>,
    }

    let mut totals = Bucket::default();
    let mut groups: BTreeMap<String, Bucket> = BTreeMap::new();

    for r in &rows {
        let rule_key: Option<String> = r.try_get("rule_key").unwrap_or(None);
        let status: String = r.get("status");
        let chosen: Option<String> = r.try_get("chosen_option_id").unwrap_or(None);
        let metadata: Value = r.try_get("metadata").unwrap_or(Value::Null);
        let dismissed_flag = metadata
            .get("dismissed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let rejected =
            chosen.as_deref() == Some("dismissed") || (status == "decided" && dismissed_flag);
        let accepted = status == "decided" && !rejected;
        let expired = status == "expired";

        let group = groups
            .entry(rule_key.unwrap_or_else(|| "unknown".to_string()))
            .or_default();
        for bucket in [&mut totals, group] {
            bucket.proposed += 1;
            if accepted {
                bucket.accepted += 1;
            }
            if rejected {
                bucket.rejected += 1;
            }
            if expired {
                bucket.expired += 1;
            }
        }
        if accepted {
            if let Some(option_id) = chosen.as_deref() {
                *totals.chosen.entry(option_id.to_string()).or_insert(0) += 1;
                let group = groups.get_mut(
                    &r.try_get::<Option<String>, _>("rule_key")
                        .unwrap_or(None)
                        .unwrap_or_else(|| "unknown".to_string()),
                );
                if let Some(group) = group {
                    *group.chosen.entry(option_id.to_string()).or_insert(0) += 1;
                }
            }
        }
    }

    let group_values: Vec<Value> = groups
        .iter()
        .map(|(rule_key, bucket)| {
            json!({
                "ruleKey": rule_key,
                "proposed": bucket.proposed,
                "accepted": bucket.accepted,
                "rejected": bucket.rejected,
                "expired": bucket.expired,
                "chosenOptions": bucket.chosen.iter().map(|(option_id, count)| json!({
                    "optionId": option_id,
                    "count": count,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    Ok(Json(json!({
        "totals": {
            "proposed": totals.proposed,
            "accepted": totals.accepted,
            "rejected": totals.rejected,
            "expired": totals.expired,
        },
        "groups": group_values,
    })))
}

#[derive(Debug, Deserialize)]
pub struct CreateDecisionBundleInput {
    pub title: String,
    pub summary: Option<String>,
    #[serde(rename = "issueId")]
    pub issue_id: Option<Uuid>,
}

/// POST /companies/:company_id/decision-bundles
async fn create_decision_bundle(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateDecisionBundleInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    let pool = &state.pool;
    let origin = resolve_decision_origin(pool, company_id, &actor, input.issue_id).await?;

    let title = input.title.trim().to_string();
    if title.is_empty() || title.len() > 240 {
        return Err(AppError::Validation(
            "title must be 1-240 characters".to_string(),
        ));
    }

    let row = sqlx::query(
        "INSERT INTO decision_bundles (company_id, title, summary, origin_agent_id, \
             origin_issue_id, origin_run_id) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, company_id, title, summary, origin_agent_id, origin_issue_id, \
                   origin_run_id, created_at",
    )
    .bind(company_id)
    .bind(&title)
    .bind(&input.summary)
    .bind(origin.agent_id)
    .bind(origin.issue_id)
    .bind(origin.run_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.get::<Uuid, _>("id"),
            "companyId": row.get::<Uuid, _>("company_id"),
            "title": row.get::<String, _>("title"),
            "summary": row.try_get::<Option<String>, _>("summary").unwrap_or(None),
            "originAgentId": row.get::<Uuid, _>("origin_agent_id"),
            "originIssueId": row.get::<Uuid, _>("origin_issue_id"),
            "originRunId": row.get::<Uuid, _>("origin_run_id"),
            "createdAt": iso(row.get::<DateTime<Utc>, _>("created_at")),
        })),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ArchiveProposalItem {
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateDecisionArchiveProposalInput {
    pub items: Vec<ArchiveProposalItem>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: Option<String>,
}

/// POST /companies/:company_id/decision-archive-proposals
async fn create_decision_archive_proposal(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateDecisionArchiveProposalInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    let pool = &state.pool;
    let origin = resolve_decision_origin(pool, company_id, &actor, None).await?;

    if input.items.is_empty() {
        return Err(AppError::Validation(
            "items must contain at least one entry".to_string(),
        ));
    }
    for item in &input.items {
        if !is_valid_source_kind(&item.source_kind) {
            return Err(AppError::Validation(format!(
                "unknown sourceKind '{}'",
                item.source_kind
            )));
        }
    }

    let now = Utc::now();
    let mut snapshot = collect_attention_items(pool, company_id).await?;
    enrich_attention_items(pool, company_id, &mut snapshot, now).await?;
    let by_key: HashMap<String, &Value> = snapshot
        .iter()
        .map(|item| (item_source_key(item), item))
        .collect();

    let mut requested: Vec<&ArchiveProposalItem> = input.items.iter().collect();
    requested.sort_by(|a, b| {
        format!("{}:{}", a.source_kind, a.source_id)
            .cmp(&format!("{}:{}", b.source_kind, b.source_id))
    });

    let mut manifest: Vec<Value> = Vec::with_capacity(requested.len());
    for item in requested {
        let key = format!("{}:{}", item.source_kind, item.source_id);
        let attention = by_key.get(&key).ok_or_else(|| {
            AppError::Validation(
                "Every archive proposal item must be on the current aging shelf".to_string(),
            )
        })?;
        let on_shelf = attention
            .get("shelf")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let archived = !matches!(attention.get("archivedAt"), Some(Value::Null) | None);
        if !on_shelf || archived {
            return Err(AppError::Validation(
                "Every archive proposal item must be on the current aging shelf".to_string(),
            ));
        }
        manifest.push(json!({
            "companyId": company_id,
            "sourceKind": item.source_kind,
            "sourceId": item.source_id,
            "expectedVersion": attention.get("retentionVersion").cloned().unwrap_or(json!(0)),
            "activityAt": attention.get("activityAt").cloned().unwrap_or(Value::Null),
            "reason": item.reason,
        }));
    }

    let manifest_hash = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(
            Value::Array(manifest.clone()).to_string().as_bytes()
        ))
    );
    let body = manifest
        .iter()
        .map(|entry| {
            format!(
                "- **{}:{}** — {}",
                entry
                    .get("sourceKind")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                entry.get("sourceId").and_then(|v| v.as_str()).unwrap_or(""),
                entry.get("reason").and_then(|v| v.as_str()).unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let plural = if manifest.len() == 1 { "" } else { "s" };
    let create_input = CreateDecisionInput {
        title: format!("Archive {} aging decision{plural}?", manifest.len()),
        body,
        rule_key: Some("attention.bulk_archive".to_string()),
        options: json!([
            { "id": "archive", "label": "Archive reviewed items", "style": "destructive", "effects": [] },
            { "id": "keep", "label": "Keep items", "effects": [] }
        ]),
        inputs: Some(json!([])),
        expires_at: None,
        idempotency_key: Some(
            input
                .idempotency_key
                .unwrap_or_else(|| format!("attention-archive:{manifest_hash}:{}", origin.run_id)),
        ),
        continuation_policy: Some("wake_origin_agent".to_string()),
        metadata: Some(json!({
            "attentionArchive": { "manifest": manifest, "manifestHash": manifest_hash },
        })),
        bundle_id: None,
        issue_id: Some(origin.issue_id),
    };

    let row = insert_decision(pool, company_id, &origin, create_input).await?;
    Ok((StatusCode::CREATED, Json(decision_to_json(&row))))
}

/// GET /decisions/:decision_id
async fn get_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(decision_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let pool = &state.pool;
    let row = load_decision(pool, decision_id).await?;
    require_company_access(&actor, row.get::<Uuid, _>("company_id"), true)?;
    Ok(Json(decision_outcome(pool, &row).await?))
}

#[derive(Debug, Deserialize)]
pub struct DecideDecisionInput {
    #[serde(rename = "optionId")]
    pub option_id: String,
    #[serde(rename = "inputValues")]
    pub input_values: Option<Value>,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: Option<String>,
}

/// Claim the chosen option's effects so an executor can drive them.
async fn claim_effects(
    pool: &PgPool,
    company_id: Uuid,
    decision_id: Uuid,
    option: &Value,
) -> Result<(), AppError> {
    let Some(effects) = option.get("effects").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    for (index, effect) in effects.iter().enumerate() {
        let effect_type = effect
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let target_issue_id = effect
            .get("issueId")
            .or_else(|| effect.get("targetIssueId"))
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok());
        sqlx::query(
            "INSERT INTO decision_effect_executions \
                 (decision_id, company_id, effect_index, effect_type, target_issue_id, status) \
             VALUES ($1, $2, $3, $4, $5, 'claimed') \
             ON CONFLICT (decision_id, effect_index) DO NOTHING",
        )
        .bind(decision_id)
        .bind(company_id)
        .bind(index as i32)
        .bind(effect_type)
        .bind(target_issue_id)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }
    Ok(())
}

fn find_option<'a>(options: &'a Value, option_id: &str) -> Option<&'a Value> {
    options
        .as_array()?
        .iter()
        .find(|option| option.get("id").and_then(|v| v.as_str()) == Some(option_id))
}

/// POST /decisions/:decision_id/decide
async fn decide_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(decision_id): Path<Uuid>,
    Json(input): Json<DecideDecisionInput>,
) -> Result<Json<Value>, AppError> {
    let pool = &state.pool;
    let row = load_decision(pool, decision_id).await?;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, false)?;
    require_board(&actor)?;

    let status: String = row.get("status");
    let options: Value = row.try_get("options").unwrap_or(Value::Null);
    let expires_at: DateTime<Utc> = row.get("expires_at");
    let now = Utc::now();

    // Spec integrity check (Paperclip `verifyDecisionSpec`).
    let expected_spec = sign_decision_spec(
        company_id,
        row.get("origin_agent_id"),
        row.get("origin_issue_id"),
        &row.get::<String, _>("title"),
        &options,
        &row.try_get::<Option<Value>, _>("inputs")
            .unwrap_or(None)
            .unwrap_or_else(|| json!([])),
        expires_at,
    );
    if row.get::<String, _>("signed_spec") != expected_spec {
        return Err(AppError::Conflict(
            "Decision spec signature does not match the stored decision".to_string(),
        ));
    }

    if status == "decided" {
        // Idempotent replay: same option (and same idempotency key when supplied).
        let chosen: Option<String> = row.try_get("chosen_option_id").unwrap_or(None);
        let stored_key: Option<String> = row.try_get("idempotency_key").unwrap_or(None);
        let key_matches = match (input.idempotency_key.as_deref(), stored_key.as_deref()) {
            (Some(provided), Some(stored)) => provided == stored,
            (None, _) => true,
            _ => false,
        };
        if chosen.as_deref() == Some(input.option_id.as_str()) && key_matches {
            return Ok(Json(decision_outcome(pool, &row).await?));
        }
        return Err(AppError::Conflict(
            "Decision has already been decided".to_string(),
        ));
    }
    if status != "open" {
        return Err(AppError::Conflict(format!(
            "Decision is {status} and can no longer be decided"
        )));
    }
    if expires_at <= now {
        sqlx::query("UPDATE decisions SET status = 'expired', updated_at = NOW() WHERE id = $1")
            .bind(decision_id)
            .execute(pool)
            .await
            .map_err(db_err)?;
        return Err(AppError::Conflict("Decision has expired".to_string()));
    }

    let option = find_option(&options, &input.option_id)
        .ok_or_else(|| AppError::Validation(format!("Unknown optionId '{}'", input.option_id)))?
        .clone();

    // Required inputs must be present.
    let inputs: Value = row
        .try_get::<Option<Value>, _>("inputs")
        .unwrap_or(None)
        .unwrap_or_else(|| json!([]));
    let input_values = input.input_values.clone().unwrap_or_else(|| json!({}));
    if let Some(defs) = inputs.as_array() {
        for def in defs {
            let id = def.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let required = def
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let provided = input_values
                .get(id)
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            if required && !provided {
                return Err(AppError::Validation(format!("Input '{id}' is required")));
            }
            if let (Some(max), Some(value)) = (
                def.get("maxLength").and_then(|v| v.as_u64()),
                input_values.get(id).and_then(|v| v.as_str()),
            ) {
                if value.chars().count() as u64 > max {
                    return Err(AppError::Validation(format!(
                        "Input '{id}' exceeds {max} characters"
                    )));
                }
            }
        }
    }

    let decided_by = actor.principal_id().map(|id| id.to_string());
    let updated = sqlx::query(&format!(
        "UPDATE decisions SET status = 'decided', execution_status = 'running', \
             chosen_option_id = $2, input_values = $3, decided_by_user_id = $4, \
             decided_at = NOW(), idempotency_key = COALESCE(idempotency_key, $5), updated_at = NOW() \
         WHERE id = $1 AND status = 'open' \
         RETURNING {}",
        DECISION_SELECT.trim_start_matches("SELECT ").replace(" FROM decisions", "")
    ))
    .bind(decision_id)
    .bind(&input.option_id)
    .bind(&input_values)
    .bind(&decided_by)
    .bind(&input.idempotency_key)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::Conflict("Decision is no longer open".to_string()))?;

    claim_effects(pool, company_id, decision_id, &option).await?;
    log_activity(
        pool,
        company_id,
        &actor,
        "decision.decided",
        "decision",
        decision_id,
        json!({ "optionId": input.option_id }),
    )
    .await;

    Ok(Json(decision_outcome(pool, &updated).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct DismissDecisionInput {
    pub reason: Option<String>,
}

/// POST /decisions/:decision_id/dismiss
async fn dismiss_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(decision_id): Path<Uuid>,
    Json(input): Json<DismissDecisionInput>,
) -> Result<Json<Value>, AppError> {
    let pool = &state.pool;
    let row = load_decision(pool, decision_id).await?;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, false)?;
    require_board(&actor)?;

    let status: String = row.get("status");
    if status != "open" {
        return Err(AppError::Conflict(format!(
            "Decision is {status} and can no longer be dismissed"
        )));
    }

    // Paperclip prefers a no-op option (empty effects) so the agent sees a real choice.
    let options: Value = row.try_get("options").unwrap_or(Value::Null);
    let noop_option_id = options.as_array().and_then(|arr| {
        arr.iter()
            .find(|option| {
                option
                    .get("effects")
                    .and_then(|v| v.as_array())
                    .map(|e| e.is_empty())
                    .unwrap_or(true)
            })
            .and_then(|option| option.get("id").and_then(|v| v.as_str()))
            .map(str::to_string)
    });
    let chosen_option_id = noop_option_id.unwrap_or_else(|| "dismissed".to_string());

    let mut metadata: Value = row.try_get("metadata").unwrap_or_else(|_| json!({}));
    if !metadata.is_object() {
        metadata = json!({});
    }
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("dismissed".into(), Value::Bool(true));
        obj.insert(
            "dismissReason".into(),
            input
                .reason
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
    }

    let decided_by = actor.principal_id().map(|id| id.to_string());
    let updated = sqlx::query(&format!(
        "UPDATE decisions SET status = 'decided', execution_status = 'skipped', \
             chosen_option_id = $2, decided_by_user_id = $3, decided_at = NOW(), \
             metadata = $4, updated_at = NOW() \
         WHERE id = $1 AND status = 'open' \
         RETURNING {}",
        DECISION_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decisions", "")
    ))
    .bind(decision_id)
    .bind(&chosen_option_id)
    .bind(&decided_by)
    .bind(&metadata)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::Conflict("Decision is no longer open".to_string()))?;

    log_activity(
        pool,
        company_id,
        &actor,
        "decision.dismissed",
        "decision",
        decision_id,
        json!({ "reason": input.reason }),
    )
    .await;

    Ok(Json(decision_outcome(pool, &updated).await?))
}

/// POST /decisions/:decision_id/cancel — only the origin agent may cancel.
async fn cancel_decision(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(decision_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let pool = &state.pool;
    let row = load_decision(pool, decision_id).await?;
    let company_id: Uuid = row.get("company_id");
    require_company_access(&actor, company_id, false)?;

    let (agent_id, _) = agent_context(&actor)?;
    if agent_id != row.get::<Uuid, _>("origin_agent_id") {
        return Err(forbid("Only the origin agent may cancel this decision"));
    }

    let status: String = row.get("status");
    if status != "open" {
        return Err(AppError::Conflict(format!(
            "Decision is {status} and can no longer be cancelled"
        )));
    }

    let updated = sqlx::query(&format!(
        "UPDATE decisions SET status = 'cancelled', updated_at = NOW() \
         WHERE id = $1 AND status = 'open' RETURNING {}",
        DECISION_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decisions", "")
    ))
    .bind(decision_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::Conflict("Decision is no longer open".to_string()))?;

    log_activity(
        pool,
        company_id,
        &actor,
        "decision.cancelled",
        "decision",
        decision_id,
        json!({}),
    )
    .await;

    Ok(Json(decision_outcome(pool, &updated).await?))
}

// ---------------------------------------------------------------------------
// Decision queues
// ---------------------------------------------------------------------------

/// GET /companies/:company_id/decision-queue-seed-rules
async fn get_decision_queue_seed_rules(
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    Ok(Json(decision_queue_seeds()))
}

fn queue_to_json(r: &PgRow, item_count: i64) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "key": r.get::<String, _>("key"),
        "title": r.get::<String, _>("title"),
        "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
        "createdByType": r.get::<String, _>("created_by_type"),
        "createdByAgentId": r.try_get::<Option<Uuid>, _>("created_by_agent_id").unwrap_or(None),
        "createdByUserId": r.try_get::<Option<String>, _>("created_by_user_id").unwrap_or(None),
        "createdByRunId": r.try_get::<Option<Uuid>, _>("created_by_run_id").unwrap_or(None),
        "retentionDays": r.try_get::<Option<i32>, _>("retention_days").unwrap_or(None),
        "seedRules": r.try_get::<Value, _>("seed_rules").unwrap_or(Value::Null),
        "seedRulesEnabled": r.try_get::<bool, _>("seed_rules_enabled").unwrap_or(false),
        "itemCount": item_count,
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
    })
}

const QUEUE_SELECT: &str = "SELECT id, company_id, key, title, description, created_by_type, \
     created_by_agent_id, created_by_user_id, created_by_run_id, retention_days, seed_rules, \
     seed_rules_enabled, created_at, updated_at FROM decision_queues";

/// `packages/shared/src/validators/decision-queue.ts` → kebab-case key, max 80 chars.
fn validate_queue_key(key: &str) -> Result<(), AppError> {
    if key.is_empty() || key.len() > 80 {
        return Err(AppError::Validation(
            "queue key must be 1-80 characters".to_string(),
        ));
    }
    let valid = key.split('-').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    });
    if !valid {
        return Err(AppError::Validation(
            "queue key must be lowercase kebab-case (a-z, 0-9, single hyphens)".to_string(),
        ));
    }
    Ok(())
}

/// GET /companies/:company_id/decision-queues
async fn list_decision_queues(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    let pool = &state.pool;
    let rows = sqlx::query(&format!(
        "{QUEUE_SELECT} WHERE company_id = $1 ORDER BY title ASC, key ASC"
    ))
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let counts = sqlx::query(
        "SELECT queue_id, COUNT(*)::bigint AS item_count FROM decision_queue_items \
          WHERE company_id = $1 GROUP BY queue_id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let mut count_map: HashMap<Uuid, i64> = HashMap::new();
    for r in &counts {
        count_map.insert(r.get("queue_id"), r.get("item_count"));
    }

    let queues: Vec<Value> = rows
        .iter()
        .map(|r| {
            let count = count_map.get(&r.get::<Uuid, _>("id")).copied().unwrap_or(0);
            queue_to_json(r, count)
        })
        .collect();
    Ok(Json(Value::Array(queues)))
}

#[derive(Debug, Deserialize)]
pub struct CreateDecisionQueueInput {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "retentionDays")]
    pub retention_days: Option<i32>,
    #[serde(rename = "seedRules")]
    pub seed_rules: Option<Value>,
    #[serde(rename = "seedRulesEnabled")]
    pub seed_rules_enabled: Option<bool>,
}

/// POST /companies/:company_id/decision-queues
async fn create_decision_queue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateDecisionQueueInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    validate_queue_key(&input.key)?;
    let title = input.title.trim().to_string();
    if title.is_empty() || title.len() > 120 {
        return Err(AppError::Validation(
            "title must be 1-120 characters".to_string(),
        ));
    }
    if let Some(description) = input.description.as_deref() {
        if description.len() > 2000 {
            return Err(AppError::Validation(
                "description may not exceed 2000 characters".to_string(),
            ));
        }
    }
    if let Some(days) = input.retention_days {
        if !(1..=3650).contains(&days) {
            return Err(AppError::Validation(
                "retentionDays must be between 1 and 3650".to_string(),
            ));
        }
    }

    let attr = attribution(&actor);
    let pool = &state.pool;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM decision_queues WHERE company_id = $1 AND key = $2")
            .bind(company_id)
            .bind(&input.key)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "Queue '{}' already exists",
            input.key
        )));
    }

    let row = sqlx::query(&format!(
        "INSERT INTO decision_queues (company_id, key, title, description, created_by_type, \
             created_by_agent_id, created_by_user_id, created_by_run_id, \
             created_by_agent_api_key_id, retention_days, seed_rules, seed_rules_enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         RETURNING {}",
        QUEUE_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decision_queues", "")
    ))
    .bind(company_id)
    .bind(&input.key)
    .bind(&title)
    .bind(&input.description)
    .bind(attr.actor_type)
    .bind(attr.agent_id)
    .bind(&attr.user_id)
    .bind(attr.run_id)
    .bind(attr.api_key_id)
    .bind(input.retention_days)
    .bind(input.seed_rules.unwrap_or_else(|| json!([])))
    .bind(input.seed_rules_enabled.unwrap_or(false))
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    Ok((StatusCode::CREATED, Json(queue_to_json(&row, 0))))
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateDecisionQueueInput {
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "retentionDays")]
    pub retention_days: Option<i32>,
    #[serde(rename = "seedRules")]
    pub seed_rules: Option<Value>,
    #[serde(rename = "seedRulesEnabled")]
    pub seed_rules_enabled: Option<bool>,
}

/// PATCH /companies/:company_id/decision-queues/:key
async fn update_decision_queue(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, key)): Path<(Uuid, String)>,
    Json(input): Json<UpdateDecisionQueueInput>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    if let Some(title) = input.title.as_deref() {
        if title.trim().is_empty() || title.len() > 120 {
            return Err(AppError::Validation(
                "title must be 1-120 characters".to_string(),
            ));
        }
    }
    if let Some(days) = input.retention_days {
        if !(1..=3650).contains(&days) {
            return Err(AppError::Validation(
                "retentionDays must be between 1 and 3650".to_string(),
            ));
        }
    }

    let pool = &state.pool;
    let row = sqlx::query(&format!(
        "UPDATE decision_queues SET \
             title = COALESCE($3, title), \
             description = COALESCE($4, description), \
             retention_days = COALESCE($5, retention_days), \
             seed_rules = COALESCE($6, seed_rules), \
             seed_rules_enabled = COALESCE($7, seed_rules_enabled), \
             updated_at = NOW() \
         WHERE company_id = $1 AND key = $2 RETURNING {}",
        QUEUE_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decision_queues", "")
    ))
    .bind(company_id)
    .bind(&key)
    .bind(input.title.as_ref().map(|t| t.trim().to_string()))
    .bind(&input.description)
    .bind(input.retention_days)
    .bind(&input.seed_rules)
    .bind(input.seed_rules_enabled)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::NotFound(format!("Queue '{key}' not found")))?;

    let item_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM decision_queue_items WHERE queue_id = $1")
            .bind(row.get::<Uuid, _>("id"))
            .fetch_one(pool)
            .await
            .map_err(db_err)?;

    Ok(Json(queue_to_json(&row, item_count)))
}

fn queue_item_to_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "queueId": r.get::<Uuid, _>("queue_id"),
        "sourceKind": r.get::<String, _>("source_kind"),
        "sourceId": r.get::<String, _>("source_id"),
        "addedByType": r.get::<String, _>("added_by_type"),
        "addedByAgentId": r.try_get::<Option<Uuid>, _>("added_by_agent_id").unwrap_or(None),
        "addedByUserId": r.try_get::<Option<String>, _>("added_by_user_id").unwrap_or(None),
        "addedByRunId": r.try_get::<Option<Uuid>, _>("added_by_run_id").unwrap_or(None),
        "responsibleUserId": r.try_get::<Option<String>, _>("responsible_user_id").unwrap_or(None),
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
    })
}

const QUEUE_ITEM_SELECT: &str = "SELECT id, company_id, queue_id, source_kind, source_id, \
     added_by_type, added_by_agent_id, added_by_user_id, added_by_run_id, responsible_user_id, \
     created_at FROM decision_queue_items";

async fn resolve_queue_id(pool: &PgPool, company_id: Uuid, key: &str) -> Result<Uuid, AppError> {
    sqlx::query_scalar("SELECT id FROM decision_queues WHERE company_id = $1 AND key = $2")
        .bind(company_id)
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::NotFound(format!("Queue '{key}' not found")))
}

/// GET /companies/:company_id/decision-queues/:key/items
async fn list_decision_queue_items(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, key)): Path<(Uuid, String)>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    let pool = &state.pool;
    let queue_id = resolve_queue_id(pool, company_id, &key).await?;
    let rows = sqlx::query(&format!(
        "{QUEUE_ITEM_SELECT} WHERE queue_id = $1 ORDER BY created_at DESC"
    ))
    .bind(queue_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(Json(Value::Array(
        rows.iter().map(queue_item_to_json).collect(),
    )))
}

#[derive(Debug, Deserialize)]
pub struct AddDecisionQueueItemInput {
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
    #[serde(rename = "sourceId")]
    pub source_id: String,
}

/// POST /companies/:company_id/decision-queues/:key/items
async fn add_decision_queue_item(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, key)): Path<(Uuid, String)>,
    Json(input): Json<AddDecisionQueueItemInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    if !is_valid_source_kind(&input.source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{}'",
            input.source_kind
        )));
    }
    if input.source_id.is_empty() || input.source_id.len() > 500 {
        return Err(AppError::Validation(
            "sourceId must be 1-500 characters".to_string(),
        ));
    }

    let pool = &state.pool;
    let queue_id = resolve_queue_id(pool, company_id, &key).await?;
    let attr = attribution(&actor);

    let row = sqlx::query(&format!(
        "INSERT INTO decision_queue_items (company_id, queue_id, source_kind, source_id, \
             added_by_type, added_by_agent_id, added_by_user_id, added_by_run_id, \
             added_by_agent_api_key_id, responsible_user_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (queue_id, source_kind, source_id) DO UPDATE SET source_id = EXCLUDED.source_id \
         RETURNING {}",
        QUEUE_ITEM_SELECT.trim_start_matches("SELECT ").replace(" FROM decision_queue_items", "")
    ))
    .bind(company_id)
    .bind(queue_id)
    .bind(&input.source_kind)
    .bind(&input.source_id)
    .bind(attr.actor_type)
    .bind(attr.agent_id)
    .bind(&attr.user_id)
    .bind(attr.run_id)
    .bind(attr.api_key_id)
    .bind(&attr.responsible_user_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    record_triage_event(
        pool,
        company_id,
        &actor,
        Some(queue_id),
        Some(&input.source_kind),
        Some(&input.source_id),
        "queue_item_added",
        json!({ "queueKey": key }),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(queue_item_to_json(&row))))
}

/// DELETE /companies/:company_id/decision-queues/:key/items/:source_kind/:source_id
async fn remove_decision_queue_item(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, key, source_kind, source_id)): Path<(Uuid, String, String, String)>,
) -> Result<StatusCode, AppError> {
    require_company_access(&actor, company_id, false)?;
    let pool = &state.pool;
    let queue_id = resolve_queue_id(pool, company_id, &key).await?;
    let result = sqlx::query(
        "DELETE FROM decision_queue_items \
          WHERE queue_id = $1 AND source_kind = $2 AND source_id = $3",
    )
    .bind(queue_id)
    .bind(&source_kind)
    .bind(&source_id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Queue item not found".to_string()));
    }
    record_triage_event(
        pool,
        company_id,
        &actor,
        Some(queue_id),
        Some(&source_kind),
        Some(&source_id),
        "queue_item_removed",
        json!({ "queueKey": key }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Triage
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn record_triage_event(
    pool: &PgPool,
    company_id: Uuid,
    actor: &AuthorizationActor,
    queue_id: Option<Uuid>,
    source_kind: Option<&str>,
    source_id: Option<&str>,
    action: &str,
    details: Value,
) -> Result<(), AppError> {
    let attr = attribution(actor);
    sqlx::query(
        "INSERT INTO decision_triage_events (company_id, queue_id, source_kind, source_id, action, \
             actor_type, actor_agent_id, actor_user_id, actor_run_id, agent_api_key_id, \
             responsible_user_id, details) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(company_id)
    .bind(queue_id)
    .bind(source_kind)
    .bind(source_id)
    .bind(action)
    .bind(attr.actor_type)
    .bind(attr.agent_id)
    .bind(&attr.user_id)
    .bind(attr.run_id)
    .bind(attr.api_key_id)
    .bind(&attr.responsible_user_id)
    .bind(details)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

fn triage_to_json(r: &PgRow) -> Value {
    let raw_decide_by: Option<String> = r.try_get("decide_by").unwrap_or(None);
    let decide_by_date: Option<NaiveDate> = r.try_get("decide_by_date").unwrap_or(None);
    let decide_by = match raw_decide_by.as_deref() {
        Some("date") => decide_by_date.map(|d| d.format("%Y-%m-%d").to_string()),
        other => other.map(str::to_string),
    };
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "sourceKind": r.get::<String, _>("source_kind"),
        "sourceId": r.get::<String, _>("source_id"),
        "decideBy": decide_by,
        "snoozedUntil": iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("snoozed_until").unwrap_or(None)),
        "setByType": r.get::<String, _>("set_by_type"),
        "setByAgentId": r.try_get::<Option<Uuid>, _>("set_by_agent_id").unwrap_or(None),
        "setByUserId": r.try_get::<Option<String>, _>("set_by_user_id").unwrap_or(None),
        "setByRunId": r.try_get::<Option<Uuid>, _>("set_by_run_id").unwrap_or(None),
        "responsibleUserId": r.try_get::<Option<String>, _>("responsible_user_id").unwrap_or(None),
        "version": r.get::<i32, _>("version"),
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
    })
}

const TRIAGE_SELECT: &str = "SELECT id, company_id, source_kind, source_id, decide_by, \
     decide_by_date, snoozed_until, set_by_type, set_by_agent_id, set_by_user_id, set_by_run_id, \
     responsible_user_id, version, created_at, updated_at FROM decision_triage";

/// GET /companies/:company_id/decision-triage/:source_kind/:source_id
async fn get_decision_triage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, source_kind, source_id)): Path<(Uuid, String, String)>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, true)?;
    if !is_valid_source_kind(&source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{source_kind}'"
        )));
    }
    require_decision_source_read(&state.pool, &actor, company_id, &source_kind, &source_id).await?;
    let pool = &state.pool;
    let row = sqlx::query(&format!(
        "{TRIAGE_SELECT} WHERE company_id = $1 AND source_kind = $2 AND source_id = $3"
    ))
    .bind(company_id)
    .bind(&source_kind)
    .bind(&source_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;

    Ok(Json(match row {
        Some(r) => triage_to_json(&r),
        // Paperclip returns the empty triage shape rather than a 404.
        None => json!({
            "id": Value::Null,
            "companyId": company_id,
            "sourceKind": source_kind,
            "sourceId": source_id,
            "decideBy": Value::Null,
            "snoozedUntil": Value::Null,
            "setByType": Value::Null,
            "setByAgentId": Value::Null,
            "setByUserId": Value::Null,
            "setByRunId": Value::Null,
            "responsibleUserId": Value::Null,
            "version": 0,
            "createdAt": Value::Null,
            "updatedAt": Value::Null,
        }),
    }))
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateDecisionTriageInput {
    #[serde(rename = "decideBy")]
    pub decide_by: Option<String>,
    #[serde(rename = "snoozedUntil")]
    pub snoozed_until: Option<String>,
}

/// PUT /companies/:company_id/decision-triage/:source_kind/:source_id
///
/// Concurrent updates on the same source serialize behind a transaction-scoped
/// advisory lock, mirroring Paperclip's `decision-triage:${...}` lock.
async fn put_decision_triage(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, source_kind, source_id)): Path<(Uuid, String, String)>,
    Json(input): Json<UpdateDecisionTriageInput>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    if !is_valid_source_kind(&source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{source_kind}'"
        )));
    }

    // decideBy is either a bucket keyword or a calendar date.
    let (decide_by, decide_by_date) = match input.decide_by.as_deref() {
        None => (None, None),
        Some("") => (None, None),
        Some(value) if matches!(value, "today" | "this_week" | "whenever") => {
            (Some(value.to_string()), None)
        }
        Some(value) => {
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                AppError::Validation(
                    "decideBy must be 'today', 'this_week', 'whenever', or YYYY-MM-DD".to_string(),
                )
            })?;
            (Some("date".to_string()), Some(date))
        }
    };

    let snoozed_until = match input.snoozed_until.as_deref() {
        None | Some("") => None,
        Some(value) => {
            let parsed = parse_iso(value).ok_or_else(|| {
                AppError::Validation("snoozedUntil must be an ISO timestamp".to_string())
            })?;
            // Paperclip caps snoozes at 5 years out.
            if parsed > Utc::now() + Duration::days(365 * 5) {
                return Err(AppError::Validation(
                    "snoozedUntil may not be more than 5 years in the future".to_string(),
                ));
            }
            Some(parsed)
        }
    };

    let attr = attribution(&actor);
    let mut tx = state.pool.begin().await.map_err(db_err)?;

    let lock_key = format!("decision-triage:{company_id}:{source_kind}:{source_id}");
    let lock_id = i64::from_be_bytes(
        Sha256::digest(lock_key.as_bytes())[..8]
            .try_into()
            .expect("sha256 digest yields at least 8 bytes"),
    );
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(lock_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    let row = sqlx::query(&format!(
        "INSERT INTO decision_triage (company_id, source_kind, source_id, decide_by, \
             decide_by_date, snoozed_until, set_by_type, set_by_agent_id, set_by_user_id, \
             set_by_run_id, set_by_agent_api_key_id, responsible_user_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (company_id, source_kind, source_id) DO UPDATE SET \
             decide_by = EXCLUDED.decide_by, \
             decide_by_date = EXCLUDED.decide_by_date, \
             snoozed_until = EXCLUDED.snoozed_until, \
             set_by_type = EXCLUDED.set_by_type, \
             set_by_agent_id = EXCLUDED.set_by_agent_id, \
             set_by_user_id = EXCLUDED.set_by_user_id, \
             set_by_run_id = EXCLUDED.set_by_run_id, \
             set_by_agent_api_key_id = EXCLUDED.set_by_agent_api_key_id, \
             responsible_user_id = EXCLUDED.responsible_user_id, \
             version = decision_triage.version + 1, \
             updated_at = NOW() \
         RETURNING {}",
        TRIAGE_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decision_triage", "")
    ))
    .bind(company_id)
    .bind(&source_kind)
    .bind(&source_id)
    .bind(&decide_by)
    .bind(decide_by_date)
    .bind(snoozed_until)
    .bind(attr.actor_type)
    .bind(attr.agent_id)
    .bind(&attr.user_id)
    .bind(attr.run_id)
    .bind(attr.api_key_id)
    .bind(&attr.responsible_user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;

    record_triage_event(
        &state.pool,
        company_id,
        &actor,
        None,
        Some(&source_kind),
        Some(&source_id),
        if snoozed_until.is_some() {
            "snoozed"
        } else {
            "triage_updated"
        },
        json!({ "decideBy": decide_by, "snoozedUntil": snoozed_until.map(iso) }),
    )
    .await?;

    Ok(Json(triage_to_json(&row)))
}

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

fn retention_to_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "sourceKind": r.get::<String, _>("source_kind"),
        "sourceId": r.get::<String, _>("source_id"),
        "sourceActivityAt": iso(r.get::<DateTime<Utc>, _>("source_activity_at")),
        "keep": r.get::<bool, _>("keep"),
        "archivedAt": iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("archived_at").unwrap_or(None)),
        "archivedReason": r.try_get::<Option<String>, _>("archived_reason").unwrap_or(None),
        "archivedByType": r.try_get::<Option<String>, _>("archived_by_type").unwrap_or(None),
        "archivedByAgentId": r.try_get::<Option<Uuid>, _>("archived_by_agent_id").unwrap_or(None),
        "archivedByUserId": r.try_get::<Option<String>, _>("archived_by_user_id").unwrap_or(None),
        "archivedByRunId": r.try_get::<Option<Uuid>, _>("archived_by_run_id").unwrap_or(None),
        "version": r.get::<i32, _>("version"),
        "archiveVersion": r.get::<i32, _>("archive_version"),
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
    })
}

const RETENTION_SELECT: &str =
    "SELECT id, company_id, source_kind, source_id, source_activity_at, \
     keep, archived_at, archived_reason, archived_by_type, archived_by_agent_id, \
     archived_by_user_id, archived_by_run_id, version, archive_version, created_at, updated_at \
     FROM decision_retention";

fn retention_returning() -> String {
    RETENTION_SELECT
        .trim_start_matches("SELECT ")
        .replace(" FROM decision_retention", "")
}

#[derive(Debug, Deserialize)]
pub struct UpdateDecisionRetentionInput {
    pub keep: bool,
}

/// PATCH /companies/:company_id/decision-retention/:source_kind/:source_id
async fn patch_decision_retention(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, source_kind, source_id)): Path<(Uuid, String, String)>,
    Json(input): Json<UpdateDecisionRetentionInput>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    if !is_valid_source_kind(&source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{source_kind}'"
        )));
    }
    require_decision_source_read(&state.pool, &actor, company_id, &source_kind, &source_id).await?;
    let pool = &state.pool;
    let row = sqlx::query(&format!(
        "INSERT INTO decision_retention (company_id, source_kind, source_id, source_activity_at, keep) \
         VALUES ($1, $2, $3, NOW(), $4) \
         ON CONFLICT (company_id, source_kind, source_id) DO UPDATE SET \
             keep = EXCLUDED.keep, version = decision_retention.version + 1, updated_at = NOW() \
         RETURNING {}",
        retention_returning()
    ))
    .bind(company_id)
    .bind(&source_kind)
    .bind(&source_id)
    .bind(input.keep)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    record_triage_event(
        pool,
        company_id,
        &actor,
        None,
        Some(&source_kind),
        Some(&source_id),
        if input.keep { "kept" } else { "unkept" },
        json!({ "keep": input.keep }),
    )
    .await?;

    Ok(Json(retention_to_json(&row)))
}

#[derive(Debug, Default, Deserialize)]
pub struct ArchiveDecisionRetentionInput {
    pub reason: Option<String>,
}

/// POST /companies/:company_id/decision-retention/:source_kind/:source_id/archive
async fn archive_decision_retention(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, source_kind, source_id)): Path<(Uuid, String, String)>,
    Json(input): Json<ArchiveDecisionRetentionInput>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    if !is_valid_source_kind(&source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{source_kind}'"
        )));
    }
    require_decision_source_read(&state.pool, &actor, company_id, &source_kind, &source_id).await?;
    let attr = attribution(&actor);
    let reason = input.reason.unwrap_or_else(|| "manual".to_string());
    let pool = &state.pool;

    let row = sqlx::query(&format!(
        "UPDATE decision_retention SET \
             archived_at = NOW(), archived_reason = $4, archived_by_type = $5, \
             archived_by_agent_id = $6, archived_by_user_id = $7, archived_by_run_id = $8, \
             archive_version = archive_version + 1, version = version + 1, updated_at = NOW() \
         WHERE company_id = $1 AND source_kind = $2 AND source_id = $3 AND archived_at IS NULL \
         RETURNING {}",
        retention_returning()
    ))
    .bind(company_id)
    .bind(&source_kind)
    .bind(&source_id)
    .bind(&reason)
    .bind(attr.actor_type)
    .bind(attr.agent_id)
    .bind(&attr.user_id)
    .bind(attr.run_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;

    let row = match row {
        Some(row) => row,
        None => {
            // Either the source has no retention row yet, or it is already archived.
            let existing = sqlx::query(&format!(
                "{RETENTION_SELECT} WHERE company_id = $1 AND source_kind = $2 AND source_id = $3"
            ))
            .bind(company_id)
            .bind(&source_kind)
            .bind(&source_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
            match existing {
                // Already archived → idempotent success.
                Some(row) => return Ok(Json(retention_to_json(&row))),
                None => {
                    return Err(AppError::NotFound(
                        "Retention record not found for this source".to_string(),
                    ))
                }
            }
        }
    };

    record_triage_event(
        pool,
        company_id,
        &actor,
        None,
        Some(&source_kind),
        Some(&source_id),
        "archived",
        json!({ "reason": reason }),
    )
    .await?;

    Ok(Json(retention_to_json(&row)))
}

/// POST /companies/:company_id/decision-retention/:source_kind/:source_id/revive
async fn revive_decision_retention(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path((company_id, source_kind, source_id)): Path<(Uuid, String, String)>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    if !is_valid_source_kind(&source_kind) {
        return Err(AppError::Validation(format!(
            "unknown sourceKind '{source_kind}'"
        )));
    }
    require_decision_source_read(&state.pool, &actor, company_id, &source_kind, &source_id).await?;
    let pool = &state.pool;
    let row = sqlx::query(&format!(
        "UPDATE decision_retention SET \
             archived_at = NULL, archived_reason = NULL, archived_by_type = NULL, \
             archived_by_agent_id = NULL, archived_by_user_id = NULL, archived_by_run_id = NULL, \
             version = version + 1, updated_at = NOW() \
         WHERE company_id = $1 AND source_kind = $2 AND source_id = $3 AND archived_at IS NOT NULL \
         RETURNING {}",
        retention_returning()
    ))
    .bind(company_id)
    .bind(&source_kind)
    .bind(&source_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;

    let row = match row {
        Some(row) => row,
        None => {
            let existing = sqlx::query(&format!(
                "{RETENTION_SELECT} WHERE company_id = $1 AND source_kind = $2 AND source_id = $3"
            ))
            .bind(company_id)
            .bind(&source_kind)
            .bind(&source_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
            match existing {
                Some(row) => return Ok(Json(retention_to_json(&row))),
                None => {
                    return Err(AppError::NotFound(
                        "Retention record not found for this source".to_string(),
                    ))
                }
            }
        }
    };

    record_triage_event(
        pool,
        company_id,
        &actor,
        None,
        Some(&source_kind),
        Some(&source_id),
        "revived",
        json!({}),
    )
    .await?;

    Ok(Json(retention_to_json(&row)))
}

// ---------------------------------------------------------------------------
// Decision training
// ---------------------------------------------------------------------------

const TRAINING_SOURCE_KINDS: [&str; 3] = ["interaction", "approval", "execution_decision"];

fn training_example_to_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": r.get::<Uuid, _>("company_id"),
        "sourceKind": r.get::<String, _>("source_kind"),
        "sourceId": r.get::<Uuid, _>("source_id"),
        "issueId": r.get::<Uuid, _>("issue_id"),
        "cutoffAt": iso(r.get::<DateTime<Utc>, _>("cutoff_at")),
        "notes": r.get::<String, _>("notes"),
        "notesHistory": r.try_get::<Value, _>("notes_history").unwrap_or(Value::Null),
        "tags": r.try_get::<Value, _>("tags").unwrap_or_else(|_| json!([])),
        "qualityScore": r.try_get::<Option<f32>, _>("quality_score").unwrap_or(None),
        "decisionOutcome": r.try_get::<Option<String>, _>("decision_outcome").unwrap_or(None),
        "retentionPolicy": r.get::<String, _>("retention_policy"),
        "snapshot": r.try_get::<Value, _>("snapshot").unwrap_or(Value::Null),
        "createdByUserId": r.get::<String, _>("created_by_user_id"),
        "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
    })
}

fn training_service_example_to_json(example: &services::DecisionTrainingExample) -> Value {
    json!({
        "id": example.id,
        "companyId": example.company_id,
        "sourceKind": match &example.source_kind {
            DecisionTrainingSourceKind::IssueThreadInteraction => "interaction",
            DecisionTrainingSourceKind::IssueApproval => "approval",
            DecisionTrainingSourceKind::IssueExecutionDecision => "execution_decision",
            DecisionTrainingSourceKind::HeartbeatDecision => "interaction",
        },
        "sourceId": example.source_id,
        "issueId": example.snapshot.issue_id,
        "cutoffAt": iso(example.cutoff_at),
        "notes": example.notes.clone().unwrap_or_default(),
        "notesHistory": serde_json::to_value(&example.notes_history).unwrap_or_else(|_| json!([])),
        "tags": example.tags.clone(),
        "qualityScore": example.quality_score,
        "decisionOutcome": example.snapshot.decision_outcome.clone(),
        "retentionPolicy": example.retention_policy,
        "snapshot": example.snapshot.clone(),
        "createdByUserId": example.created_by_user_id.clone(),
        "createdAt": iso(example.created_at),
        "updatedAt": iso(example.updated_at),
    })
}

const TRAINING_SELECT: &str =
    "SELECT id, company_id, source_kind, source_id, issue_id, cutoff_at, \
     notes, notes_history, tags, quality_score, decision_outcome, retention_policy, snapshot, created_by_user_id, \
     created_at, updated_at FROM decision_training_examples";

struct SourceDecision {
    cutoff_at: DateTime<Utc>,
    outcome: Option<String>,
    payload: Value,
    actor: Value,
    exact_run_id: Option<Uuid>,
}

/// Paperclip `loadSourceDecision()`.
async fn load_source_decision(
    pool: &PgPool,
    company_id: Uuid,
    source_kind: &str,
    source_id: Uuid,
    issue_id: Uuid,
    captured_at: DateTime<Utc>,
) -> Result<SourceDecision, AppError> {
    match source_kind {
        "interaction" => {
            let row = sqlx::query(
                "SELECT id, kind, status::text AS status, title, summary, payload, result, \
                        source_run_id, created_by_agent_id, created_by_user_id, \
                        resolved_by_agent_id, resolved_by_user_id, resolved_at \
                   FROM issue_thread_interactions \
                  WHERE id = $1 AND company_id = $2 AND issue_id = $3",
            )
            .bind(source_id)
            .bind(company_id)
            .bind(issue_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::NotFound("Decision interaction not found".to_string()))?;

            let status: String = row.get("status");
            let resolved_at: Option<DateTime<Utc>> = row.try_get("resolved_at").unwrap_or(None);
            let resolved = resolved_at.is_some() && status != "pending";
            Ok(SourceDecision {
                cutoff_at: if resolved {
                    resolved_at.unwrap()
                } else {
                    captured_at
                },
                outcome: resolved.then_some(status),
                payload: json!({
                    "kind": row.get::<String, _>("kind"),
                    "title": row.try_get::<Option<String>, _>("title").unwrap_or(None),
                    "summary": row.try_get::<Option<String>, _>("summary").unwrap_or(None),
                    "payload": row.try_get::<Option<Value>, _>("payload").unwrap_or(None),
                    "result": if resolved { row.try_get::<Option<Value>, _>("result").unwrap_or(None) } else { None },
                }),
                actor: if resolved {
                    json!({
                        "userId": row.try_get::<Option<String>, _>("resolved_by_user_id").unwrap_or(None),
                        "agentId": row.try_get::<Option<Uuid>, _>("resolved_by_agent_id").unwrap_or(None),
                    })
                } else {
                    json!({
                        "userId": row.try_get::<Option<String>, _>("created_by_user_id").unwrap_or(None),
                        "agentId": row.try_get::<Option<Uuid>, _>("created_by_agent_id").unwrap_or(None),
                    })
                },
                exact_run_id: row.try_get("source_run_id").unwrap_or(None),
            })
        }
        "approval" => {
            let row = sqlx::query(
                "SELECT a.id, a.approval_type::text AS approval_type, a.status::text AS status, \
                        a.payload, a.decision_note, a.decided_by_user_id, a.decided_at, \
                        a.requested_by_user_id, a.requested_by_agent_id \
                   FROM approvals a \
                   JOIN issue_approvals ia ON ia.approval_id = a.id AND ia.issue_id = $3 \
                  WHERE a.id = $1 AND a.company_id = $2 LIMIT 1",
            )
            .bind(source_id)
            .bind(company_id)
            .bind(issue_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::NotFound("Decision approval not found".to_string()))?;

            let status: String = row.get("status");
            let decided_at: Option<DateTime<Utc>> = row.try_get("decided_at").unwrap_or(None);
            let resolved = decided_at.is_some() && status != "pending";
            Ok(SourceDecision {
                cutoff_at: if resolved {
                    decided_at.unwrap()
                } else {
                    captured_at
                },
                outcome: resolved.then_some(status),
                payload: json!({
                    "type": row.get::<String, _>("approval_type"),
                    "payload": row.try_get::<Option<Value>, _>("payload").unwrap_or(None),
                    "decisionNote": if resolved { row.try_get::<Option<String>, _>("decision_note").unwrap_or(None) } else { None },
                }),
                actor: if resolved {
                    json!({ "userId": row.try_get::<Option<Uuid>, _>("decided_by_user_id").unwrap_or(None) })
                } else {
                    json!({
                        "userId": row.try_get::<Option<Uuid>, _>("requested_by_user_id").unwrap_or(None),
                        "agentId": row.try_get::<Option<Uuid>, _>("requested_by_agent_id").unwrap_or(None),
                    })
                },
                exact_run_id: None,
            })
        }
        "execution_decision" => {
            let row = sqlx::query(
                "SELECT id, origin_agent_id, origin_issue_id, title, body, options, inputs,
                        status::text AS status, execution_status, chosen_option_id, input_values,
                        decided_by_user_id, decided_at, origin_run_id
                   FROM decisions
                  WHERE id = $1 AND company_id = $2 AND origin_issue_id = $3",
            )
            .bind(source_id)
            .bind(company_id)
            .bind(issue_id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::NotFound("Execution decision not found".to_string()))?;

            let status: String = row.get("status");
            let decided_at: Option<DateTime<Utc>> = row.try_get("decided_at").unwrap_or(None);
            let resolved = matches!(status.as_str(), "decided" | "cancelled" | "expired")
                && decided_at.is_some();
            Ok(SourceDecision {
                cutoff_at: if resolved { decided_at.unwrap() } else { captured_at },
                outcome: resolved.then_some(status),
                payload: json!({
                    "title": row.get::<String, _>("title"),
                    "body": row.get::<String, _>("body"),
                    "options": row.get::<Value, _>("options"),
                    "inputs": row.try_get::<Option<Value>, _>("inputs").unwrap_or(None),
                    "executionStatus": row.try_get::<Option<String>, _>("execution_status").unwrap_or(None),
                    "chosenOptionId": if resolved { row.try_get::<Option<String>, _>("chosen_option_id").unwrap_or(None) } else { None },
                    "inputValues": if resolved { row.try_get::<Option<Value>, _>("input_values").unwrap_or(None) } else { None },
                }),
                actor: if resolved {
                    json!({ "userId": row.try_get::<Option<String>, _>("decided_by_user_id").unwrap_or(None) })
                } else {
                    json!({ "agentId": row.try_get::<Option<Uuid>, _>("origin_agent_id").unwrap_or(None) })
                },
                exact_run_id: row.try_get("origin_run_id").unwrap_or(None),
            })
        }
        _ => Err(AppError::BadRequest("Unsupported decision training source".to_string())),
    }
}

struct CapturedSnapshot {
    cutoff_at: DateTime<Utc>,
    decision_outcome: Option<String>,
    snapshot: Value,
}

/// Paperclip `captureDecisionSnapshot()` → DecisionTrainingSnapshotV1.
async fn capture_decision_snapshot(
    pool: &PgPool,
    company_id: Uuid,
    source_kind: &str,
    source_id: Uuid,
    issue_id: Uuid,
) -> Result<CapturedSnapshot, AppError> {
    let captured_at = Utc::now();
    let decision = load_source_decision(
        pool,
        company_id,
        source_kind,
        source_id,
        issue_id,
        captured_at,
    )
    .await?;

    let issue_row = sqlx::query(
        "SELECT id, company_id, project_id, parent_id, title, description, status::text AS status, \
                work_mode::text AS work_mode, priority::text AS priority, assignee_agent_id, \
                assignee_user_id, responsible_user_id, identifier, issue_number, created_at, updated_at \
           FROM issues WHERE id = $1 AND company_id = $2",
    )
    .bind(issue_id)
    .bind(company_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(|| AppError::NotFound("Issue not found".to_string()))?;

    let issue = json!({
        "id": issue_row.get::<Uuid, _>("id"),
        "companyId": issue_row.get::<Uuid, _>("company_id"),
        "projectId": issue_row.try_get::<Option<Uuid>, _>("project_id").unwrap_or(None),
        "parentId": issue_row.try_get::<Option<Uuid>, _>("parent_id").unwrap_or(None),
        "title": issue_row.get::<String, _>("title"),
        "description": issue_row.try_get::<Option<String>, _>("description").unwrap_or(None),
        "status": issue_row.try_get::<Option<String>, _>("status").unwrap_or(None),
        "workMode": issue_row.try_get::<Option<String>, _>("work_mode").unwrap_or(None),
        "priority": issue_row.try_get::<Option<String>, _>("priority").unwrap_or(None),
        "assigneeAgentId": issue_row.try_get::<Option<Uuid>, _>("assignee_agent_id").unwrap_or(None),
        "assigneeUserId": issue_row.try_get::<Option<Uuid>, _>("assignee_user_id").unwrap_or(None),
        "responsibleUserId": issue_row.try_get::<Option<Uuid>, _>("responsible_user_id").unwrap_or(None),
        "identifier": issue_row.try_get::<Option<String>, _>("identifier").unwrap_or(None),
        "issueNumber": issue_row.try_get::<Option<i32>, _>("issue_number").unwrap_or(None),
        "createdAt": iso(issue_row.get::<DateTime<Utc>, _>("created_at")),
        "updatedAt": iso(issue_row.get::<DateTime<Utc>, _>("updated_at")),
    });

    let comment_rows = sqlx::query(
        "SELECT id, issue_id, body, actor_type::text AS actor_type, actor_id, actor_run_id, \
                metadata, created_at, updated_at \
           FROM issue_comments \
          WHERE company_id = $1 AND issue_id = $2 AND created_at <= $3 \
          ORDER BY created_at ASC, id ASC",
    )
    .bind(company_id)
    .bind(issue_id)
    .bind(decision.cutoff_at)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let comments: Vec<Value> = comment_rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "issueId": r.get::<Uuid, _>("issue_id"),
                "body": r.try_get::<Option<String>, _>("body").unwrap_or(None),
                "actorType": r.try_get::<Option<String>, _>("actor_type").unwrap_or(None),
                "actorId": r.try_get::<Option<Uuid>, _>("actor_id").unwrap_or(None),
                "actorRunId": r.try_get::<Option<Uuid>, _>("actor_run_id").unwrap_or(None),
                "metadata": r.try_get::<Option<Value>, _>("metadata").unwrap_or(None),
                "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
                "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
            })
        })
        .collect();

    let run_rows = sqlx::query(
        "SELECT id, agent_id, status::text AS status, invocation_source, context_snapshot, \
                error, started_at, finished_at, created_at, updated_at \
           FROM heartbeat_runs \
          WHERE company_id = $1 AND context_snapshot->>'issueId' = $2 AND updated_at <= $3 \
          ORDER BY created_at ASC",
    )
    .bind(company_id)
    .bind(issue_id.to_string())
    .bind(decision.cutoff_at)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let runs: Vec<Value> = run_rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "agentId": r.get::<Uuid, _>("agent_id"),
                "status": r.get::<String, _>("status"),
                "invocationSource": r.try_get::<Option<String>, _>("invocation_source").unwrap_or(None),
                "contextSnapshot": r.try_get::<Option<Value>, _>("context_snapshot").unwrap_or(None),
                "error": r.try_get::<Option<String>, _>("error").unwrap_or(None),
                "startedAt": iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("started_at").unwrap_or(None)),
                "finishedAt": iso_opt(r.try_get::<Option<DateTime<Utc>>, _>("finished_at").unwrap_or(None)),
                "createdAt": iso(r.get::<DateTime<Utc>, _>("created_at")),
                "updatedAt": iso(r.get::<DateTime<Utc>, _>("updated_at")),
            })
        })
        .collect();

    // Best-effort commit resolution from run context snapshots.
    let exact_commit = decision.exact_run_id.and_then(|run_id| {
        run_rows
            .iter()
            .find(|r| r.get::<Uuid, _>("id") == run_id)
            .and_then(|r| {
                find_commit_sha(
                    &r.try_get::<Option<Value>, _>("context_snapshot")
                        .unwrap_or(None),
                )
            })
    });
    let nearest_commit = run_rows.iter().rev().find_map(|r| {
        find_commit_sha(
            &r.try_get::<Option<Value>, _>("context_snapshot")
                .unwrap_or(None),
        )
    });
    let commit_sha = exact_commit.clone().or_else(|| nearest_commit.clone());
    let resolution = if exact_commit.is_some() {
        "exact"
    } else if nearest_commit.is_some() {
        "nearest_run"
    } else {
        "none"
    };

    Ok(CapturedSnapshot {
        cutoff_at: decision.cutoff_at,
        decision_outcome: decision.outcome.clone(),
        snapshot: json!({
            "version": 1,
            "retention": {
                "policy": DECISION_TRAINING_RETENTION_POLICY,
                "commentDeletion": "redact",
                "issueDeletion": "cascade",
            },
            "capturedAt": iso(captured_at),
            "cutoff": {
                "at": iso(decision.cutoff_at),
                "lastCommentId": comments.last().and_then(|c| c.get("id").cloned()).unwrap_or(Value::Null),
                "commentCount": comments.len(),
            },
            "issue": issue,
            "comments": comments,
            "runs": runs,
            "decision": {
                "kind": source_kind,
                "payload": decision.payload,
                "actor": decision.actor,
                "outcome": decision.outcome,
            },
            "code": {
                // Parrot does not model project/execution workspaces on this path.
                "repoUrl": Value::Null,
                "ref": Value::Null,
                "commitSha": commit_sha,
                "resolution": resolution,
            },
        }),
    })
}

/// Paperclip `findCommitSha()` — recursive scan for a commit-sha shaped value.
fn find_commit_sha(value: &Option<Value>) -> Option<String> {
    fn scan(value: &Value) -> Option<String> {
        match value {
            Value::Object(map) => {
                for key in ["commitSha", "commit_sha", "commit"] {
                    if let Some(sha) = map.get(key).and_then(|v| v.as_str()) {
                        if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                            return Some(sha.to_string());
                        }
                    }
                }
                map.values().find_map(scan)
            }
            Value::Array(items) => items.iter().find_map(scan),
            _ => None,
        }
    }
    value.as_ref().and_then(scan)
}

#[derive(Debug, Deserialize)]
pub struct CreateDecisionTrainingInput {
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
    #[serde(rename = "sourceId")]
    pub source_id: Uuid,
    #[serde(rename = "issueId")]
    pub issue_id: Uuid,
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "qualityScore")]
    pub quality_score: Option<f32>,
}

fn normalize_training_tags(tags: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.len() > 64 {
            return Err(AppError::Validation("training tag is too long".to_string()));
        }
        if !normalized.iter().any(|existing| existing == tag) {
            normalized.push(tag.to_string());
        }
    }
    normalized.sort();
    Ok(normalized)
}

fn validate_training_quality(score: Option<f32>) -> Result<(), AppError> {
    if score.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(AppError::Validation(
            "qualityScore must be between 0 and 1".to_string(),
        ));
    }
    Ok(())
}

fn require_human(actor: &AuthorizationActor) -> Result<String, AppError> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(user_id.to_string()),
        _ => Err(forbid("A human board session is required")),
    }
}

/// POST /companies/:company_id/decision-training
async fn create_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateDecisionTrainingInput>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    require_company_access(&actor, company_id, false)?;
    let user_id = require_human(&actor)?;
    if !TRAINING_SOURCE_KINDS.contains(&input.source_kind.as_str()) {
        return Err(AppError::Validation(format!(
            "sourceKind must be one of {TRAINING_SOURCE_KINDS:?}"
        )));
    }

    let pool = &state.pool;
    let captured = capture_decision_snapshot(
        pool,
        company_id,
        &input.source_kind,
        input.source_id,
        input.issue_id,
    )
    .await?;
    let notes = input.notes.unwrap_or_default();
    let tags = normalize_training_tags(input.tags.unwrap_or_default())?;
    validate_training_quality(input.quality_score)?;

    let example_id = state
        .decision_training_service
        .persist_snapshot(PersistSnapshotInput {
            company_id,
            source_kind: training_source_kind(&input.source_kind)?,
            source_id: input.source_id,
            issue_id: input.issue_id,
            cutoff_at: captured.cutoff_at,
            notes,
            tags,
            quality_score: input.quality_score,
            decision_outcome: captured.decision_outcome.clone(),
            retention_policy: DECISION_TRAINING_RETENTION_POLICY.to_string(),
            snapshot: captured.snapshot,
            created_by_user_id: user_id,
        })
        .await
        .map_err(|error| match error {
            TrainingError::InvalidSnapshot(message)
                if message == "duplicate decision training example" => {
                    AppError::Conflict("This decision is already trained by this user".to_string())
                }
            other => training_service_err(other),
        })?;
    let row = sqlx::query(&format!("{TRAINING_SELECT} WHERE id = $1"))
        .bind(example_id)
        .fetch_one(pool)
        .await
        .map_err(db_err)?;
    let example = training_example_to_json(&row);

    log_activity(
        pool,
        company_id,
        &actor,
        "decision_training.created",
        "decision_training_example",
        row.get::<Uuid, _>("id"),
        json!({
            "sourceKind": input.source_kind,
            "sourceId": input.source_id,
            "issueId": input.issue_id,
        }),
    )
    .await;

    Ok((StatusCode::CREATED, Json(example)))
}

#[derive(Debug, Deserialize)]
pub struct PreviewDecisionTrainingInput {
    #[serde(rename = "sourceKind")]
    pub source_kind: String,
    #[serde(rename = "sourceId")]
    pub source_id: Uuid,
    #[serde(rename = "issueId")]
    pub issue_id: Uuid,
}

/// POST /companies/:company_id/decision-training/preview
///
/// Read-only snapshot preview for the create drawer. Same authz as a write
/// (humans only) because it exposes the same captured decision state, but it
/// never persists anything.
async fn preview_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<PreviewDecisionTrainingInput>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, false)?;
    require_human(&actor)?;
    if !TRAINING_SOURCE_KINDS.contains(&input.source_kind.as_str()) {
        return Err(AppError::Validation(format!(
            "sourceKind must be one of {TRAINING_SOURCE_KINDS:?}"
        )));
    }

    let captured = capture_decision_snapshot(
        &state.pool,
        company_id,
        &input.source_kind,
        input.source_id,
        input.issue_id,
    )
    .await?;

    Ok(Json(json!({
        "cutoffAt": iso(captured.cutoff_at),
        "decisionOutcome": captured.decision_outcome,
        "snapshot": captured.snapshot,
    })))
}

#[derive(Debug, Default, Deserialize)]
pub struct TrainingListQuery {
    pub project: Option<Uuid>,
    pub kind: Option<String>,
    pub author: Option<String>,
    pub q: Option<String>,
}

const TRAINING_LIST_SELECT: &str = "SELECT e.id, e.company_id, e.source_kind, e.source_id, \
     e.issue_id, e.cutoff_at, e.notes, e.notes_history, e.tags, e.quality_score, e.decision_outcome, e.retention_policy, \
     e.snapshot, e.created_by_user_id, e.created_at, e.updated_at, \
     i.title AS issue_title, i.identifier AS issue_identifier \
       FROM decision_training_examples e \
       JOIN issues i ON i.id = e.issue_id \
      WHERE e.company_id = $1 \
        AND ($2::uuid IS NULL OR i.project_id = $2) \
        AND ($3::text IS NULL OR e.source_kind = $3) \
        AND ($4::text IS NULL OR e.created_by_user_id = $4) \
        AND ($5::text IS NULL OR e.notes ILIKE $5 OR i.title ILIKE $5 OR i.identifier ILIKE $5) \
      ORDER BY e.created_at DESC, e.id DESC";

/// Paperclip `decisionTrainingService.list()` — examples joined to their issue.
async fn load_training_rows(
    pool: &PgPool,
    company_id: Uuid,
    filters: &TrainingListQuery,
) -> Result<Vec<Value>, AppError> {
    let pattern = filters
        .q
        .as_ref()
        .map(|q| q.trim())
        .filter(|q| !q.is_empty())
        .map(|q| format!("%{q}%"));

    let rows = sqlx::query(TRAINING_LIST_SELECT)
        .bind(company_id)
        .bind(filters.project)
        .bind(filters.kind.as_deref())
        .bind(filters.author.as_deref())
        .bind(pattern.as_deref())
        .fetch_all(pool)
        .await
        .map_err(db_err)?;

    Ok(rows
        .iter()
        .map(|r| {
            json!({
                "example": training_example_to_json(r),
                "issueTitle": r.get::<String, _>("issue_title"),
                "issueIdentifier": r.try_get::<Option<String>, _>("issue_identifier").unwrap_or(None),
            })
        })
        .collect())
}

/// GET /companies/:company_id/decision-training
async fn list_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<TrainingListQuery>,
) -> Result<Json<Value>, AppError> {
    require_board(&actor)?;
    require_company_access(&actor, company_id, true)?;

    if let Some(kind) = query.kind.as_deref() {
        if !TRAINING_SOURCE_KINDS.contains(&kind) {
            return Err(AppError::BadRequest(
                "Invalid decision training query".to_string(),
            ));
        }
    }
    if query.q.as_ref().is_some_and(|q| q.trim().len() > 500) {
        return Err(AppError::BadRequest(
            "Invalid decision training query".to_string(),
        ));
    }

    let rows = load_training_rows(&state.pool, company_id, &query).await?;
    Ok(Json(Value::Array(rows)))
}

/// GET /companies/:company_id/decision-training/export.jsonl
async fn export_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<axum::response::Response, AppError> {
    require_board(&actor)?;
    require_company_access(&actor, company_id, true)?;

    let pool = &state.pool;
    let rows = load_training_rows(pool, company_id, &TrainingListQuery::default()).await?;

    let lines: Vec<String> = rows
        .iter()
        .filter_map(|row| row.get("example"))
        .map(|example| {
            json!({
                "retentionPolicy": example.get("retentionPolicy").cloned().unwrap_or(Value::Null),
                "state": example.get("snapshot").cloned().unwrap_or(Value::Null),
                "label": {
                    "outcome": example.get("decisionOutcome").cloned().unwrap_or(Value::Null),
                    "notes": example.get("notes").cloned().unwrap_or(Value::Null),
                },
            })
            .to_string()
        })
        .collect();
    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };

    let example_ids: Vec<Value> = rows
        .iter()
        .filter_map(|row| row.get("example").and_then(|e| e.get("id")).cloned())
        .collect();
    log_activity(
        pool,
        company_id,
        &actor,
        "decision_training.exported",
        "decision_training_export",
        company_id,
        json!({ "exampleCount": rows.len(), "exampleIds": example_ids }),
    )
    .await;

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/x-ndjson; charset=utf-8",
        )],
        body,
    )
        .into_response())
}

fn training_not_found() -> AppError {
    AppError::NotFound("Decision training example not found".to_string())
}

/// Loads an example and enforces that the caller can see its company.
async fn load_training_example(
    pool: &PgPool,
    actor: &AuthorizationActor,
    example_id: Uuid,
) -> Result<PgRow, AppError> {
    let row = sqlx::query(&format!("{TRAINING_SELECT} WHERE id = $1"))
        .bind(example_id)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?
        .ok_or_else(training_not_found)?;

    let company_id: Uuid = row.get("company_id");
    // Paperclip masks cross-company reads as 404 rather than 403.
    if crate::routes::assert_company_access(actor, company_id, true).is_err() {
        return Err(training_not_found());
    }
    Ok(row)
}

fn require_example_owner(user_id: &str, created_by_user_id: &str) -> Result<(), AppError> {
    if user_id != created_by_user_id {
        return Err(forbid(
            "Only the example author can change decision training examples",
        ));
    }
    Ok(())
}

/// GET /decision-training/:example_id
async fn get_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(example_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_board(&actor)?;
    let pool = &state.pool;
    let service_example = state
        .decision_training_service
        .get_example(example_id)
        .await
        .map_err(training_service_err)?
        .ok_or_else(training_not_found)?;
    if crate::routes::assert_company_access(&actor, service_example.company_id, true).is_err() {
        return Err(training_not_found());
    }
    let example = training_service_example_to_json(&service_example);

    log_activity(
        pool,
        service_example.company_id,
        &actor,
        "decision_training.read",
        "decision_training_example",
        service_example.id,
        json!({
            "sourceKind": example.get("sourceKind"),
            "sourceId": example.get("sourceId"),
            "issueId": example.get("issueId"),
        }),
    )
    .await;

    Ok(Json(example))
}

#[derive(Debug, Deserialize)]
pub struct UpdateDecisionTrainingInput {
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "qualityScore")]
    pub quality_score: Option<f32>,
}

/// PATCH /decision-training/:example_id
async fn update_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(example_id): Path<Uuid>,
    Json(input): Json<UpdateDecisionTrainingInput>,
) -> Result<Json<Value>, AppError> {
    let pool = &state.pool;
    let existing = load_training_example(pool, &actor, example_id).await?;
    let user_id = require_human(&actor)?;
    let created_by: String = existing.get("created_by_user_id");
    require_example_owner(&user_id, &created_by)?;
    let tags = match input.tags {
        Some(tags) => Some(normalize_training_tags(tags)?),
        None => None,
    };
    validate_training_quality(input.quality_score)?;

    let previous_notes: String = existing.get("notes");
    let notes = input.notes.unwrap_or_else(|| previous_notes.clone());
    if notes == previous_notes && tags.is_none() && input.quality_score.is_none() {
        return Ok(Json(training_example_to_json(&existing)));
    }

    if notes.len() > 100_000 {
        return Err(AppError::Validation(
            "notes must be at most 100000 characters".to_string(),
        ));
    }

    let mut history = existing
        .try_get::<Value, _>("notes_history")
        .unwrap_or_else(|_| json!([]));
    if !history.is_array() {
        history = json!([]);
    }
    if notes != previous_notes {
        if let Some(entries) = history.as_array_mut() {
            entries.push(json!({
                "author": user_id,
                "at": iso(Utc::now()),
                "body": previous_notes,
            }));
        }
    }

    let updated = sqlx::query(&format!(
        "UPDATE decision_training_examples \
            SET notes = $1, notes_history = $2, \
                tags = COALESCE($3, tags), quality_score = COALESCE($4, quality_score), updated_at = NOW() \
          WHERE id = $5 \
      RETURNING {}",
        TRAINING_SELECT
            .trim_start_matches("SELECT ")
            .replace(" FROM decision_training_examples", "")
    ))
    .bind(&notes)
    .bind(&history)
    .bind(tags.as_ref().map(|value| serde_json::to_value(value)).transpose().map_err(|error| AppError::InternalServerError(error.to_string()))?)
    .bind(input.quality_score)
    .bind(example_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or_else(training_not_found)?;

    log_activity(
        pool,
        updated.get::<Uuid, _>("company_id"),
        &actor,
        "decision_training.notes_updated",
        "decision_training_example",
        updated.get::<Uuid, _>("id"),
        json!({ "issueId": updated.get::<Uuid, _>("issue_id") }),
    )
    .await;

    Ok(Json(training_example_to_json(&updated)))
}

/// DELETE /decision-training/:example_id
async fn delete_decision_training(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(example_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let pool = &state.pool;
    let existing = state
        .decision_training_service
        .get_example(example_id)
        .await
        .map_err(training_service_err)?
        .ok_or_else(training_not_found)?;
    if crate::routes::assert_company_access(&actor, existing.company_id, true).is_err() {
        return Err(training_not_found());
    }
    let user_id = require_human(&actor)?;
    require_example_owner(&user_id, &existing.created_by_user_id)?;

    if !state
        .decision_training_service
        .delete_example(example_id)
        .await
        .map_err(training_service_err)?
    {
        return Err(training_not_found());
    }

    log_activity(
        pool,
        existing.company_id,
        &actor,
        "decision_training.deleted",
        "decision_training_example",
        example_id,
        json!({
            "issueId": existing.snapshot.issue_id,
            "deletedByUserId": user_id,
        }),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_router_constructs() {
        let _router = decision_routes();
    }

    #[test]
    fn decision_queue_seeds_expose_three_rules() {
        let seeds = decision_queue_seeds();
        let items = seeds.as_array().expect("seeds are an array");
        assert_eq!(items.len(), 3);
        let keys: Vec<&str> = items
            .iter()
            .filter_map(|s| s.get("key").and_then(|k| k.as_str()))
            .collect();
        assert!(keys.contains(&"prs"));
        assert!(keys.contains(&"plans"));
        assert!(keys.contains(&"questions"));
    }

    #[test]
    fn queue_keys_are_validated() {
        assert!(validate_queue_key("my-queue").is_ok());
        assert!(validate_queue_key("").is_err());
        assert!(validate_queue_key("Bad Key").is_err());
    }

    #[test]
    fn attention_source_kinds_are_recognised() {
        assert!(is_valid_source_kind("approval"));
        assert!(!is_valid_source_kind("nope"));
    }

    #[test]
    fn training_tags_are_normalized_and_quality_is_bounded() {
        assert_eq!(
            normalize_training_tags(vec![
                "  urgent ".to_string(),
                "urgent".to_string(),
                "".to_string(),
                "review".to_string(),
            ])
            .unwrap(),
            vec!["review".to_string(), "urgent".to_string()]
        );
        assert!(validate_training_quality(Some(0.0)).is_ok());
        assert!(validate_training_quality(Some(1.0)).is_ok());
        assert!(validate_training_quality(Some(1.01)).is_err());
        assert!(validate_training_quality(Some(f32::NAN)).is_err());
    }

    #[test]
    fn attention_activity_sort_is_deterministic() {
        let mut items = vec![
            json!({"activityAt": "2026-01-01T00:00:00Z", "severity": "low", "sourceKind": "approval", "dedupKey": "z"}),
            json!({"activityAt": "2026-01-01T00:00:00Z", "severity": "critical", "sourceKind": "approval", "dedupKey": "a"}),
            json!({"activityAt": "2026-01-02T00:00:00Z", "severity": "low", "sourceKind": "approval", "dedupKey": "new"}),
        ];
        items.sort_by(compare_attention_items);
        assert_eq!(items[0]["dedupKey"], "new");
        assert_eq!(items[1]["dedupKey"], "a");
        assert_eq!(items[2]["dedupKey"], "z");
    }

    #[test]
    fn attention_decide_sort_prefers_decision_ready_items() {
        let now = Utc::now();
        let mut items = vec![
            json!({"activityAt": "2026-01-01T00:00:00Z", "severity": "critical", "sourceKind": "approval", "dedupKey": "whenever", "decideBy": "whenever"}),
            json!({"activityAt": "2026-01-02T00:00:00Z", "severity": "low", "sourceKind": "approval", "dedupKey": "today", "decideBy": "today"}),
        ];
        items.sort_by(|left, right| compare_decide_items(left, right, now));
        assert_eq!(items[0]["dedupKey"], "today");
    }

    #[test]
    fn cursor_round_trips() {
        let item = json!({
            "id": "approval:11111111-1111-1111-1111-111111111111",
            "sourceKind": "approval",
            "sourceId": "11111111-1111-1111-1111-111111111111",
            "createdAt": "2026-01-01T00:00:00.000Z",
            "lastActivityAt": "2026-01-01T00:00:00.000Z",
        });
        let cursor = encode_cursor("recent", &item);
        assert_eq!(
            decode_cursor(&cursor, "recent").unwrap(),
            "approval:11111111-1111-1111-1111-111111111111"
        );
        // A cursor minted for one sort order is not valid for another.
        assert!(decode_cursor(&cursor, "decide").is_err());
        assert!(decode_cursor("!!!not-base64!!!", "recent").is_err());
    }

    #[test]
    fn commit_sha_is_found_recursively() {
        let snapshot = json!({ "nested": { "commitSha": "abcdef1234567890" } });
        assert_eq!(
            find_commit_sha(&Some(snapshot)),
            Some("abcdef1234567890".to_string())
        );
        assert_eq!(find_commit_sha(&None), None);
    }
}
