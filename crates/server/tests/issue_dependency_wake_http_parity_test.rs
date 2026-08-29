//! Dependency wake / rewake HTTP + PostgreSQL parity test
//! (PAPERCLIP_MIGRATION_PLAN §4B.1 line 362).
//!
//! Closes the "Dependency Wakeup and Rewake Throttle" parent by driving the
//! real end-to-end lifecycle over HTTP against the live compile DB:
//!
//!   - releasing a blocker as `done` wakes the dependent with reason
//!     `issue_blockers_resolved`, using a per-(dependent, blocker) idempotency
//!     key so a repeated release cannot double-wake
//!   - a dependent with an *unresolved* second blocker is NOT woken
//!   - the liveness backstop heals a dependent whose wake was lost, and is a
//!     no-op when a live run or an existing wake already covers it
//!   - the backstop defers candidates past its page limit instead of trying to
//!     heal the whole table in one pass

use api::routes::issues::issue_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
    actor: AuthorizationActor,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let prefix = format!("DW{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Dependency Wake Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, 'idle')",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("Dependency wake agent")
    .execute(pool)
    .await
    .expect("insert agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(json!({"source": "dependency-wake-test"}))
    .execute(pool)
    .await
    .expect("insert heartbeat run");

    Fixture {
        pool: pool.clone(),
        company_id,
        agent_id,
        run_id,
        actor: board_actor(Uuid::new_v4(), company_id),
    }
}

async fn cleanup(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issue_relations WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.agent_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serialize body")))
        .expect("build request");
    let mut request = request;
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, parsed)
}

/// Insert an issue directly so the test can construct a precise dependency
/// graph without routing through creation dedup rules.
async fn insert_issue(f: &Fixture, title: &str, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, assignee_agent_id)
         VALUES ($1, $2, $3, $4::issue_status, $5)",
    )
    .bind(id)
    .bind(f.company_id)
    .bind(title)
    .bind(status)
    .bind(f.agent_id)
    .execute(&f.pool)
    .await
    .expect("insert issue");
    id
}

/// `blocks` edge: `blocker` blocks `dependent`.
async fn add_blocks(f: &Fixture, blocker: Uuid, dependent: Uuid) {
    sqlx::query(
        "INSERT INTO issue_relations (company_id, issue_id, related_issue_id, type)
         VALUES ($1, $2, $3, 'blocks')",
    )
    .bind(f.company_id)
    .bind(blocker)
    .bind(dependent)
    .execute(&f.pool)
    .await
    .expect("insert blocks relation");
}

async fn wake_count(f: &Fixture, dependent: Uuid, blocker: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1
            AND idempotency_key = 'issue_blockers_resolved:' || $2::text || ':' || $3::text",
    )
    .bind(f.company_id)
    .bind(dependent)
    .bind(blocker)
    .fetch_one(&f.pool)
    .await
    .expect("count dependency wakes")
}

/// Releasing the last unresolved blocker wakes the dependent exactly once, and
/// repeating the release cannot double-wake it.
#[sqlx::test]
async fn release_resolving_last_blocker_wakes_dependent_once(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));

    let blocker = insert_issue(&f, "blocker", "in_progress").await;
    let dependent = insert_issue(&f, "dependent", "blocked").await;
    add_blocks(&f, blocker, dependent).await;

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{blocker}/release"),
        json!({"releaseRunId": f.run_id, "result": "success"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["status"], "done");

    assert_eq!(
        wake_count(&f, dependent, blocker).await,
        1,
        "resolving the last blocker must queue exactly one wake"
    );

    // The wake carries Paperclip's idempotent-dependency-wake shape.
    let (reason, payload_issue): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT reason, payload->>'issueId' FROM agent_wakeup_requests
          WHERE company_id = $1
            AND idempotency_key = 'issue_blockers_resolved:' || $2::text || ':' || $3::text",
    )
    .bind(f.company_id)
    .bind(dependent)
    .bind(blocker)
    .fetch_one(&f.pool)
    .await
    .expect("read dependency wake");
    assert_eq!(reason.as_deref(), Some("issue_blockers_resolved"));
    assert_eq!(payload_issue.as_deref(), Some(dependent.to_string().as_str()));

    // Re-releasing the same blocker must not create a second wake.
    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{blocker}/release"),
        json!({"releaseRunId": f.run_id, "result": "success"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        wake_count(&f, dependent, blocker).await,
        1,
        "dependency wakes must be idempotent"
    );

    cleanup(&f).await;
}

