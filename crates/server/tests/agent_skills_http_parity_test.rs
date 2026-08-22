//! HTTP parity integration tests for Adapter runtime skills (#129).
//!
//! `POST /agents/:id/skills/sync` persists the desired skill set into the
//! agent's `adapter_config.desired_skills` and returns the materialized
//! snapshot (`GET /agents/:id/skills`); config-revision rollback restores the
//! adapter_config (and therefore the desired skills).
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test agent_skills_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use sqlx::Row;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::agents::agent_routes;
use parrot_server::build_app_state;
use services::auth::{ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};

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

fn session_board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        )],
        false,
    )
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
        .bind("Agent Skills Parity Co")
        .bind(format!("AS{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Skills Worker")
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
        .expect("connect database for agent skills HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #129 adapter runtime skills materialize / sync / rollback acceptance.
#[tokio::test]
async fn agent_skills_sync_materialize_and_rollback_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = agent_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let sync_uri = format!("/agents/{}/skills/sync", f.agent_a);

    // 1. Sync persists the desired skills and returns the materialized snapshot.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &sync_uri,
        Some(json!({ "desiredSkills": ["docs.read"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "skill sync → 200");
    let snapshot = parse(&body);
    assert_eq!(snapshot["desiredSkills"], json!(["docs.read"]));
    let entries = snapshot["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 1, "one materialized entry");
    assert_eq!(entries[0]["key"], "docs.read");
    assert_eq!(entries[0]["managed"], true);

    // The adapter config on disk now carries the desired set.
    let stored: Value = sqlx::query_scalar(
        "SELECT adapter_config FROM agents WHERE id = $1",
    )
    .bind(f.agent_a)
    .fetch_one(&f.pool)
    .await
    .expect("read adapter_config");
    assert_eq!(stored["desired_skills"], json!(["docs.read"]), "sync persisted");

    // 2. A follow-up sync replaces the set (update, not append).
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &sync_uri,
        Some(json!({ "desiredSkills": ["docs.read", "web.search"] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let snapshot = parse(&body);
    assert_eq!(
        snapshot["desiredSkills"],
        json!(["docs.read", "web.search"]),
        "sync replaces the desired set"
    );

    // 3. GET returns the materialized snapshot.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/{}/skills", f.agent_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get skills → 200");
    let snapshot = parse(&body);
    assert_eq!(snapshot["desiredSkills"], json!(["docs.read", "web.search"]));

    // 4. Cross-company actor cannot read or sync (agent:read / agent:update).
    let outsider = session_board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/agents/{}/skills", f.agent_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company read → 403");
    let (status, _) = send(
        &app,
        &outsider,
        "POST",
        &sync_uri,
        Some(json!({ "desiredSkills": ["docs.read"] })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company sync → 403");

    cleanup_fixture(&f).await;
}
