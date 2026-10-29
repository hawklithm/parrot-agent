//! HTTP + PostgreSQL coverage for task-driven team orchestration.
//!
//! These tests drive the real Axum routes against a real PostgreSQL schema and
//! a deterministic "fake agent": the agent identity is an authenticated-actor
//! extension, and every agent-side call is an ordinary HTTP request. No model
//! or adapter process is involved.
//!
//! Run with:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test task_orchestration_http_parity_test

use api::routes::{interactions::interaction_routes, issues::issue_routes};
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

mod common;
use common::migrate;

/// Start a scripted fake adapter endpoint. The HTTP adapter returns whatever
/// body this endpoint sends, which is enough to drive a real `execute_run`
/// without any model account.
async fn fake_adapter_server(status: StatusCode) -> std::net::SocketAddr {
    use axum::routing::post;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake adapter");
    let addr = listener.local_addr().expect("fake adapter addr");
    let app = Router::new().route(
        "/run",
        post(move || async move { (status, "{\"result\":\"fake adapter ok\"}") }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

async fn point_agents_at_http(server: std::net::SocketAddr, company_id: Uuid, pool: &PgPool) {
    sqlx::query(
        "UPDATE agents
            SET adapter_type = 'http',
                adapter_config = jsonb_build_object('url', $2, 'method', 'POST')
          WHERE company_id = $1",
    )
    .bind(company_id)
    .bind(format!("http://{server}/run"))
    .execute(pool)
    .await
    .expect("point agents at fake adapter");
}

async fn wait_for_no_live_run(pool: &PgPool, company_id: Uuid, issue_id: Uuid) {
    for _ in 0..80 {
        let live: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM heartbeat_runs
              WHERE company_id = $1
                AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)
                AND status IN ('queued', 'running')",
        )
        .bind(company_id)
        .bind(issue_id.to_string())
        .fetch_one(pool)
        .await
        .expect("count live runs");
        if live == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("a heartbeat run for {issue_id} never became terminal");
}

async fn wait_for_new_terminal_run(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
) -> (Uuid, String) {
    for _ in 0..120 {
        let run: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, status::text FROM heartbeat_runs
              WHERE company_id = $1
                AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)
                AND status IN ('succeeded', 'failed', 'cancelled')
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(company_id)
        .bind(issue_id.to_string())
        .fetch_optional(pool)
        .await
        .expect("read terminal run");
        if let Some(run) = run {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("no terminal heartbeat run for {issue_id}");
}

/// Wait until the issue has at least one terminal run *and* no live run. A
/// create-time assignment wake must finish before the test starts its own run,
/// otherwise the two starts contend on the per-agent start lock.
async fn wait_until_settled(pool: &PgPool, company_id: Uuid, issue_id: Uuid) {
    wait_for_new_terminal_run(pool, company_id, issue_id).await;
    wait_for_no_live_run(pool, company_id, issue_id).await;
}

/// Wait for a terminal run created after `after`. Used when the issue already
/// has an earlier terminal run (for example the create-time assignment run).
async fn wait_for_terminal_run_after(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
    after: chrono::DateTime<chrono::Utc>,
) -> (Uuid, String) {
    for _ in 0..120 {
        let run: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, status::text FROM heartbeat_runs
              WHERE company_id = $1
                AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)
                AND status IN ('succeeded', 'failed', 'cancelled')
                AND created_at > $3
              ORDER BY created_at DESC
              LIMIT 1",
        )
        .bind(company_id)
        .bind(issue_id.to_string())
        .bind(after)
        .fetch_optional(pool)
        .await
        .expect("read terminal run after marker");
        if let Some(run) = run {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("no terminal heartbeat run for {issue_id} after {after}");
}

struct Fixture {
    company_id: Uuid,
    board_user_id: Uuid,
    ceo_id: Uuid,
    lead_id: Uuid,
    worker_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents)
         VALUES ($1, $2, $3, FALSE)",
    )
    .bind(company_id)
    .bind("Orchestration parity")
    .bind(format!("OR{}", &company_id.simple().to_string()[..8]))
    .execute(pool)
    .await
    .expect("insert company");

    let ceo_id = seed_agent(pool, company_id, "CEO", "ceo").await;
    let lead_id = seed_agent(pool, company_id, "Lead", "manager").await;
    let worker_id = seed_agent(pool, company_id, "Worker", "general").await;
    sqlx::query("UPDATE agents SET reports_to = $1 WHERE id = $2")
        .bind(ceo_id)
        .bind(lead_id)
        .execute(pool)
        .await
        .expect("link lead to ceo");
    sqlx::query("UPDATE agents SET reports_to = $1 WHERE id = $2")
        .bind(lead_id)
        .bind(worker_id)
        .execute(pool)
        .await
        .expect("link worker to lead");

    Fixture {
        company_id,
        board_user_id,
        ceo_id,
        lead_id,
        worker_id,
    }
}

async fn seed_agent(pool: &PgPool, company_id: Uuid, name: &str, role: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents
            (id, company_id, name, role, status, adapter_type, adapter_config)
         VALUES ($1, $2, $3, $4, 'idle', 'process', '{}'::jsonb)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(name)
    .bind(role)
    .execute(pool)
    .await
    .expect("insert agent");
    agent_id
}

fn board_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.board_user_id, fixture.company_id)
}

fn agent_actor(fixture: &Fixture, agent_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::Agent {
        agent_id,
        company_id: fixture.company_id,
        run_id: None,
        source: ActorSource::AgentKey,
        key_id: None,
        key_scope: None,
        responsible_user_id: None,
        on_behalf_of_user_id: None,
        on_behalf_of_memberships: vec![],
    }
}

async fn build_router(pool: &PgPool) -> (Router, api::app_state::AppState) {
    let state = build_app_state(pool.clone())
        .await
        .expect("build app state");
    let router = Router::new()
        .merge(issue_routes())
        .merge(interaction_routes())
        .merge(api::routes::approvals::approval_routes())
        .merge(api::routes::task_orchestration::task_orchestration_routes())
        .with_state(state.clone());
    (router, state)
}

fn orchestration_service(state: &api::app_state::AppState) -> services::TaskOrchestrationService {
    services::TaskOrchestrationService::new(
        state.pool.clone(),
        state.heartbeat_service.clone(),
        state.agent_service.clone(),
        state.approval_service.clone(),
        state.issue_service.clone(),
    )
}

async fn send(
    app: &Router,
    method: Method,
    uri: String,
    actor: &AuthorizationActor,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&body).expect("serialize request"),
        ))
        .expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::Null)
    };
    (status, value)
}

fn issue_id(value: &Value) -> Uuid {
    value["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .or_else(|| {
            value["issue"]["id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok())
        })
        .expect("issue id")
}

