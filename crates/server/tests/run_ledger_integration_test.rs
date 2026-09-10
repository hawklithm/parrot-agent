//! HTTP parity integration tests for Run Ledger / heartbeat_runs endpoints.
//!
//! Covers:
//! - GET  /companies/:id/heartbeat-runs — list runs
//! - GET  /heartbeat-runs/:run_id — get single run (X3)
//! - POST /heartbeat-runs/:run_id/cancel — cancel a run (X4)
//! - Cross-company isolation
//! - Token cost tracking via context_snapshot
//!
//! L576 — Run Ledger end-to-end verification.
//! Paperclip source: packages/server/src/routes/heartbeat_runs.ts

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::heartbeat_runs::heartbeat_run_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@localhost:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let prefix = format!("RL{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("RunLedgerTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("rl{}@test.com", user_id))
    .bind("RL User")
    .execute(pool)
    .await
    .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind("user")
    .bind(user_id)
    .bind("owner")
    .bind("active")
    .execute(pool)
    .await
    .expect("insert membership");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("RL Agent")
    .bind("http")
    .bind("idle")
    .execute(pool)
    .await
    .expect("insert agent");

    Fixture {
        pool: pool.clone(),
        company_id,
        user_id,
        agent_id,
    }
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        fixture.user_id,
        fixture.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            fixture.company_id,
            PrincipalType::User,
            fixture.user_id,
            MembershipRole::Owner,
        )],
        true,
    )
}

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

fn parse_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

// ---------------------------------------------------------------------------
// RL1: List heartbeat runs for empty company returns 200 with empty array
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_heartbeat_runs_empty() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/heartbeat-runs", f.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200 OK, got {}", status);
    let json = parse_json(&body);
    assert!(json.is_array(), "response should be a JSON array, got: {}", json);
    assert_eq!(json.as_array().unwrap().len(), 0, "expected empty array");
}

// ---------------------------------------------------------------------------
// RL2: Create heartbeat runs and list them with token cost tracking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_heartbeat_runs_with_token_costs() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let now = chrono::Utc::now();

    // Run 1: completed with token usage
    let run_1_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs \
         (id, company_id, agent_id, status, started_at, completed_at, \
          input_tokens, output_tokens, cached_input_tokens, total_cost_usd, context_snapshot)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(run_1_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("completed")
    .bind(now)
    .bind(now + chrono::Duration::seconds(30))
    .bind(1200)
    .bind(800)
    .bind(500)
    .bind(0.015)
    .bind(json!({"issue_id": "test-issue-1", "step": "plan"}))
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run 1");

    // Run 2: failed with error code
    let run_2_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs \
         (id, company_id, agent_id, status, started_at, completed_at, \
          input_tokens, output_tokens, cached_input_tokens, total_cost_usd, error_code)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(run_2_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("failed")
    .bind(now - chrono::Duration::hours(1))
    .bind(now - chrono::Duration::seconds(30))
    .bind(500)
    .bind(200)
    .bind(0)
    .bind(0.005)
    .bind("TIMEOUT")
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run 2");

    // Run 3: running
    let run_3_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs \
         (id, company_id, agent_id, status, started_at, context_snapshot)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(run_3_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("running")
    .bind(now - chrono::Duration::minutes(5))
    .bind(json!({"issue_id": "test-issue-2", "step": "execute"}))
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run 3");

    // List should return 3 runs
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/heartbeat-runs", f.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert!(json.is_array());
    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 3, "expected 3 heartbeat runs, got {}", items.len());

    // Verify token cost fields are present
    let run_1_json = items
        .iter()
        .find(|r| r.get("id").and_then(Value::as_str) == Some(&run_1_id.to_string()))
        .expect("run 1 not found");
    assert_eq!(run_1_json["inputTokens"], 1200);
    assert_eq!(run_1_json["outputTokens"], 800);
    assert_eq!(run_1_json["cachedInputTokens"], 500);
    assert!((run_1_json["totalCostUsd"].as_f64().unwrap() - 0.015).abs() < 0.0001);
}

// ---------------------------------------------------------------------------
// RL3: Get single heartbeat run by ID (X3 endpoint)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_single_heartbeat_run() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO heartbeat_runs \
         (id, company_id, agent_id, status, started_at, completed_at, \
          input_tokens, output_tokens, total_cost_usd)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(run_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("completed")
    .bind(now)
    .bind(now + chrono::Duration::seconds(10))
    .bind(100)
    .bind(50)
    .bind(0.001)
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run");

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200, got {}", status);
    let json = parse_json(&body);
    assert_eq!(json["id"], run_id.to_string());
    assert_eq!(json["companyId"], f.company_id.to_string());
    assert_eq!(json["agentId"], f.agent_id.to_string());
    assert_eq!(json["status"], "completed");
}

// ---------------------------------------------------------------------------
// RL4: Get heartbeat run for non-existent run returns 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_heartbeat_run_not_found() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let fake_run_id = Uuid::new_v4();

    let (status, _) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", fake_run_id),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404 for non-existent run, got {}",
        status
    );
}

