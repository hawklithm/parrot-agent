//! HTTP parity integration tests for Secret Proposal Review (#133).
//!
//! Agents propose secrets; board reviews them (list/detail/approve/reject)
//! and agents may withdraw their own. Secret values never appear in
//! responses (encrypted at rest, fingerprinted).
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test secret_proposals_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::secret_proposals::secret_proposal_routes;
use parrot_server::build_app_state;
use services::auth::AuthorizationActor;

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

fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, company_id)
}

fn agent_actor(agent_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(agent_id, company_id, None)
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Secret Proposals Co")
        .bind(format!("SP{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Proposing Agent")
        .execute(pool)
        .await
        .expect("insert agent");
    Fixture {
        pool: pool.clone(),
        company_a,
        agent_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM company_secret_proposals WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM company_secrets WHERE company_id = $1")
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
        .expect("connect database for secret proposal HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #133 secret proposal review acceptance.
#[tokio::test]
async fn secret_proposal_lifecycle_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = secret_proposal_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let agent = agent_actor(f.agent_a, f.company_a);

    // 1. Agent proposes a secret → 201, pending.
    let (status, body) = send(
        &app,
        &agent,
        "POST",
        "/agents/me/secret-proposals",
        Some(json!({
            "kind": "secret",
            "name": "DEPLOY_TOKEN",
            "key": "DEPLOY_TOKEN",
            "value": "s3cr3t-value",
            "justification": "Needed for the release pipeline.",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "agent create proposal → 201");
    let created = parse(&body);
    let proposal_id = created["id"].as_str().expect("proposal id").to_string();
    assert_eq!(created["status"], "pending");
    let created_text = serde_json::to_string(&created).unwrap_or_default();
    assert!(
        !created_text.contains("s3cr3t-value"),
        "secret value must not appear in the create response"
    );

    // 2. Agent lists own proposals.
    let (status, body) = send(&app, &agent, "GET", "/agents/me/secret-proposals", None).await;
    assert_eq!(status, StatusCode::OK, "agent list proposals → 200");
    let list = parse(&body);
    assert_eq!(
        list["proposals"].as_array().map(|a| a.len()).unwrap_or(0),
        1,
        "one own proposal"
    );

    // 3. Board lists company proposals and reads the detail.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/secret-proposals", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board list → 200");
    let board_list = parse(&body);
    assert_eq!(
        board_list["proposals"].as_array().map(|a| a.len()).unwrap_or(0),
        1,
        "board sees the proposal"
    );
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/secret-proposals/{}", f.company_a, proposal_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board detail → 200");
    let detail_text = serde_json::to_string(&parse(&body)).unwrap_or_default();
    assert!(
        !detail_text.contains("s3cr3t-value"),
        "secret value must not appear in the detail response"
    );

    // 4. Board approves → approved, resolved, audited.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/secret-proposals/{}/approve", f.company_a, proposal_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board approve → 200");
    let approved = parse(&body);
    assert_eq!(approved["status"], "approved");
    assert!(!approved["resolvedAt"].is_null(), "approval records resolvedAt");

    // 5. A second proposal is rejected.
    let (status, body) = send(
        &app,
        &agent,
        "POST",
        "/agents/me/secret-proposals",
        Some(json!({
            "kind": "secret",
            "name": "OLD_KEY",
            "key": "OLD_KEY",
            "value": "old-value",
            "justification": "Rotating out.",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "second proposal → 201");
    let second_id = parse(&body)["id"].as_str().expect("second id").to_string();
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/secret-proposals/{}/reject", f.company_a, second_id),
        Some(json!({ "reason": "Deprecated key." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "board reject → 200");
    let rejected = parse(&body);
    assert_eq!(rejected["status"], "rejected");

    // 6. A fresh proposal can be withdrawn by its agent.
    let (status, body) = send(
        &app,
        &agent,
        "POST",
        "/agents/me/secret-proposals",
        Some(json!({
            "kind": "secret",
            "name": "WITHDRAWN",
            "key": "WITHDRAWN",
            "value": "w-value",
            "justification": "No longer needed.",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "third proposal → 201");
    let third_id = parse(&body)["id"].as_str().expect("third id").to_string();
    let (status, body) = send(
        &app,
        &agent,
        "DELETE",
        &format!("/agents/me/secret-proposals/{third_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent withdraw → 200");
    assert_eq!(parse(&body)["status"], "withdrawn");
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/secret-proposals/{third_id}", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "withdrawn proposal still visible to board");
    assert_eq!(parse(&body)["status"], "withdrawn");

    // 7. An agent cannot list board proposals or approve.
    let (status, _) = send(
        &app,
        &agent,
        "GET",
        &format!("/companies/{}/secret-proposals", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "agent board list → 403");

    cleanup_fixture(&f).await;
}
