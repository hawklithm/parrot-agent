//! HTTP parity tests for agent API-key issuance and the responsible-user
//! binding that makes a key usable.
//!
//! Reference contract:
//!   `packages/db/src/schema/agent_api_keys.ts:14`
//!     `responsibleUserId: text("responsible_user_id")` — the binding lives on
//!     the key.
//!   `server/src/routes/agents.ts:4171-4177`
//!     `POST /agents/:id/keys` records `req.actor.userId` as the responsible
//!     user.
//!   `server/src/services/agents.ts:1055-1080`
//!     `createApiKey` refuses only `pending_approval` and `terminated`.
//!   `server/src/middleware/auth.ts:400-425`
//!     the key path admits every status except those two, and answers
//!     `RESPONSIBLE_USER_UNAVAILABLE` when the binding is missing.
//!
//! Run with a live database:
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test agent_api_key_responsible_user_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::app_state::create_router;
use api::routes::agents::agent_routes;
use parrot_server::build_app_state;
use repositories::board_api_key_repository::hash_api_key;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------


mod common;
use common::connect_and_migrate;


struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
}

/// Seeds a company, a board user, and an agent in the requested status.
async fn seed(pool: &PgPool, agent_status: &str) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let prefix = format!("AK{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("AgentKeyTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("ak{}@test.com", user_id))
        .bind("AK User")
        .execute(pool)
        .await
        .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active')",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("AK Agent")
    .bind("http")
    .bind(agent_status)
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

fn owner_actor(f: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        f.user_id,
        f.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            f.company_id,
            PrincipalType::User,
            f.user_id,
            MembershipRole::Owner,
        )],
        false,
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
    let req_body = match &body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize body"))
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
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "response must be JSON, got {:?}: {error}",
            String::from_utf8_lossy(bytes)
        )
    })
}

async fn app_for(pool: &PgPool) -> Router {
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    agent_routes().with_state(state)
}

/// Hashes a raw key the way the auth middleware does.
fn hash_key(raw: &str) -> String {
    hash_api_key(raw)
}

// ---------------------------------------------------------------------------
// AK1: issuance records the acting board user as the responsible user
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ak1_key_issuance_records_board_user_as_responsible_user() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool, "idle").await;
    let app = app_for(&pool).await;

    let (status, body) = send(
        &app,
        &owner_actor(&f),
        "POST",
        &format!("/agents/{}/keys", f.agent_id),
        Some(json!({ "name": "issuance" })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "Paperclip answers 201: {}",
        String::from_utf8_lossy(&body)
    );
    let json = parse(&body);
    assert_eq!(
        json["responsibleUserId"],
        f.user_id.to_string(),
        "the issuing board user is the responsible user: {json}"
    );

    // The same value must be persisted, not merely echoed.
    let stored: Option<Uuid> = sqlx::query_scalar(
        "SELECT responsible_user_id FROM agent_api_keys WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(f.agent_id)
    .fetch_one(&f.pool)
    .await
    .expect("load key row");
    assert_eq!(stored, Some(f.user_id));
}

/// An **idle** agent's key must authenticate. Parrot previously required
/// `status == running` and read the responsible user from `agents.reports_to`
/// (an agent self-FK), so an idle agent's key was rejected outright.
#[tokio::test]
async fn ak2_idle_and_paused_agents_can_still_use_their_keys() {
    let pool = connect_and_migrate().await;

    for status in ["idle", "paused"] {
        let f = seed(&pool, status).await;
        let app = app_for(&pool).await;
        let raw = format!("aak_{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO agent_api_keys (id, company_id, agent_id, key_hash, name, responsible_user_id) \
             VALUES ($1, $2, $3, $4, 'idle-key', $5)",
        )
        .bind(Uuid::new_v4())
        .bind(f.company_id)
        .bind(f.agent_id)
        .bind(hash_key(&raw))
        .bind(f.user_id)
        .execute(&f.pool)
        .await
        .expect("insert key");

        let full = create_router(build_app_state(pool.clone()).await.expect("state"));
        let (resolved, _) = send(
            &full,
            &AuthorizationActor::none(),
            "GET",
            "/api/auth/get-session",
            None,
        )
        .await;
        assert_eq!(
            resolved,
            StatusCode::OK,
            "router must answer the session probe"
        );

        // Send the key through the real auth path.
        let req = Request::builder()
            .method("GET")
            .uri("/api/auth/get-session")
            .header("authorization", format!("Bearer {raw}"))
            .body(Body::empty())
            .expect("build request");
        let resp = full.clone().oneshot(req).await.expect("dispatch");
        let status_code = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("body");
        let json = parse(&bytes);
        assert_eq!(
            status_code,
            StatusCode::OK,
            "{status} agent key must authenticate: {json}"
        );
        assert_eq!(
            json["session"]["source"], "agent_key",
            "{status} agent resolved via key: {json}"
        );
    }
}

/// An **idle** agent may still be issued a key, but `pending_approval` and
/// `terminated` agents may not (`services/agents.ts:1064-1069`).
#[tokio::test]
async fn ak3_key_issuance_is_refused_for_pending_and_terminated_agents() {
    let pool = connect_and_migrate().await;

    for status in ["pending_approval", "terminated"] {
        let f = seed(&pool, status).await;
        let app = app_for(&pool).await;

        let (code, body) = send(
            &app,
            &owner_actor(&f),
            "POST",
            &format!("/agents/{}/keys", f.agent_id),
            Some(json!({ "name": "refused" })),
        )
        .await;

        assert_eq!(
            code,
            StatusCode::CONFLICT,
            "{status} agent key issuance must be a 409: {}",
            String::from_utf8_lossy(&body)
        );

        let rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM agent_api_keys WHERE agent_id = $1")
                .bind(f.agent_id)
                .fetch_one(&f.pool)
                .await
                .expect("count keys");
        assert_eq!(rows, 0, "{status}: no key may be written");
    }
}

/// A key whose responsible user is missing must be refused with Paperclip's
/// `RESPONSIBLE_USER_UNAVAILABLE`, not silently resolved.
#[tokio::test]
async fn ak4_key_without_responsible_user_is_refused() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool, "idle").await;
    let app = create_router(build_app_state(pool.clone()).await.expect("state"));

    let raw = format!("aak_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO agent_api_keys (id, company_id, agent_id, key_hash, name) \
         VALUES ($1, $2, $3, $4, 'orphan')",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(hash_key(&raw))
    .execute(&f.pool)
    .await
    .expect("insert orphan key");

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/get-session")
        .header("authorization", format!("Bearer {raw}"))
        .body(Body::empty())
        .expect("build request");
    let resp = app.clone().oneshot(req).await.expect("dispatch");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("body");

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "an unbound key cannot authenticate: {}",
        String::from_utf8_lossy(&bytes)
    );
    let json = parse(&bytes);
    assert_eq!(
        json["code"], "RESPONSIBLE_USER_UNAVAILABLE",
        "clients branch on the code: {json}"
    );
}

