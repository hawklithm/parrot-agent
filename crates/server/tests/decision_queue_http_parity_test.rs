//! HTTP parity integration tests for Paperclip's Decision Queue routes.
//!
//! These tests stand up the *real* Axum router (`decision_routes()`) wired to a
//! *real* `AppState` built by `parrot_server::build_app_state` against a live
//! PostgreSQL pool. The global auth middleware is bypassed: each request
//! injects an `AuthorizationActor` via a request extension, exactly as the auth
//! middleware would after resolving the caller. This lets us assert the route
//! behaviour matches Paperclip's `decisionQueueService` without standing up the
//! full auth stack.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test decision_queue_http_parity_test
//!
//! Mirrors `server/src/__tests__/decision-queues-routes.test.ts`.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::decisions::decision_routes;
use parrot_server::build_app_state;
use services::auth::AuthorizationActor;

/// Send one request with `actor` injected as the `AuthorizationActor` extension
/// and return `(status, body_bytes)`.
async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match body {
        Some(ref value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).expect("build request");
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

/// LocalImplicit board actor: the canonical trusted caller (single-user mode
/// bypasses company scoping, so the board sees every source).
fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, company_id)
}

/// Standard agent actor. For a `failed_run` source it owns, the agent reads via
/// the owner short-circuit in `can_read_decision_source` (no explicit grant
/// required); for a board-only source (an approval with no linked issue) it is
/// denied.
fn agent_actor(agent_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(agent_id, company_id, None)
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_a: Uuid,
    run_1: Uuid,
    run_2: Uuid,
    approval_id: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let run_1 = Uuid::new_v4();
    let run_2 = Uuid::new_v4();
    let approval_id = Uuid::new_v4();
    let prefix_a = format!("DQ{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Decision Queue Parity Co")
        .bind(prefix_a)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Decision Queue Parity Agent")
        .execute(pool)
        .await
        .expect("insert agent");

    // Two heartbeat runs owned by the agent. Status 'completed' with a NULL
    // context snapshot keeps them clear of the partial unique index on active
    // runs. The agent reads these via the failed_run owner short-circuit.
    for run in [run_1, run_2] {
        sqlx::query(
            "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'succeeded')",
        )
        .bind(run)
        .bind(company_a)
        .bind(agent_a)
        .execute(pool)
        .await
        .expect("insert heartbeat run");
    }

    // An approval with NO linked issue (no issue_approvals row) is board-only:
    // agents cannot read it, so it is hidden from agent-scoped listings.
    sqlx::query(
        "INSERT INTO approvals (id, company_id, approval_type, status, payload) VALUES ($1, $2, 'create_resource', 'pending', $3)",
    )
    .bind(approval_id)
    .bind(company_a)
    .bind(json!({}))
    .execute(pool)
    .await
    .expect("insert approval");

    Fixture {
        pool: pool.clone(),
        company_a,
        agent_a,
        run_1,
        run_2,
        approval_id,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM decision_triage_events WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_triage WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_queue_items WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM decision_queues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM approvals WHERE id = $1")
        .bind(f.approval_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.agent_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for decision queue HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

// ===========================================================================
// Decision Queue — PAPERCLIP_MIGRATION_PLAN.md #139
// ===========================================================================

/// Mirrors the Paperclip test "creates idempotently, patches, lists by updated
/// time, and audits queue mutations".
#[tokio::test]
async fn queue_create_is_idempotent_and_list_is_sorted_by_updated_desc() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = decision_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let base = format!("/companies/{}/decision-queues", f.company_a);

    // First create → 201.
    let (status, body) = send(&app, &board, "POST", &base, Some(json!({
        "key": "launches",
        "title": "Launches",
        "description": "Ship decisions",
    })))
    .await;
    assert_eq!(status, StatusCode::CREATED, "first create → 201");
    let first = parse(&body);
    let launches_id = first["id"].as_str().expect("queue id").to_string();

    // Repeated create with same key → 200 and the SAME id (idempotent), not 409.
    let (status, body) = send(&app, &board, "POST", &base, Some(json!({
        "key": "launches",
        "title": "Ignored duplicate title",
    })))
    .await;
    assert_eq!(status, StatusCode::OK, "repeated create → 200 (idempotent)");
    let repeated = parse(&body);
    assert_eq!(repeated["id"], launches_id, "repeated create returns same id");
    assert_eq!(repeated["title"], "Launches", "original title is preserved");
    assert_eq!(repeated["itemCount"], 0);

    // A later create wins the list ordering by updated_at DESC.
    let (status, _) = send(&app, &board, "POST", &base, Some(json!({
        "key": "older",
        "title": "Older",
    })))
    .await;
    assert_eq!(status, StatusCode::CREATED, "second distinct create → 201");

    let (status, body) = send(&app, &board, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK, "list → 200");
    let list = parse(&body);
    let keys: Vec<&str> = list
        .as_array()
        .expect("list is an array")
        .iter()
        .map(|q| q["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["older", "launches"], "list ordered by updated_at DESC");

    cleanup_fixture(&f).await;
}

/// Mirrors "adds and removes three attention source kinds and hides board-only
/// membership from agents" — adapted to use `failed_run` sources the agent owns
/// (so the agent has read authority without an explicit grant) plus a
/// board-only `approval` (no linked issue).
#[tokio::test]
async fn queue_items_filter_board_only_and_board_cleans_up_disappeared_source() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = decision_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let agent = agent_actor(f.agent_a, f.company_a);

    // Board creates the queue.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/decision-queues", f.company_a),
        Some(json!({ "key": "triage", "title": "Triage" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create queue → 201");
    let _queue_id = parse(&body)["id"].as_str().expect("queue id").to_string();

    let items_uri = format!("/companies/{}/decision-queues/triage/items", f.company_a);

    // Board enqueues two agent-owned failed_runs + one board-only approval.
    for (sk, sid) in [
        ("failed_run", f.run_1.to_string()),
        ("failed_run", f.run_2.to_string()),
        ("approval", f.approval_id.to_string()),
    ] {
        let (status, _) = send(
            &app,
            &board,
            "POST",
            &items_uri,
            Some(json!({ "sourceKind": sk, "sourceId": sid })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "board add {sk} → 201");
    }

    // Board sees all three items.
    let (status, body) = send(&app, &board, "GET", &items_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body).as_array().unwrap().len(), 3, "board sees 3 items");

    // Board queue list reports itemCount 3.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/decision-queues", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let board_count = parse(&body).as_array().unwrap()[0]["itemCount"].as_i64().unwrap();
    assert_eq!(board_count, 3, "board itemCount is 3");

    // Agent sees only the two failed_runs (approval is board-only) and a
    // viewer-scoped itemCount of 2.
    let (status, body) = send(&app, &agent, "GET", &items_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    let agent_items = parse(&body).as_array().unwrap().clone();
    assert_eq!(agent_items.len(), 2, "agent sees 2 items");
    assert!(
        agent_items
            .iter()
            .all(|i| i["sourceKind"] == "failed_run"),
        "agent only sees failed_run items"
    );

    let (status, body) = send(
        &app,
        &agent,
        "GET",
        &format!("/companies/{}/decision-queues", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let agent_count = parse(&body).as_array().unwrap()[0]["itemCount"].as_i64().unwrap();
    assert_eq!(agent_count, 2, "agent itemCount is 2 (board-only hidden)");

    // Agent cannot remove the board-only approval (404, not a probe).
    let (status, _) = send(
        &app,
        &agent,
        "DELETE",
        &format!(
            "/companies/{}/decision-queues/triage/items/approval/{}",
            f.company_a, f.approval_id
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "agent remove board-only → 404");

    // Agent also cannot ADD a board-only approval (source-read enforced on add).
    let (status, _) = send(
        &app,
        &agent,
        "POST",
        &items_uri,
        Some(json!({ "sourceKind": "approval", "sourceId": f.approval_id })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "agent add board-only → 404");

    // Board can remove an existing item (failed_run/run_1).
    let (status, _) = send(
        &app,
        &board,
        "DELETE",
        &format!(
            "/companies/{}/decision-queues/triage/items/failed_run/{}",
            f.company_a, f.run_1
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "board remove → 204");

    // Simulate the approval source disappearing; board may still clean up the
    // orphaned sidecar (removal does not require the source to still exist).
    sqlx::query("DELETE FROM approvals WHERE id = $1")
        .bind(f.approval_id)
        .execute(&pool)
        .await
        .expect("delete approval to simulate disappearance");
    let (status, _) = send(
        &app,
        &board,
        "DELETE",
        &format!(
            "/companies/{}/decision-queues/triage/items/approval/{}",
            f.company_a, f.approval_id
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "board cleans up disappeared source → 204");

    // Only the remaining failed_run/run_2 item is left.
    let (status, body) = send(&app, &board, "GET", &items_uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body).as_array().unwrap().len(), 1, "one item remains");

    cleanup_fixture(&f).await;
}

/// Mirrors "records agent decide-by, preserves override history" and
/// "serializes concurrent partial triage updates without losing state" —
/// adapted to a `failed_run` source the agent owns.
#[tokio::test]
async fn triage_version_increments_and_survives_concurrent_updates() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = decision_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let agent = agent_actor(f.agent_a, f.company_a);
    let triage_uri = |source_id: &Uuid| {
        format!(
            "/companies/{}/decision-triage/failed_run/{}",
            f.company_a, source_id
        )
    };

    // Agent sets triage → version 1, attributed to the agent.
    let (status, body) = send(
        &app,
        &agent,
        "PUT",
        &triage_uri(&f.run_1),
        Some(json!({ "decideBy": "today", "snoozedUntil": "2026-08-03T12:00:00.000Z" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent triage → 200");
    let agent_set = parse(&body);
    assert_eq!(agent_set["decideBy"], "today");
    assert_eq!(agent_set["setByType"], "agent");
    assert_eq!(agent_set["setByAgentId"], f.agent_a.to_string());
    assert_eq!(agent_set["version"], 1);

    // Board overrides → version 2, attributed to the user.
    let (status, body) = send(
        &app,
        &board,
        "PUT",
        &triage_uri(&f.run_1),
        Some(json!({ "decideBy": "2026-08-08", "snoozedUntil": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board override → 200");
    let overridden = parse(&body);
    assert_eq!(overridden["decideBy"], "2026-08-08");
    assert_eq!(overridden["setByType"], "user");
    assert_eq!(overridden["version"], 2);

    // Current state reflects both updates.
    let (status, body) = send(&app, &board, "GET", &triage_uri(&f.run_1), None).await;
    assert_eq!(status, StatusCode::OK);
    let current = parse(&body);
    assert_eq!(current["decideBy"], "2026-08-08");
    assert_eq!(current["version"], 2);

    // Concurrent partial updates on a fresh source serialize without losing
    // state; versions are 1 and 2 (never reused).
    let run2_uri = triage_uri(&f.run_2);
    let (r1, r2) = tokio::join!(
        send(
            &app,
            &board,
            "PUT",
            &run2_uri,
            Some(json!({ "decideBy": "today" })),
        ),
        send(
            &app,
            &board,
            "PUT",
            &run2_uri,
            Some(json!({ "snoozedUntil": "2026-08-03T12:00:00.000Z" })),
        ),
    );
    assert_eq!(r1.0, StatusCode::OK);
    assert_eq!(r2.0, StatusCode::OK);
    let v1 = parse(&r1.1)["version"].as_i64().unwrap();
    let v2 = parse(&r2.1)["version"].as_i64().unwrap();
    let mut versions = [v1, v2];
    versions.sort_unstable();
    assert_eq!(versions, [1, 2], "concurrent updates produce versions 1 and 2");

    let (status, body) = send(&app, &board, "GET", &triage_uri(&f.run_2), None).await;
    assert_eq!(status, StatusCode::OK);
    let final_state = parse(&body);
    assert_eq!(final_state["decideBy"], "today");
    assert_eq!(final_state["snoozedUntil"], "2026-08-03T12:00:00.000Z");
    assert_eq!(final_state["version"], 2);

    cleanup_fixture(&f).await;
}