// ---------------------------------------------------------------------------
// RL5: Cross-company isolation — company B cannot see company A's runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_heartbeat_runs_company_isolation() {
    let pool = connect_and_migrate().await;

    // Seed company A
    let company_a_id = Uuid::new_v4();
    let user_a_id = Uuid::new_v4();
    let agent_a_id = Uuid::new_v4();
    let prefix_a = format!("CA{}", &company_a_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a_id)
        .bind("Company A")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company A");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_a_id)
    .bind(format!("a{}@test.com", user_a_id))
    .bind("User A")
    .execute(&pool)
    .await
    .expect("insert user A");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_a_id)
    .bind("user")
    .bind(user_a_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership A");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(agent_a_id)
    .bind(company_a_id)
    .bind("Agent A")
    .bind("http")
    .bind("idle")
    .execute(&pool)
    .await
    .expect("insert agent A");

    let actor_a = AuthorizationActor::board_with_source(
        user_a_id,
        company_a_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_a_id,
            PrincipalType::User,
            user_a_id,
            MembershipRole::Owner,
        )],
        true,
    );

    // Insert a run in company A
    let run_a_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, started_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(run_a_id)
    .bind(company_a_id)
    .bind(agent_a_id)
    .bind("completed")
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await
    .expect("insert run A");

    let app_state_a = build_app_state(pool.clone()).await.unwrap();
    let app_a = heartbeat_run_routes().with_state(app_state_a);

    let (status, body) = send(
        &app_a,
        &actor_a,
        "GET",
        &format!("/companies/{}/heartbeat-runs", company_a_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert_eq!(json.as_array().unwrap().len(), 1);

    // Verify we can also GET the run directly
    let (status, _) = send(
        &app_a,
        &actor_a,
        "GET",
        &format!("/heartbeat-runs/{}", run_a_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get run A should succeed");

    // Now seed company B
    let company_b_id = Uuid::new_v4();
    let user_b_id = Uuid::new_v4();
    let prefix_b = format!("CB{}", &company_b_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_b_id)
        .bind("Company B")
        .bind(&prefix_b)
        .execute(&pool)
        .await
        .expect("insert company B");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_b_id)
    .bind(format!("b{}@test.com", user_b_id))
    .bind("User B")
    .execute(&pool)
    .await
    .expect("insert user B");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_b_id)
    .bind("user")
    .bind(user_b_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership B");

    let actor_b = AuthorizationActor::board_with_source(
        user_b_id,
        company_b_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_b_id,
            PrincipalType::User,
            user_b_id,
            MembershipRole::Owner,
        )],
        true,
    );

    let app_state_b = build_app_state(pool.clone()).await.unwrap();
    let app_b = heartbeat_run_routes().with_state(app_state_b);

    // Company B should see 0 runs
    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/companies/{}/heartbeat-runs", company_b_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert_eq!(
        json.as_array().unwrap().len(),
        0,
        "company B should see 0 heartbeat runs"
    );

    // Company B should NOT be able to access company A's run
    let (status, _) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/heartbeat-runs/{}", run_a_id),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "company B should not see company A's run"
    );
}

// ---------------------------------------------------------------------------
// RL6: Cancel heartbeat run returns 200 and updates status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cancel_heartbeat_run() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, started_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(run_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("running")
    .bind(now)
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run");

    let (status, _body) = send(
        &app,
        &actor,
        "POST",
        &format!("/heartbeat-runs/{}/cancel", run_id),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200, got {}",
        status
    );

    // Verify the run was cancelled
    let cancelled_status: String = sqlx::query_scalar(
        "SELECT status FROM heartbeat_runs WHERE id = $1",
    )
    .bind(run_id)
    .fetch_one(&f.pool)
    .await
    .expect("query run status");

    assert_eq!(cancelled_status, "cancelled");
}

// ---------------------------------------------------------------------------
// RL7: Heartbeat run ledger aggregates token costs across runs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_ledger_token_aggregation() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let now = chrono::Utc::now();

    // Insert multiple runs with varying token costs
    for i in 0..5 {
        let run_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO heartbeat_runs \
             (id, company_id, agent_id, status, started_at, completed_at, \
              input_tokens, output_tokens, cached_input_tokens, total_cost_usd)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(run_id)
        .bind(f.company_id)
        .bind(f.agent_id)
        .bind("completed")
        .bind(now - chrono::Duration::hours(i))
        .bind(now - chrono::Duration::hours(i) + chrono::Duration::minutes(30))
        .bind(100 + i * 50)
        .bind(50 + i * 25)
        .bind(10 + i * 5)
        .bind(0.001 + (i as f64) * 0.0005)
        .execute(&f.pool)
        .await
        .expect(&format!("insert heartbeat run {}", i));
    }

    // List should return 5 runs
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/heartbeat-runs", f.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 5, "expected 5 heartbeat runs");

    // Verify token aggregation
    let total_input: u64 = items.iter().map(|r| r["inputTokens"].as_u64().unwrap()).sum();
    let total_output: u64 = items.iter().map(|r| r["outputTokens"].as_u64().unwrap()).sum();
    let total_cost: f64 = items.iter().map(|r| r["totalCostUsd"].as_f64().unwrap()).sum();

    // 100+150+200+250+300 = 1000 input, 50+75+100+125+150 = 500 output
    assert_eq!(total_input, 1000, "total input tokens mismatch");
    assert_eq!(total_output, 500, "total output tokens mismatch");
    assert!((total_cost - 0.005).abs() < 0.0001, "total cost mismatch: {}", total_cost);
}

// ---------------------------------------------------------------------------
// RL8: Error codes persist in heartbeat_runs (migration 58 feature)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_error_code_persistence() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(state);
    let actor = owner_actor(&f);

    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        "INSERT INTO heartbeat_runs \
         (id, company_id, agent_id, status, started_at, completed_at, error_code)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind("failed")
    .bind(now)
    .bind(now + chrono::Duration::seconds(10))
    .bind("AUTH_EXPIRED")
    .execute(&f.pool)
    .await
    .expect("insert failed run");

    // GET the run directly
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert_eq!(json["status"], "failed");
    assert_eq!(json["errorCode"], "AUTH_EXPIRED");
}