/// `agents.reports_to` must never stand in for the responsible user: it is an
/// `agents(id)` self-reference (the org-chart manager). Setting it must not
/// make an otherwise-unbound key work.
#[tokio::test]
async fn ak5_reports_to_does_not_supply_the_responsible_user() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool, "running").await;

    let manager_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) \
         VALUES ($1, $2, 'AK Manager', 'http', 'running')",
    )
    .bind(manager_id)
    .bind(f.company_id)
    .execute(&f.pool)
    .await
    .expect("insert manager");

    sqlx::query("UPDATE agents SET reports_to = $2 WHERE id = $1")
        .bind(f.agent_id)
        .bind(manager_id)
        .execute(&f.pool)
        .await
        .expect("set reports_to");

    let raw = format!("aak_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO agent_api_keys (id, company_id, agent_id, key_hash, name) \
         VALUES ($1, $2, $3, $4, 'reports-to-only')",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(hash_key(&raw))
    .execute(&f.pool)
    .await
    .expect("insert key");

    let app = create_router(build_app_state(pool.clone()).await.expect("state"));
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/get-session")
        .header("authorization", format!("Bearer {raw}"))
        .body(Body::empty())
        .expect("build request");
    let resp = app.clone().oneshot(req).await.expect("dispatch");
    let status = resp.status();

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "reports_to is an agent, and must not resolve as the responsible user"
    );
}

/// `terminated` / `pending_approval` agents are refused at the auth layer too,
/// independently of key issuance.
#[tokio::test]
async fn ak6_key_auth_refuses_terminated_agent() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool, "terminated").await;
    let raw = format!("aak_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO agent_api_keys (id, company_id, agent_id, key_hash, name, responsible_user_id) \
         VALUES ($1, $2, $3, $4, 'terminated', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(hash_key(&raw))
    .bind(f.user_id)
    .execute(&f.pool)
    .await
    .expect("insert key");

    let app = create_router(build_app_state(pool.clone()).await.expect("state"));
    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/get-session")
        .header("authorization", format!("Bearer {raw}"))
        .body(Body::empty())
        .expect("build request");
    let resp = app.clone().oneshot(req).await.expect("dispatch");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a terminated agent's key must not authenticate"
    );
}
