//! Paperclip canonical recovery-action schema and HTTP contract coverage.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use api::routes::issues::issue_routes;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor};
use services::job_scheduler::{RecoveryActionRetryJob, ScheduledJob};
use sqlx::{PgPool, Row};
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    issue_id: Uuid,
    action_id: Uuid,
    board_user_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let action_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    let prefix = format!("RA{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Recovery action parity")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status)
         VALUES ($1, $2, 'Recovery action issue', 'blocked')",
    )
    .bind(issue_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert issue");
    sqlx::query(
        "INSERT INTO issue_recovery_actions
         (id, company_id, source_issue_id, kind, status, owner_type, cause,
          fingerprint, evidence, next_action, attempt_count)
         VALUES ($1, $2, $3, 'issue_graph_liveness', 'active', 'system',
                 'blocked issue lost its recovery wake', 'fp-1', '{\"source\":\"test\"}',
                 'reconcile dependencies', 0)",
    )
    .bind(action_id)
    .bind(company_id)
    .bind(issue_id)
    .execute(pool)
    .await
    .expect("insert recovery action");

    Fixture {
        pool: pool.clone(),
        company_id,
        issue_id,
        action_id,
        board_user_id,
    }
}

fn board_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.board_user_id, fixture.company_id)
}

async fn app(pool: PgPool) -> Router {
    issue_routes().with_state(build_app_state(pool).await.expect("build app state"))
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request_body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).expect("serialize body"))
        }
        None => Body::empty(),
    };
    let mut request = builder.body(request_body).expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app.clone().oneshot(request).await.expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response must be JSON")
    };
    (status, value)
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