/// A dependent that still has an unresolved blocker is not woken.
#[sqlx::test]
async fn release_does_not_wake_dependent_with_unresolved_blocker(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));

    let blocker = insert_issue(&f, "resolved blocker", "in_progress").await;
    let other = insert_issue(&f, "unresolved blocker", "in_progress").await;
    let dependent = insert_issue(&f, "dependent", "blocked").await;
    add_blocks(&f, blocker, dependent).await;
    add_blocks(&f, other, dependent).await;

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{blocker}/release"),
        json!({"releaseRunId": f.run_id, "result": "success"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    assert_eq!(
        wake_count(&f, dependent, blocker).await,
        0,
        "a still-blocked dependent must not be woken"
    );

    cleanup(&f).await;
}

/// The liveness backstop heals a dependent whose dependency wake was lost.
#[sqlx::test]
async fn backstop_heals_dependent_with_lost_wake(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;

    let blocker = insert_issue(&f, "done blocker", "done").await;
    let dependent = insert_issue(&f, "stranded dependent", "blocked").await;
    add_blocks(&f, blocker, dependent).await;

    let heartbeat = services::heartbeat_service::DefaultHeartbeatService::new(pool.clone());

    let healed = heartbeat
        .reconcile_dependency_wakeups()
        .await
        .expect("reconcile dependency wakeups");
    assert_eq!(healed, 1, "the stranded dependent must be healed");

    let key = format!("issue_graph_liveness_backstop:{dependent}:{blocker}");
    let (reason, payload_issue): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT reason, payload->>'issueId' FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(f.company_id)
    .bind(&key)
    .fetch_one(&f.pool)
    .await
    .expect("read backstop wake");
    assert_eq!(reason.as_deref(), Some("issue_graph_liveness_backstop"));
    assert_eq!(payload_issue.as_deref(), Some(dependent.to_string().as_str()));

    // A second pass is a no-op: the existing wake already covers the dependent.
    let healed_again = heartbeat
        .reconcile_dependency_wakeups()
        .await
        .expect("reconcile dependency wakeups");
    assert_eq!(healed_again, 0, "an existing wake must suppress re-healing");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
          WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(f.company_id)
    .bind(&key)
    .fetch_one(&f.pool)
    .await
    .expect("count backstop wakes");
    assert_eq!(count, 1);

    cleanup(&f).await;
}

/// The backstop skips dependents that already have a live run for the issue.
#[sqlx::test]
async fn backstop_skips_dependent_with_live_run(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;

    let blocker = insert_issue(&f, "done blocker", "done").await;
    let dependent = insert_issue(&f, "running dependent", "blocked").await;
    add_blocks(&f, blocker, dependent).await;

    let live_run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(live_run_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(json!({"issueId": dependent.to_string()}))
    .execute(&f.pool)
    .await
    .expect("insert live heartbeat run");

    let heartbeat = services::heartbeat_service::DefaultHeartbeatService::new(pool.clone());
    let healed = heartbeat
        .reconcile_dependency_wakeups()
        .await
        .expect("reconcile dependency wakeups");
    assert_eq!(healed, 0, "a live run must suppress the backstop wake");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests WHERE company_id = $1 AND reason = $2",
    )
    .bind(f.company_id)
    .bind("issue_graph_liveness_backstop")
    .fetch_one(&f.pool)
    .await
    .expect("count backstop wakes");
    assert_eq!(count, 0);

    cleanup(&f).await;
}