fn orchestration_uri(fixture: &Fixture, issue_id: Uuid, suffix: &str) -> String {
    format!(
        "/companies/{}/issues/{}/orchestration{}",
        fixture.company_id, issue_id, suffix
    )
}

async fn orchestration_view(app: &Router, fixture: &Fixture, issue_id: Uuid) -> Value {
    let (status, body) = send(
        app,
        Method::GET,
        orchestration_uri(fixture, issue_id, ""),
        &board_actor(fixture),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "view failed: {body}");
    body
}

async fn submit_contract(
    app: &Router,
    fixture: &Fixture,
    issue_id: Uuid,
    contract: Value,
) -> (StatusCode, Value) {
    send(
        app,
        Method::POST,
        orchestration_uri(fixture, issue_id, "/decision"),
        &agent_actor(fixture, fixture.ceo_id),
        contract,
    )
    .await
}

fn simple_contract() -> Value {
    json!({
        "contractVersion": 1,
        "assessment": "Small, self-contained request.",
        "complexity": "simple",
        "rationale": "The CEO can write the report directly.",
        "ceoExecutes": true,
        "members": [],
        "subtasks": [],
        "questions": [],
    })
}

#[sqlx::test]
async fn zero_task_budget_is_present_before_first_wake_and_clearing_it_replays_the_skipped_wake(
    pool: PgPool,
) {
    migrate(&pool).await;
    let server = fake_adapter_server(StatusCode::OK).await;
    let fixture = seed(&pool).await;
    point_agents_at_http(server, fixture.company_id, &pool).await;
    let (app, _state) = build_router(&pool).await;

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        json!({ "title": "Zero-budget task", "status": "todo", "taskBudgetCents": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let issue = issue_id(&body);
    let (budget, revision): (Option<i64>, i64) = sqlx::query_as(
        "SELECT task_budget_cents, task_budget_revision FROM issues WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("read persisted task budget");
    assert_eq!(budget, Some(0));
    assert_eq!(revision, 1);

    let first_wake: Option<(String, String)> = sqlx::query_as(
        "SELECT status::text, reason FROM agent_wakeup_requests
          WHERE company_id = $1 AND payload->>'issueId' = $2
          ORDER BY created_at LIMIT 1",
    )
    .bind(fixture.company_id)
    .bind(issue.to_string())
    .fetch_optional(&pool)
    .await
    .expect("read first wake");
    assert_eq!(
        first_wake,
        Some(("skipped".into(), "task_budget_hard_stop".into()))
    );
    let runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(fixture.company_id)
    .bind(issue.to_string())
    .fetch_one(&pool)
    .await
    .expect("count first task runs");
    assert_eq!(runs, 0, "the zero cap must block before run creation");

    let (status, _) = send(
        &app,
        Method::PUT,
        orchestration_uri(&fixture, issue, "/budget"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "budgetCents": null }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "only Board may change the cap"
    );

    let run_marker = chrono::Utc::now();
    let (status, body) = send(
        &app,
        Method::PUT,
        orchestration_uri(&fixture, issue, "/budget"),
        &board_actor(&fixture),
        json!({ "budgetCents": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["budgetCents"], Value::Null);
    assert_eq!(body["revision"], 2);
    assert_eq!(body["resumedWakeCount"], 1);

    let (_, run_status) =
        wait_for_terminal_run_after(&pool, fixture.company_id, issue, run_marker).await;
    assert_eq!(run_status, "succeeded");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_logs WHERE company_id = $1 AND resource_id = $2 AND event_type = 'issue.task_budget_updated'",
    )
    .bind(fixture.company_id)
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("budget activity audit");
    assert_eq!(audit_count, 1);
    let orchestration_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_orchestration_events event
          JOIN task_orchestrations orchestration ON orchestration.id = event.orchestration_id
         WHERE orchestration.company_id = $1 AND orchestration.root_issue_id = $2
           AND event.event_type = 'task_budget_updated'",
    )
    .bind(fixture.company_id)
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("orchestration budget event");
    assert_eq!(orchestration_event_count, 1);
}

#[sqlx::test]
async fn failed_orchestration_start_is_not_dispatched_and_create_retry_resumes_once(pool: PgPool) {
    migrate(&pool).await;
    let server = fake_adapter_server(StatusCode::OK).await;
    let fixture = seed(&pool).await;
    point_agents_at_http(server, fixture.company_id, &pool).await;
    let (app, state) = build_router(&pool).await;
    sqlx::query(
        "CREATE FUNCTION fail_task_orchestration_start_for_test() RETURNS trigger
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced orchestration start failure'; END $$",
    )
    .execute(&pool)
    .await
    .expect("create temporary failure trigger function");
    sqlx::query(
        "CREATE TRIGGER fail_task_orchestration_start_for_test
         BEFORE INSERT ON task_orchestration_events
         FOR EACH ROW WHEN (NEW.event_type = 'orchestration_started')
         EXECUTE FUNCTION fail_task_orchestration_start_for_test()",
    )
    .execute(&pool)
    .await
    .expect("create temporary failure trigger");

    let request_body = json!({
        "title": "Retry orchestration start",
        "status": "todo",
        "taskBudgetCents": 1000,
    });
    let (status, _) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        request_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let issue: Uuid = sqlx::query_scalar(
        "SELECT id FROM issues WHERE company_id = $1 AND title = 'Retry orchestration start'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("failed response still has a durable retry target");
    let required: bool = sqlx::query_scalar(
        "SELECT task_orchestration_required FROM issues WHERE company_id = $1 AND id = $2",
    )
    .bind(fixture.company_id)
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("read orchestration-required marker");
    assert!(required);
    let runs_before_probe: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(fixture.company_id)
    .bind(issue.to_string())
    .fetch_one(&pool)
    .await
    .expect("count runs after failed start");
    assert_eq!(
        runs_before_probe, 0,
        "failed orchestration start must not dispatch an ordinary issue run"
    );

    state
        .heartbeat_service
        .wakeup_with_options(
            fixture.ceo_id,
            issue,
            fixture.company_id,
            services::HeartbeatWakeupOptions {
                source: Some("automation".into()),
                reason: Some("pending_orchestration_probe".into()),
                idempotency_key: Some(format!("pending-orchestration-probe:{issue}")),
                context_snapshot: Some(json!({ "issueId": issue })),
                ..Default::default()
            },
        )
        .await
        .expect("pending orchestration wake is safely skipped");
    let pending_reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM agent_wakeup_requests WHERE company_id = $1 AND idempotency_key = $2",
    )
    .bind(fixture.company_id)
    .bind(format!("pending-orchestration-probe:{issue}"))
    .fetch_optional(&pool)
    .await
    .expect("read pending marker gate");
    assert_eq!(
        pending_reason.as_deref(),
        Some("task_orchestration_start_pending")
    );

    sqlx::query("DROP TRIGGER fail_task_orchestration_start_for_test ON task_orchestration_events")
        .execute(&pool)
        .await
        .expect("drop temporary failure trigger");
    sqlx::query("DROP FUNCTION fail_task_orchestration_start_for_test()")
        .execute(&pool)
        .await
        .expect("drop temporary failure trigger function");

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        request_body.clone(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "same-title retry should recover: {body}"
    );
    assert_eq!(issue_id(&body), issue);
    let (_, run_status) = wait_for_new_terminal_run(&pool, fixture.company_id, issue).await;
    assert_eq!(run_status, "succeeded");

    // Recovery replays must follow the CEO persisted on the orchestration,
    // even if the root issue's assignee was later cleared or changed.
    sqlx::query("UPDATE issues SET assignee_agent_id = NULL WHERE company_id = $1 AND id = $2")
        .bind(fixture.company_id)
        .bind(issue)
        .execute(&pool)
        .await
        .expect("clear the issue assignee after orchestration start");

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        request_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "completed idempotency replay: {body}"
    );
    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(fixture.company_id)
    .bind(issue.to_string())
    .fetch_one(&pool)
    .await
    .expect("count runs after duplicate replay");
    assert_eq!(
        run_count, 1,
        "stable wake idempotency must prevent duplicate runs"
    );
}

#[sqlx::test]
async fn task_budget_counts_root_and_child_costs_but_not_foreign_company_costs(pool: PgPool) {
    migrate(&pool).await;
    let server = fake_adapter_server(StatusCode::OK).await;
    let fixture = seed(&pool).await;
    point_agents_at_http(server, fixture.company_id, &pool).await;
    let (app, state) = build_router(&pool).await;

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        json!({ "title": "Tree-budget task", "status": "todo", "taskBudgetCents": 100 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let root_issue = issue_id(&body);
    wait_until_settled(&pool, fixture.company_id, root_issue).await;

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/companies/{}/issues", fixture.company_id),
        &board_actor(&fixture),
        json!({ "title": "Budget child", "status": "todo", "parentId": root_issue }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let child_issue = issue_id(&body);

    let foreign_company_id = Uuid::new_v4();
    let foreign_agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents) VALUES ($1, 'Foreign budget tenant', $2, FALSE)",
    )
    .bind(foreign_company_id)
    .bind(format!("FX{}", &foreign_company_id.simple().to_string()[..7]))
    .execute(&pool)
    .await
    .expect("insert foreign company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type, adapter_config)
         VALUES ($1, $2, 'Foreign agent', 'general', 'idle', 'process', '{}'::jsonb)",
    )
    .bind(foreign_agent_id)
    .bind(foreign_company_id)
    .execute(&pool)
    .await
    .expect("insert foreign agent");

    for (company_id, agent_id, issue_id, amount) in [
        (fixture.company_id, fixture.ceo_id, root_issue, 60_i32),
        (fixture.company_id, fixture.ceo_id, child_issue, 40_i32),
        (foreign_company_id, foreign_agent_id, root_issue, 9_000_i32),
    ] {
        sqlx::query(
            "INSERT INTO cost_events (id, company_id, agent_id, issue_id, amount_cents, cost_cents, event_type)
             VALUES ($1, $2, $3, $4, $5, $5, 'task_budget_test')",
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id)
        .bind(amount)
        .execute(&pool)
        .await
        .expect("insert task budget cost event");
    }

    let (status, view) = send(
        &app,
        Method::GET,
        orchestration_uri(&fixture, root_issue, ""),
        &board_actor(&fixture),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["taskBudgetCents"], 100);
    assert_eq!(view["taskSpentCents"], 100);
    assert_eq!(view["taskRemainingCents"], 0);
    assert_eq!(view["taskBudgetExhausted"], true);

    let before_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(fixture.company_id)
    .bind(root_issue.to_string())
    .fetch_one(&pool)
    .await
    .expect("count runs before exact-cap wake");
    let wake_marker = chrono::Utc::now();
    state
        .heartbeat_service
        .wakeup_with_options(
            fixture.ceo_id,
            root_issue,
            fixture.company_id,
            services::HeartbeatWakeupOptions {
                source: Some("assignment".into()),
                trigger_detail: Some("system".into()),
                reason: Some("issue_assigned".into()),
                requested_by_actor_type: Some("user".into()),
                requested_by_actor_id: Some(fixture.board_user_id),
                payload: Some(json!({ "issueId": root_issue, "mutation": "create" })),
                context_snapshot: Some(json!({
                    "issueId": root_issue,
                    "source": "issue.create",
                })),
                ..Default::default()
            },
        )
        .await
        .expect("exact-cap wake should be recorded as skipped");
    let (exact_wake_reason, exact_wake_status): (String, String) = sqlx::query_as(
        "SELECT reason, status::text FROM agent_wakeup_requests
          WHERE company_id = $1 AND agent_id = $2 AND requested_by_actor_id = $3
            AND requested_by_actor_type = 'user' AND source = 'assignment'
            AND created_at >= $4
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(fixture.company_id)
    .bind(fixture.ceo_id)
    .bind(fixture.board_user_id)
    .bind(wake_marker)
    .fetch_one(&pool)
    .await
    .expect("read exact-cap wake result");
    assert_eq!(exact_wake_reason, "task_budget_hard_stop");
    assert_eq!(exact_wake_status, "skipped");
    let after_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND (context_snapshot->>'issueId' = $2 OR context_snapshot->>'taskId' = $2)",
    )
    .bind(fixture.company_id)
    .bind(root_issue.to_string())
    .fetch_one(&pool)
    .await
    .expect("count runs after exact-cap wake");
    assert_eq!(after_runs, before_runs);

    let run_marker = chrono::Utc::now();
    let (status, body) = send(
        &app,
        Method::PUT,
        orchestration_uri(&fixture, root_issue, "/budget"),
        &board_actor(&fixture),
        json!({ "budgetCents": 101 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["resumedWakeCount"], 1);
    let (_, run_status) =
        wait_for_terminal_run_after(&pool, fixture.company_id, root_issue, run_marker).await;
    assert_eq!(run_status, "succeeded");
}

#[sqlx::test]
async fn simple_task_reaches_a_queryable_final_report_without_new_members(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({
                "title": "Write the weekly note",
                "description": "A short update.",
                "status": "todo",
            }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    let assignee: Option<Uuid> =
        sqlx::query_scalar("SELECT assignee_agent_id FROM issues WHERE id = $1")
            .bind(issue)
            .fetch_one(&pool)
            .await
            .expect("assignee");
    assert_eq!(assignee, Some(fixture.ceo_id));

    let view = orchestration_view(&app, &fixture, issue).await;
    assert_eq!(view["stage"], "assessing");
    assert_eq!(view["ceoAgentId"], fixture.ceo_id.to_string());

    let (status, body) = submit_contract(&app, &fixture, issue, simple_contract()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "executing");

    // A successful run is not acceptance: the orchestrated issue still requires
    // the persisted report.
    let requires_review = services::task_orchestration_service::orchestration_requires_review(
        &pool,
        fixture.company_id,
        issue,
    )
    .await
    .expect("gate");
    assert!(requires_review);

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/final-report"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({
            "body": "Weekly note delivered.",
            "evidence": { "summary": "one paragraph" },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "completed");

    let view = orchestration_view(&app, &fixture, issue).await;
    assert_eq!(view["stage"], "completed");
    assert_eq!(view["finalReport"], "Weekly note delivered.");
    assert!(view["completedAt"].is_string());
    assert!(
        view["subtasks"].as_array().expect("subtasks").is_empty(),
        "a simple task must not manufacture subtasks"
    );
    assert!(
        view["deliverables"]
            .as_array()
            .expect("deliverables")
            .iter()
            .any(|deliverable| deliverable["verdict"].is_null()
                && deliverable["body"] == "Weekly note delivered."),
        "the final deliverable must be readable"
    );

    let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE company_id = $1")
        .bind(fixture.company_id)
        .fetch_one(&pool)
        .await
        .expect("agent count");
    assert_eq!(agent_count, 3, "no member may be created for a simple task");

    let requires_review = services::task_orchestration_service::orchestration_requires_review(
        &pool,
        fixture.company_id,
        issue,
    )
    .await
    .expect("gate");
    assert!(!requires_review);

    let status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(issue)
        .fetch_one(&pool)
        .await
        .expect("issue status");
    assert_eq!(status, "done");
}

#[sqlx::test]
async fn complex_task_clarifies_rejects_resubmits_and_escalates_to_the_ceo(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({
                "title": "Prepare the launch plan",
                "description": "Needs analysis plus a plan.",
                "status": "todo",
            }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    // 1. The CEO asks a blocking question; no work may be planned yet.
    let asking_contract = json!({
        "contractVersion": 1,
        "assessment": "Scope depends on the target launch date.",
        "complexity": "complex",
        "rationale": "One analyst and one lead are enough for this scope.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.lead_id, "action": "reuse", "role": "lead",
              "responsibilities": "own the plan", "reason": "existing capacity" },
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "market analysis", "reason": "analytical skills",
              "reportsToAgentId": fixture.lead_id },
        ],
        "subtasks": [],
        "questions": [
            { "question": "What launch date must the plan target?",
              "whyItBlocks": "The timeline changes the staffing and scope." }
        ],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, asking_contract).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "clarifying");

    let view = orchestration_view(&app, &fixture, issue).await;
    assert_eq!(
        view["pendingClarifications"]
            .as_array()
            .expect("questions")
            .len(),
        1
    );
    let interaction_id = view["pendingClarifications"][0]["interactionId"]
        .as_str()
        .expect("interaction id")
        .to_string();

    // 2. The user answers; the orchestration resumes at its pre-clarification
    // stage, and the CEO can now plan the work.
    let (status, body) = send(
        &app,
        Method::POST,
        format!("/issues/{}/interactions/{}/answer", issue, interaction_id),
        &board_actor(&fixture),
        json!({ "answers": [ { "questionId": "q-1", "otherText": "March 15" } ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = submit_contract(
        &app,
        &fixture,
        issue,
        json!({
            "contractVersion": 1,
            "assessment": "Scope is known now.",
            "complexity": "complex",
            "rationale": "One analyst plus one lead who synthesises.",
            "ceoExecutes": false,
            "members": [
                { "agentId": fixture.lead_id, "action": "reuse", "role": "lead",
                  "responsibilities": "own the plan", "reason": "existing capacity" },
                { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
                  "responsibilities": "market analysis", "reason": "analytical skills",
                  "reportsToAgentId": fixture.lead_id },
            ],
            "subtasks": [
                { "key": "analysis", "title": "Market analysis",
                  "description": "Analyse competitors and pricing.",
                  "ownerAgentId": fixture.worker_id,
                  "acceptanceCriteria": "Findings with sources" },
                { "key": "plan", "title": "Launch plan",
                  "description": "Synthesise the findings into a plan.",
                  "ownerAgentId": fixture.lead_id,
                  "acceptanceCriteria": "Dated, owned plan",
                  "blockedBy": ["analysis"] }
            ],
            "questions": [],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let view = orchestration_view(&app, &fixture, issue).await;
    assert_eq!(view["stage"], "executing");
    assert_eq!(
        view["pendingClarifications"]
            .as_array()
            .expect("questions")
            .len(),
        0,
        "the answered question must be closed"
    );
    let subtasks = view["subtasks"].as_array().expect("subtasks").clone();
    assert_eq!(subtasks.len(), 2, "{view}");
    let analysis = subtasks
        .iter()
        .find(|subtask| subtask["ownerAgentId"] == fixture.worker_id.to_string())
        .expect("worker subtask");
    let plan = subtasks
        .iter()
        .find(|subtask| subtask["ownerAgentId"] == fixture.lead_id.to_string())
        .expect("lead subtask");
    let analysis_issue = analysis["issueId"].as_str().unwrap().to_string();
    let plan_issue = plan["issueId"].as_str().unwrap().to_string();
    assert_eq!(analysis["reviewerAgentId"], fixture.lead_id.to_string());
    assert_eq!(plan["reviewerAgentId"], fixture.ceo_id.to_string());

    // The CEO run prompt must carry the persisted workflow state.
    let briefing = services::heartbeat_service::orchestration_prompt_briefing(
        &pool,
        fixture.company_id,
        issue,
    )
    .await
    .expect("orchestration briefing");
    assert!(briefing.contains("Stage: executing"), "{briefing}");
    assert!(briefing.contains(&analysis_issue), "{briefing}");
    assert!(briefing.contains(&plan_issue), "{briefing}");
    assert!(briefing.contains("Pending user questions: 0"), "{briefing}");
    // A subtask executor sees its own acceptance contract.
    let subtask_briefing = services::heartbeat_service::orchestration_prompt_briefing(
        &pool,
        fixture.company_id,
        Uuid::parse_str(&analysis_issue).unwrap(),
    )
    .await
    .expect("subtask briefing");
    assert!(
        subtask_briefing.contains("Findings with sources"),
        "{subtask_briefing}"
    );

    // The dependent plan subtask starts blocked and is released on acceptance.
    let plan_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(Uuid::parse_str(&plan_issue).unwrap())
        .fetch_one(&pool)
        .await
        .expect("plan status");
    assert_eq!(plan_status, "blocked");

    // 2. The worker submits; the lead rejects with feedback.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{analysis_issue}/submit"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "body": "Draft findings", "evidence": { "source": "v1" } }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first_deliverable = body["deliverableId"].clone();
    assert_eq!(body["version"], 1);

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{analysis_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.lead_id),
        json!({ "accepted": false, "feedback": "Add competitor pricing." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "executing");

    let analysis_status: String =
        sqlx::query_scalar("SELECT status FROM task_orchestration_subtasks WHERE issue_id = $1")
            .bind(Uuid::parse_str(&analysis_issue).unwrap())
            .fetch_one(&pool)
            .await
            .expect("subtask status");
    assert_eq!(
        analysis_status, "pending",
        "a rejection reopens the subtask"
    );
    let issue_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(Uuid::parse_str(&analysis_issue).unwrap())
        .fetch_one(&pool)
        .await
        .expect("issue status");
    assert_ne!(issue_status, "done", "a rejected subtask is reopened");

    // 3. Re-submission is a new version; the rejected history stays readable.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{analysis_issue}/submit"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "body": "Final findings", "evidence": { "source": "v2" } }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["version"], 2);
    let second_deliverable = body["deliverableId"].clone();
    assert_ne!(first_deliverable, second_deliverable);

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{analysis_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.lead_id),
        json!({ "accepted": true, "feedback": "Looks good." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let view = orchestration_view(&app, &fixture, issue).await;
    let decisions = view["deliverables"].as_array().expect("deliverables");
    assert_eq!(
        decisions
            .iter()
            .filter(|deliverable| deliverable["verdict"] == "rejected")
            .count(),
        1
    );
    assert_eq!(
        decisions
            .iter()
            .filter(|deliverable| deliverable["verdict"] == "accepted")
            .count(),
        1
    );

    // 4. The blocker resolved, so the lead's own subtask became runnable.
    let plan_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(Uuid::parse_str(&plan_issue).unwrap())
        .fetch_one(&pool)
        .await
        .expect("plan status");
    assert!(
        plan_status != "blocked" && plan_status != "done",
        "the dependent subtask is released but not completed: {plan_status}"
    );

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, &format!("/subtasks/{plan_issue}/submit")),
        &agent_actor(&fixture, fixture.lead_id),
        json!({ "body": "Launch plan", "evidence": { "analysis": "done" } }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // 5. The CEO cannot finish before the lead's deliverable is accepted.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/final-report"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "body": "Not yet." }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // 6. The lead is the worker's superior; the CEO is the lead's superior.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, &format!("/subtasks/{plan_issue}/review")),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "accepted": true, "feedback": "Approved at the top." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/final-report"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({
            "body": "Launch plan delivered.",
            "evidence": {
                "analysis": "Final findings",
                "plan": "Launch plan",
            },
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "completed");

    let view = orchestration_view(&app, &fixture, issue).await;
    assert_eq!(view["stage"], "completed");
    assert_eq!(view["finalReport"], "Launch plan delivered.");
    assert_eq!(
        services::task_orchestration_service::pending_required_subtasks(
            &pool,
            fixture.company_id,
            issue
        )
        .await
        .expect("pending"),
        0
    );
    let plan_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(issue)
        .fetch_one(&pool)
        .await
        .expect("root status");
    assert_eq!(plan_status, "done");
}

#[sqlx::test]
async fn permissions_isolation_and_concurrent_reviews_are_enforced(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;

    // A second company with its own agent.
    let other_company = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix, require_board_approval_for_new_agents)
         VALUES ($1, $2, $3, FALSE)",
    )
    .bind(other_company)
    .bind("Other company")
    .bind(format!("OT{}", &other_company.simple().to_string()[..8]))
    .execute(&pool)
    .await
    .expect("company");
    let outsider = seed_agent(&pool, other_company, "Outsider", "general").await;

    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Review isolation", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    let contract = json!({
        "contractVersion": 1,
        "assessment": "One deliverable.",
        "complexity": "complex",
        "rationale": "One owner.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [
            { "key": "a", "title": "Analyse", "description": "Analyse",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "Clear" }
        ],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, contract).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let subtask_issue = body["subtaskIssueIds"][0].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/submit"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "body": "Work" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // Self-review is rejected.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "accepted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // An agent from another company cannot read the orchestration.
    let (status, _) = send(
        &app,
        Method::GET,
        orchestration_uri(&fixture, issue, ""),
        &AuthorizationActor::Agent {
            agent_id: outsider,
            company_id: other_company,
            run_id: None,
            source: ActorSource::AgentKey,
            key_id: None,
            key_scope: None,
            responsible_user_id: None,
            on_behalf_of_user_id: None,
            on_behalf_of_memberships: vec![],
        },
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Only the direct superior (the CEO, because the worker has no plan
    // superior) may accept - the lead is not authorized here.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.lead_id),
        json!({ "accepted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // Two concurrent verdicts produce exactly one decision.
    let app_one = app.clone();
    let app_two = app.clone();
    let fixture_one = &fixture;
    let fixture_two = &fixture;
    let uri = orchestration_uri(
        &fixture,
        issue,
        &format!("/subtasks/{subtask_issue}/review"),
    );
    let actor_one = agent_actor(fixture_one, fixture_one.ceo_id);
    let actor_two = agent_actor(fixture_two, fixture_two.ceo_id);
    let (first, second) = tokio::join!(
        send(
            &app_one,
            Method::POST,
            uri.clone(),
            &actor_one,
            json!({ "accepted": true, "feedback": "ok" }),
        ),
        send(
            &app_two,
            Method::POST,
            uri,
            &actor_two,
            json!({ "accepted": false, "feedback": "no" }),
        )
    );
    let statuses = [first.0, second.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1,
        "exactly one verdict must win: {statuses:?}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1,
        "the loser is a conflict: {statuses:?}"
    );
    let decisions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM task_acceptance_decisions WHERE company_id = $1")
            .bind(fixture.company_id)
            .fetch_one(&pool)
            .await
            .expect("decisions");
    assert_eq!(decisions, 1);
}

#[sqlx::test]
async fn team_membership_changes_keep_history_and_never_touch_the_agent_row(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Team adjustment", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    let scoped_contract = json!({
        "contractVersion": 1,
        "assessment": "One deliverable plus a lead.",
        "complexity": "complex",
        "rationale": "Two roles are enough.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.lead_id, "action": "reuse", "role": "lead",
              "responsibilities": "review", "reason": "capacity" },
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills",
              "reportsToAgentId": fixture.lead_id },
        ],
        "subtasks": [
            { "key": "a", "title": "Analyse", "description": "Analyse",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "Clear" }
        ],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, scoped_contract).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let subtask_issue = body["subtaskIssueIds"][0].as_str().unwrap().to_string();

    // An owner with an open subtask cannot be removed.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/team-members"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "members": [ { "agentId": fixture.worker_id, "action": "exit",
                                "role": "analyst", "responsibilities": "analysis",
                                "reason": "no longer needed" } ] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // Hand the work over by accepting it, then remove the member.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/submit"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "body": "Findings" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.lead_id),
        json!({ "accepted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/team-members"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "members": [ { "agentId": fixture.worker_id, "action": "exit",
                                "role": "analyst", "responsibilities": "analysis",
                                "reason": "work handed over" } ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["revision"], 2);
    let members = body["members"].as_array().expect("members");
    let exited = members
        .iter()
        .find(|member| member["agentId"] == fixture.worker_id.to_string())
        .expect("worker member");
    assert_eq!(exited["action"], "exit");
    assert!(exited["exitedAt"].is_string());

    // The company-level agent is untouched and can still take other work.
    let agent_row: (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status::text, paused_at FROM agents WHERE id = $1")
            .bind(fixture.worker_id)
            .fetch_one(&pool)
            .await
            .expect("agent");
    assert_ne!(agent_row.0, "terminated");
    assert!(agent_row.1.is_none());

    // The plan history keeps the earlier revision.
    let revisions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_team_plans WHERE orchestration_id = (
            SELECT id FROM task_orchestrations WHERE root_issue_id = $1)",
    )
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("revisions");
    assert_eq!(revisions, 2);

    let events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_orchestration_events
          WHERE orchestration_id = (SELECT id FROM task_orchestrations WHERE root_issue_id = $1)
            AND event_type = 'team_members_adjusted'",
    )
    .bind(issue)
    .fetch_one(&pool)
    .await
    .expect("events");
    assert_eq!(events, 1);
}

#[sqlx::test]
async fn hires_are_inert_until_approved_and_rejections_stay_pending(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    sqlx::query("UPDATE companies SET require_board_approval_for_new_agents = TRUE WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&pool)
        .await
        .expect("approval policy");

    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Needs a designer", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    let hire_contract = json!({
        "contractVersion": 1,
        "assessment": "The team has no designer.",
        "complexity": "complex",
        "rationale": "Hire one designer before planning work.",
        "ceoExecutes": false,
        "members": [
            { "action": "hire", "role": "designer",
              "responsibilities": "design the launch assets",
              "reason": "capability gap",
              "hire": { "name": "Designer", "adapterType": "process" } },
        ],
        "subtasks": [],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, hire_contract.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "awaiting_team_approval");
    let approval_id = body["approvalIds"][0].as_str().unwrap().to_string();

    let pending_agents: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agents WHERE company_id = $1 AND status = 'pending_approval'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("pending agents");
    assert_eq!(pending_agents, 1, "a hired agent starts pending approval");

    // Work cannot be planned for a pending hire.
    let (status, body) = submit_contract(
        &app,
        &fixture,
        issue,
        json!({
            "contractVersion": 1,
            "assessment": "x",
            "complexity": "complex",
            "rationale": "x",
            "ceoExecutes": false,
            "members": [
                { "action": "hire", "role": "designer", "responsibilities": "design",
                  "reason": "gap", "hire": { "name": "Designer2", "adapterType": "process" } },
            ],
            "subtasks": [
                { "key": "a", "title": "A", "description": "A",
                  "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "ok" }
            ],
            "questions": [],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Approving the team plan advances the workflow; approving the hire
    // activates the agent.
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/team-plan"),
        &board_actor(&fixture),
        json!({ "approved": true, "note": "go ahead" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "planning");

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/approvals/{approval_id}/approve"),
        &board_actor(&fixture),
        json!({ "decisionNote": "approved" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (hired_agent, status): (Uuid, String) = sqlx::query_as(
        "SELECT id, status::text FROM agents
          WHERE company_id = $1 AND name = 'Designer'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("hired agent");
    assert_eq!(status, "idle", "an approved hire becomes executable");
    let hire_status: Option<String> = sqlx::query_scalar(
        "SELECT member.hire_status FROM task_team_members member
           JOIN task_team_plans plan ON plan.id = member.plan_id
          WHERE plan.orchestration_id = (
              SELECT id FROM task_orchestrations WHERE root_issue_id = $1)
            AND member.hired_agent_id = $2",
    )
    .bind(issue)
    .bind(hired_agent)
    .fetch_optional(&pool)
    .await
    .expect("member hire status");
    assert_eq!(hire_status.as_deref(), Some("hired"));

    // The new member can now be reused for real work.
    let (status, body) = submit_contract(
        &app,
        &fixture,
        issue,
        json!({
            "contractVersion": 1,
            "assessment": "The designer joined.",
            "complexity": "complex",
            "rationale": "One designer own the deliverable.",
            "ceoExecutes": false,
            "members": [
                { "agentId": hired_agent, "action": "reuse", "role": "designer",
                  "responsibilities": "design", "reason": "hired for this task" },
            ],
            "subtasks": [
                { "key": "a", "title": "Design", "description": "Design",
                  "ownerAgentId": hired_agent, "acceptanceCriteria": "Assets shipped" }
            ],
            "questions": [],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "executing");

    // A rejected hire never becomes executable and its member row says so.
    let second_issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Second hire", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    let mut second_hire = hire_contract.clone();
    second_hire["members"][0]["hire"]["name"] = json!("Designer Two");
    let (status, body) = submit_contract(&app, &fixture, second_issue, second_hire).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let rejected_approval = body["approvalIds"][0].as_str().unwrap().to_string();
    let (status, _) = send(
        &app,
        Method::POST,
        format!("/approvals/{rejected_approval}/reject"),
        &board_actor(&fixture),
        json!({ "decisionNote": "not now" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rejected_status: Option<String> = sqlx::query_scalar(
        "SELECT member.hire_status FROM task_team_members member
           JOIN task_team_plans plan ON plan.id = member.plan_id
          WHERE plan.orchestration_id = (
              SELECT id FROM task_orchestrations WHERE root_issue_id = $1)",
    )
    .bind(second_issue)
    .fetch_optional(&pool)
    .await
    .expect("hire status");
    assert_eq!(rejected_status.as_deref(), Some("rejected"));
    let still_pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agents WHERE company_id = $1 AND name = 'Designer Two' AND status = 'pending_approval'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("pending count");
    assert_eq!(still_pending, 1, "a rejected hire stays pending and inert");
}

#[sqlx::test]
async fn outbox_actions_replay_once_and_clarification_answers_are_idempotent(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, state) = build_router(&pool).await;
    let service = orchestration_service(&state);

    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Recovery coverage", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    let (status, body) = submit_contract(&app, &fixture, issue, simple_contract()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let orchestration_id: Uuid =
        sqlx::query_scalar("SELECT id FROM task_orchestrations WHERE root_issue_id = $1")
            .bind(issue)
            .fetch_one(&pool)
            .await
            .expect("orchestration id");

    // A queued action is applied exactly once.
    sqlx::query(
        "INSERT INTO task_orchestration_actions
            (orchestration_id, company_id, action_type, payload, idempotency_key)
         VALUES ($1, $2, 'wake_assignee', $3, 'recovery-test-wake')",
    )
    .bind(orchestration_id)
    .bind(fixture.company_id)
    .bind(json!({
        "issueId": issue,
        "agentId": fixture.ceo_id,
        "reason": "recovery_test",
    }))
    .execute(&pool)
    .await
    .expect("insert action");
    assert_eq!(
        service
            .drain_actions(orchestration_id)
            .await
            .expect("drain"),
        1
    );
    assert_eq!(
        service
            .drain_actions(orchestration_id)
            .await
            .expect("drain again"),
        0,
        "a delivered action is not replayed"
    );

    // A crash between claim and apply leaves `running`; the recovery sweep
    // returns it to `pending` and the next drain applies it.
    sqlx::query(
        "INSERT INTO task_orchestration_actions
            (orchestration_id, company_id, action_type, payload, idempotency_key,
             status, updated_at)
         VALUES ($1, $2, 'wake_assignee', $3, 'recovery-test-stale', 'running',
                 NOW() - INTERVAL '10 minutes')",
    )
    .bind(orchestration_id)
    .bind(fixture.company_id)
    .bind(json!({
        "issueId": issue,
        "agentId": fixture.ceo_id,
        "reason": "recovery_stale",
    }))
    .execute(&pool)
    .await
    .expect("insert stale action");
    assert_eq!(
        service
            .recover_stale_actions(60)
            .await
            .expect("recover stale"),
        1
    );
    assert!(
        service.recover_pending_work(8).await.expect("recover work") >= 1,
        "the released action is applied"
    );

    // A replayed clarification answer is a no-op.
    let question_issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Duplicate answer", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    let asking = json!({
        "contractVersion": 1,
        "assessment": "Need one answer.",
        "complexity": "complex",
        "rationale": "The answer decides the shape.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [],
        "questions": [ { "question": "Which market?", "whyItBlocks": "Scope" } ],
    });
    let (status, body) = submit_contract(&app, &fixture, question_issue, asking).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "clarifying");
    let interaction_id = body["questionInteractionIds"][0]
        .as_str()
        .unwrap()
        .to_string();

    let (status, body) = send(
        &app,
        Method::POST,
        format!("/issues/{question_issue}/interactions/{interaction_id}/answer"),
        &board_actor(&fixture),
        json!({ "answers": [ { "questionId": "q-1", "otherText": "EU" } ] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = send(
        &app,
        Method::POST,
        format!("/issues/{question_issue}/interactions/{interaction_id}/answer"),
        &board_actor(&fixture),
        json!({ "answers": [ { "questionId": "q-1", "otherText": "US" } ] }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a resolved question cannot be answered twice: {body}"
    );

    let stage: String =
        sqlx::query_scalar("SELECT stage FROM task_orchestrations WHERE root_issue_id = $1")
            .bind(question_issue)
            .fetch_one(&pool)
            .await
            .expect("stage");
    assert_eq!(stage, "planning");
    // Replaying the recorded answer at the service level is idempotent.
    assert!(service
        .record_clarification_answer(
            fixture.company_id,
            question_issue,
            Uuid::parse_str(&interaction_id).unwrap()
        )
        .await
        .is_ok());
    assert!(service
        .record_clarification_answer(
            fixture.company_id,
            question_issue,
            Uuid::parse_str(&interaction_id).unwrap()
        )
        .await
        .is_ok());
    let stage_after: String =
        sqlx::query_scalar("SELECT stage FROM task_orchestrations WHERE root_issue_id = $1")
            .bind(question_issue)
            .fetch_one(&pool)
            .await
            .expect("stage");
    assert_eq!(stage_after, "planning", "a replay never advances the stage");
}

#[sqlx::test]
async fn cancellation_closes_child_issues_without_touching_agents_and_resumes(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Cancel coverage", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    let contract = json!({
        "contractVersion": 1,
        "assessment": "One deliverable.",
        "complexity": "complex",
        "rationale": "One owner.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [
            { "key": "a", "title": "Analyse", "description": "Analyse",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "Clear" }
        ],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, contract).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let subtask_issue = body["subtaskIssueIds"][0].as_str().unwrap().to_string();

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/cancel"),
        &board_actor(&fixture),
        json!({ "reason": "user changed their mind" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "cancelled");

    let child_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(Uuid::parse_str(&subtask_issue).unwrap())
        .fetch_one(&pool)
        .await
        .expect("child status");
    assert_eq!(child_status, "cancelled");
    let agent_status: String = sqlx::query_scalar("SELECT status::text FROM agents WHERE id = $1")
        .bind(fixture.worker_id)
        .fetch_one(&pool)
        .await
        .expect("agent status");
    assert_ne!(agent_status, "terminated");

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/resume"),
        &board_actor(&fixture),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "executing");
}

#[sqlx::test]
async fn a_real_run_settles_into_review_and_the_ceo_report_completes_the_task(pool: PgPool) {
    migrate(&pool).await;
    let server = fake_adapter_server(StatusCode::OK).await;
    let fixture = seed(&pool).await;
    point_agents_at_http(server, fixture.company_id, &pool).await;
    let (app, state) = build_router(&pool).await;

    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Run settlement", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    // The assignment wake starts a real run immediately; let it finish so this
    // test observes the run it explicitly starts.
    wait_until_settled(&pool, fixture.company_id, issue).await;

    let (status, body) = submit_contract(&app, &fixture, issue, simple_contract()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["stage"], "executing");

    let run_marker = chrono::Utc::now();
    state
        .heartbeat_service
        .wakeup_with_options(
            fixture.ceo_id,
            issue,
            fixture.company_id,
            services::HeartbeatWakeupOptions {
                source: Some("automation".to_string()),
                trigger_detail: Some("test".to_string()),
                reason: Some("run_settlement_test".to_string()),
                idempotency_key: Some(format!("run-settlement:{issue}")),
                context_snapshot: Some(
                    json!({ "issueId": issue, "wakeReason": "run_settlement_test" }),
                ),
                ..Default::default()
            },
        )
        .await
        .expect("wake CEO");

    let (run_id, run_status) =
        wait_for_terminal_run_after(&pool, fixture.company_id, issue, run_marker).await;
    assert_eq!(run_status, "succeeded", "run {run_id} did not succeed");

    let issue_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(issue)
        .fetch_one(&pool)
        .await
        .expect("issue status");
    assert_eq!(
        issue_status, "in_review",
        "a successful run must not complete an orchestrated task"
    );
    assert!(
        services::task_orchestration_service::orchestration_requires_review(
            &pool,
            fixture.company_id,
            issue
        )
        .await
        .expect("review gate")
    );
    let stage: String =
        sqlx::query_scalar("SELECT stage FROM task_orchestrations WHERE root_issue_id = $1")
            .bind(issue)
            .fetch_one(&pool)
            .await
            .expect("stage");
    assert_eq!(stage, "executing");

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/final-report"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "body": "Delivered after a real adapter run." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "completed");
    let issue_status: String = sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
        .bind(issue)
        .fetch_one(&pool)
        .await
        .expect("issue status");
    assert_eq!(issue_status, "done");
}

#[sqlx::test]
async fn a_subtask_run_waits_for_review_and_a_failed_run_reopens_the_issue(pool: PgPool) {
    migrate(&pool).await;
    let ok_server = fake_adapter_server(StatusCode::OK).await;
    let bad_server = fake_adapter_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let fixture = seed(&pool).await;
    point_agents_at_http(ok_server, fixture.company_id, &pool).await;
    let (app, state) = build_router(&pool).await;

    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Subtask run settlement", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    wait_until_settled(&pool, fixture.company_id, issue).await;

    let run_marker = chrono::Utc::now();
    let contract = json!({
        "contractVersion": 1,
        "assessment": "One deliverable.",
        "complexity": "complex",
        "rationale": "One owner.",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [
            { "key": "a", "title": "Analyse", "description": "Analyse",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "Clear" }
        ],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, contract).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let subtask_issue: Uuid =
        Uuid::parse_str(body["subtaskIssueIds"][0].as_str().unwrap()).expect("subtask id");

    // Planning dispatched the owner through the durable outbox; a real run
    // executed and settled.
    let (_run_id, run_status) =
        wait_for_terminal_run_after(&pool, fixture.company_id, subtask_issue, run_marker).await;
    assert_eq!(run_status, "succeeded");
    let subtask_issue_status: String =
        sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
            .bind(subtask_issue)
            .fetch_one(&pool)
            .await
            .expect("subtask issue status");
    assert_eq!(subtask_issue_status, "in_review");
    let subtask_status: String =
        sqlx::query_scalar("SELECT status FROM task_orchestration_subtasks WHERE issue_id = $1")
            .bind(subtask_issue)
            .fetch_one(&pool)
            .await
            .expect("subtask status");
    assert_eq!(
        subtask_status, "pending",
        "a finished run is not an accepted subtask"
    );

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/submit"),
        ),
        &agent_actor(&fixture, fixture.worker_id),
        json!({ "body": "Findings", "evidence": { "run": _run_id } }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(
            &fixture,
            issue,
            &format!("/subtasks/{subtask_issue}/review"),
        ),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "accepted": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let subtask_issue_status: String =
        sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
            .bind(subtask_issue)
            .fetch_one(&pool)
            .await
            .expect("subtask issue status");
    assert_eq!(subtask_issue_status, "done");

    let (status, body) = send(
        &app,
        Method::POST,
        orchestration_uri(&fixture, issue, "/final-report"),
        &agent_actor(&fixture, fixture.ceo_id),
        json!({ "body": "Analysis accepted." }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stage"], "completed");

    // A failing adapter must reopen the issue and keep the workflow open. Use a
    // separate company so no previous CEO run can contend for the start lock.
    let failed_fixture = seed(&pool).await;
    point_agents_at_http(bad_server, failed_fixture.company_id, &pool).await;
    let failed_issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", failed_fixture.company_id),
            &board_actor(&failed_fixture),
            json!({ "title": "Failed run settlement", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };
    wait_until_settled(&pool, failed_fixture.company_id, failed_issue).await;
    let (status, body) =
        submit_contract(&app, &failed_fixture, failed_issue, simple_contract()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let run_marker = chrono::Utc::now();
    state
        .heartbeat_service
        .wakeup_with_options(
            failed_fixture.ceo_id,
            failed_issue,
            failed_fixture.company_id,
            services::HeartbeatWakeupOptions {
                source: Some("automation".to_string()),
                trigger_detail: Some("test".to_string()),
                reason: Some("failed_run_test".to_string()),
                idempotency_key: Some(format!("failed-run:{failed_issue}")),
                context_snapshot: Some(json!({ "issueId": failed_issue })),
                ..Default::default()
            },
        )
        .await
        .expect("wake CEO for failing run");
    let (_run_id, run_status) =
        wait_for_terminal_run_after(&pool, failed_fixture.company_id, failed_issue, run_marker)
            .await;
    assert_eq!(run_status, "failed");
    let failed_issue_status: String =
        sqlx::query_scalar("SELECT status::text FROM issues WHERE id = $1")
            .bind(failed_issue)
            .fetch_one(&pool)
            .await
            .expect("failed issue status");
    assert_eq!(
        failed_issue_status, "todo",
        "a failed run reopens the issue"
    );
    let stage: String =
        sqlx::query_scalar("SELECT stage FROM task_orchestrations WHERE root_issue_id = $1")
            .bind(failed_issue)
            .fetch_one(&pool)
            .await
            .expect("failed orchestration stage");
    assert_eq!(
        stage, "executing",
        "a failed run must not complete the task"
    );
}

#[sqlx::test]
async fn invalid_reporting_and_dependency_cycles_are_rejected(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let (app, _state) = build_router(&pool).await;
    let issue = {
        let (status, body) = send(
            &app,
            Method::POST,
            format!("/companies/{}/issues", fixture.company_id),
            &board_actor(&fixture),
            json!({ "title": "Cycle rejection", "status": "todo" }),
        )
        .await;
        assert!(status.is_success(), "{status} {body}");
        issue_id(&body)
    };

    // A reporting edge to an agent outside the task team is rejected.
    let bad_reporting = json!({
        "contractVersion": 1,
        "assessment": "x",
        "complexity": "complex",
        "rationale": "x",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills",
              "reportsToAgentId": fixture.worker_id },
        ],
        "subtasks": [],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, bad_reporting).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // A dependency cycle inside the contract is rejected.
    let cyclic = json!({
        "contractVersion": 1,
        "assessment": "x",
        "complexity": "complex",
        "rationale": "x",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [
            { "key": "a", "title": "A", "description": "A",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "ok",
              "blockedBy": ["b"] },
            { "key": "b", "title": "B", "description": "B",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "ok",
              "blockedBy": ["a"] }
        ],
        "questions": [],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, cyclic).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Asking a question and planning subtasks at the same time is rejected.
    let mixed = json!({
        "contractVersion": 1,
        "assessment": "x",
        "complexity": "complex",
        "rationale": "x",
        "ceoExecutes": false,
        "members": [
            { "agentId": fixture.worker_id, "action": "reuse", "role": "analyst",
              "responsibilities": "analysis", "reason": "skills" },
        ],
        "subtasks": [
            { "key": "a", "title": "A", "description": "A",
              "ownerAgentId": fixture.worker_id, "acceptanceCriteria": "ok" }
        ],
        "questions": [
            { "question": "Which?", "whyItBlocks": "Scope" }
        ],
    });
    let (status, body) = submit_contract(&app, &fixture, issue, mixed).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // The orchestration is still usable after rejected contracts.
    let (status, body) = submit_contract(&app, &fixture, issue, simple_contract()).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}