#[sqlx::test]
async fn list_returns_paperclip_active_actions_projection_and_resolve_is_scoped(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let app = app(pool.clone()).await;
    let actor = board_actor(&fixture);
    let uri = format!("/issues/{}/recovery-actions", fixture.issue_id);

    let (status, body) = send(&app, &actor, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"]["id"], fixture.action_id.to_string());
    assert_eq!(body["active"]["sourceIssueId"], fixture.issue_id.to_string());
    assert_eq!(body["active"]["kind"], "issue_graph_liveness");
    assert_eq!(body["active"]["status"], "active");
    assert_eq!(body["actions"].as_array().expect("actions array").len(), 1);

    let (status, _) = send(
        &app,
        &AuthorizationActor::board_with_source(
            Uuid::new_v4(),
            Uuid::new_v4(),
            ActorSource::Session,
            Vec::new(),
            false,
        ),
        "GET",
        &uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let blocker_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status)
         VALUES ($1, $2, 'Unresolved blocker', 'todo')",
    )
    .bind(blocker_id)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("insert blocker issue");
    sqlx::query(
        "INSERT INTO issue_relations (company_id, issue_id, related_issue_id, type)
         VALUES ($1, $2, $3, 'blocks')",
    )
    .bind(fixture.company_id)
    .bind(blocker_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("insert blocker relation");

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("{uri}/resolve"),
        Some(json!({
            "actionId": fixture.action_id,
            "outcome": "blocked",
            "sourceIssueStatus": "blocked",
            "resolutionNote": "Dependency recovery remains blocked"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issue"]["id"], fixture.issue_id.to_string());
    assert_eq!(body["issue"]["status"], "blocked");
    assert!(body["issue"]["activeRecoveryAction"].is_null());
    assert_eq!(body["recoveryAction"]["id"], fixture.action_id.to_string());
    assert_eq!(body["recoveryAction"]["outcome"], "blocked");

    let row = sqlx::query(
        "SELECT status, outcome, resolution_note, resolved_at
           FROM issue_recovery_actions WHERE id = $1 AND company_id = $2",
    )
    .bind(fixture.action_id)
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("load resolved action");
    assert_eq!(row.get::<String, _>("status"), "resolved");
    assert_eq!(row.get::<String, _>("outcome"), "blocked");
    assert_eq!(
        row.get::<Option<String>, _>("resolution_note").as_deref(),
        Some("Dependency recovery remains blocked")
    );
    assert!(row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at").is_some());

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn blocked_resolution_without_unresolved_blocker_is_atomic(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let app = app(pool.clone()).await;

    let (status, _) = send(
        &app,
        &board_actor(&fixture),
        "POST",
        &format!("/issues/{}/recovery-actions/resolve", fixture.issue_id),
        Some(json!({
            "actionId": fixture.action_id,
            "outcome": "blocked",
            "sourceIssueStatus": "blocked",
            "resolutionNote": "No blocker exists"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let row = sqlx::query(
        "SELECT i.status::text AS issue_status, r.status, r.outcome
           FROM issues i
           JOIN issue_recovery_actions r ON r.source_issue_id = i.id
          WHERE i.id = $1 AND r.id = $2",
    )
    .bind(fixture.issue_id)
    .bind(fixture.action_id)
    .fetch_one(&pool)
    .await
    .expect("load unchanged recovery state");
    assert_eq!(row.get::<String, _>("issue_status"), "blocked");
    assert_eq!(row.get::<String, _>("status"), "active");
    assert!(row.get::<Option<String>, _>("outcome").is_none());
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn agent_owner_resolution_hands_back_and_queues_one_wakeup(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let app = app(pool.clone()).await;
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status)
         VALUES ($1, $2, 'Recovery owner', 'idle')",
    )
    .bind(agent_id)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("insert recovery owner");
    sqlx::query("UPDATE issues SET assignee_agent_id = $2 WHERE id = $1")
        .bind(fixture.issue_id)
        .bind(agent_id)
        .execute(&pool)
        .await
        .expect("assign issue owner");
    sqlx::query(
        "UPDATE issue_recovery_actions
            SET owner_type = 'agent', owner_agent_id = $2, return_owner_agent_id = $2
          WHERE id = $1",
    )
    .bind(fixture.action_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("assign recovery owner");

    let (status, body) = send(
        &app,
        &AuthorizationActor::agent(agent_id, fixture.company_id, None),
        "POST",
        &format!("/issues/{}/recovery-actions/resolve", fixture.issue_id),
        Some(json!({
            "actionId": fixture.action_id,
            "outcome": "restored",
            "sourceIssueStatus": "todo",
            "resolutionNote": "Restored to the previous owner"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["issue"]["status"], "todo");
    assert_eq!(body["issue"]["assigneeAgentId"], agent_id.to_string());
    assert_eq!(body["recoveryAction"]["outcome"], "handed_back");

    let wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2
            AND payload->>'issueId' = $3
            AND reason = 'issue_recovery_action_restored'",
    )
    .bind(fixture.company_id)
    .bind(agent_id)
    .bind(fixture.issue_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count recovery wakeups");
    assert_eq!(wake_count, 1);
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn concurrent_resolve_of_one_action_has_one_winner(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let app = app(pool.clone()).await;
    let actor = board_actor(&fixture);
    let uri = format!("/issues/{}/recovery-actions/resolve", fixture.issue_id);
    let payload = json!({
        "actionId": fixture.action_id,
        "outcome": "restored",
        "sourceIssueStatus": "todo",
        "resolutionNote": "Concurrent restore"
    });

    let first = send(&app, &actor, "POST", &uri, Some(payload.clone()));
    let second = send(&app, &actor, "POST", &uri, Some(payload));
    let (first, second) = tokio::join!(first, second);
    let statuses = [first.0, second.0];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::NOT_FOUND));

    let row = sqlx::query(
        "SELECT i.status::text AS issue_status, r.status, r.outcome
           FROM issues i
           JOIN issue_recovery_actions r ON r.source_issue_id = i.id
          WHERE i.id = $1 AND r.id = $2",
    )
    .bind(fixture.issue_id)
    .bind(fixture.action_id)
    .fetch_one(&pool)
    .await
    .expect("load concurrent recovery result");
    assert_eq!(row.get::<String, _>("issue_status"), "todo");
    assert_eq!(row.get::<String, _>("status"), "resolved");
    assert_eq!(row.get::<String, _>("outcome"), "restored");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn active_source_and_fingerprint_constraints_are_canonical(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;

    let source_conflict = sqlx::query(
        "INSERT INTO issue_recovery_actions
         (company_id, source_issue_id, kind, status, cause, fingerprint, next_action)
         VALUES ($1, $2, 'workspace_validation', 'active', 'second', 'fp-2', 'retry')",
    )
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await;
    assert_eq!(source_conflict.expect_err("active source must be unique").as_database_error().and_then(|e| e.constraint()), Some("issue_recovery_actions_active_source_uq"));

    sqlx::query("UPDATE issue_recovery_actions SET status = 'cancelled' WHERE id = $1")
        .bind(fixture.action_id)
        .execute(&pool)
        .await
        .expect("cancel action");
    let second_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_recovery_actions
         (id, company_id, source_issue_id, kind, status, cause, fingerprint, next_action)
         VALUES ($1, $2, $3, 'workspace_validation', 'active', 'second', 'fp-2', 'retry')",
    )
    .bind(second_id)
    .bind(fixture.company_id)
    .bind(fixture.issue_id)
    .execute(&pool)
    .await
    .expect("insert after cancellation");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn recovery_retry_job_uses_attempt_count_and_escalated_status(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;

    sqlx::query("UPDATE issues SET status = 'todo' WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&pool)
        .await
        .expect("make recovery condition healthy");
    let result = RecoveryActionRetryJob::new(pool.clone())
        .execute()
        .await
        .expect("run recovery retry job");
    assert!(result.contains("resolved 1"));
    let status: String = sqlx::query_scalar(
        "SELECT status FROM issue_recovery_actions WHERE id = $1",
    )
    .bind(fixture.action_id)
    .fetch_one(&pool)
    .await
    .expect("load resolved recovery action");
    assert_eq!(status, "resolved");

    sqlx::query("UPDATE issues SET status = 'blocked' WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&pool)
        .await
        .expect("reset recovery issue");
    sqlx::query(
        "UPDATE issue_recovery_actions
            SET status = 'active', outcome = NULL, resolved_at = NULL,
                attempt_count = 0, max_attempts = 1, last_attempt_at = NULL,
                updated_at = NOW()
          WHERE id = $1",
    )
    .bind(fixture.action_id)
    .execute(&pool)
    .await
    .expect("reset recovery action");
    let before_retry = sqlx::query(
        "SELECT i.status::text AS issue_status, r.kind, r.status, r.max_attempts
           FROM issues i JOIN issue_recovery_actions r ON r.source_issue_id = i.id
          WHERE r.id = $1",
    )
    .bind(fixture.action_id)
    .fetch_one(&pool)
    .await
    .expect("inspect reset recovery action");
    assert_eq!(before_retry.get::<String, _>("issue_status"), "blocked");
    assert_eq!(before_retry.get::<String, _>("kind"), "issue_graph_liveness");
    assert_eq!(before_retry.get::<String, _>("status"), "active");
    assert_eq!(before_retry.get::<Option<i32>, _>("max_attempts"), Some(1));
    let result = RecoveryActionRetryJob::new(pool.clone())
        .execute()
        .await
        .expect("escalate exhausted recovery action");
    assert!(result.contains("escalated 1"), "result={result}");
    let row = sqlx::query(
        "SELECT status, outcome, attempt_count FROM issue_recovery_actions WHERE id = $1",
    )
    .bind(fixture.action_id)
    .fetch_one(&pool)
    .await
    .expect("load escalated recovery action");
    assert_eq!(row.get::<String, _>("status"), "escalated");
    assert_eq!(row.get::<String, _>("outcome"), "escalated");
    assert_eq!(row.get::<i32, _>("attempt_count"), 1);
    cleanup(&fixture).await;
}
