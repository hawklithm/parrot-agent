//! HTTP parity tests for the agent `urlKey` projection.
//!
//! Paperclip persists no `url_key` column: every agent read path derives it from
//! the name (falling back to the id) and the route resolver matches on that same
//! value. This suite pins the three observable contracts:
//!
//!   1. agent list / detail responses carry `urlKey` (the frontend `Agent` type
//!      declares it as required, and `api/agents.ts` resolves strictly on it);
//!   2. `GET /agents/:urlKey?companyId=…` resolves the same agent the list
//!      reports, including names whose URL key differs from the raw name;
//!   3. resolution honours paperclip's lookup rules: terminated agents are not
//!      addressable by key, an unknown key is a 404, and a missing `companyId`
//!      for a non-UUID reference is a 422.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test agent_url_key_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::agents::agent_routes;
use parrot_server::build_app_state;
use services::auth::AuthorizationActor;

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = Body::empty();
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

fn find_agent<'a>(body: &'a Value, id: Uuid) -> &'a Value {
    body.as_array()
        .expect("agent list must be an array")
        .iter()
        .find(|agent| agent["id"] == json!(id))
        .expect("seeded agent must appear in the list")
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    /// "Chief of Staff!" — normalizes to `chief-of-staff`, so the key is not the
    /// raw lowercased name.
    agent_punctuated: Uuid,
    /// A second agent sharing the first one's normalized key.
    agent_ambiguous: Uuid,
    /// Terminated: present in the table, not addressable by key.
    agent_terminated: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let agent_punctuated = Uuid::new_v4();
    let agent_ambiguous = Uuid::new_v4();
    let agent_terminated = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Agent URL Key Co")
        .bind(format!("AK{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");

    // `agent_ambiguous` starts terminated so the earlier steps resolve a unique
    // key; the ambiguity test reactivates it.
    for (id, name, status) in [
        (agent_punctuated, "Chief of Staff!", "idle"),
        (agent_ambiguous, "chief---of--staff", "terminated"),
        (agent_terminated, "Retired Worker", "terminated"),
    ] {
        sqlx::query("INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(company_id)
            .bind(name)
            .bind(status)
            .execute(pool)
            .await
            .expect("insert agent");
    }

    Fixture {
        pool: pool.clone(),
        company_id,
        agent_punctuated,
        agent_ambiguous,
        agent_terminated,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}


mod common;
use common::connect_and_migrate;


/// The list projection emits `urlKey`, and the detail route resolves it.
#[tokio::test]
async fn agent_list_emits_url_key_and_resolves_by_it() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = agent_routes().with_state(state);
    let board = AuthorizationActor::board(Uuid::new_v4(), f.company_id);

    // 1. The list carries a urlKey derived from the name, not the raw name.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/agents", f.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent list → 200");
    let listed_body = parse(&body);
    let listed = find_agent(&listed_body, f.agent_punctuated);
    assert_eq!(
        listed["urlKey"], json!("chief-of-staff"),
        "urlKey is normalized: trim → lowercase → collapse non-alphanumerics → trim dashes"
    );

    // 2. The key from the list resolves back through the reference route.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/chief-of-staff?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "urlKey lookup → 200");
    let resolved = parse(&body);
    assert_eq!(
        resolved["id"],
        json!(f.agent_punctuated),
        "the key resolves to the same agent the list reported"
    );
    assert_eq!(
        resolved["urlKey"], json!("chief-of-staff"),
        "the detail projection carries the same urlKey"
    );

    // 3. A raw name with trailing punctuation resolves too (the resolver
    //    normalizes the incoming reference before matching).
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/Chief%20of%20Staff!?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "raw-name reference resolves → 200");

    // 4. Two agents sharing a normalized key are ambiguous, not silently ordered.
    sqlx::query("UPDATE agents SET status = 'idle' WHERE id = $1")
        .bind(f.agent_ambiguous)
        .execute(&pool)
        .await
        .expect("reactivate the second agent sharing the normalized key");
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/chief-of-staff?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "two agents matching one key → 409"
    );

    cleanup_fixture(&f).await;
}

/// Terminated agents are excluded from the list and not addressable by urlKey.
#[tokio::test]
async fn terminated_agent_is_not_addressable_by_url_key() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = agent_routes().with_state(state);
    let board = AuthorizationActor::board(Uuid::new_v4(), f.company_id);

    // The default list omits terminated agents; the resolver agrees.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/agents", f.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let listed_body = parse(&body);
    assert!(
        listed_body
            .as_array()
            .expect("agent list must be an array")
            .iter()
            .all(|agent| agent["id"] != json!(f.agent_terminated)),
        "terminated agents are excluded from the agent list"
    );

    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/retired-worker?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "terminated agent is not resolvable by urlKey"
    );

    cleanup_fixture(&f).await;
}

/// Reference-route error contract for non-UUID references.
#[tokio::test]
async fn shortname_lookup_requires_company_id_and_404s_unknown_keys() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = agent_routes().with_state(state);
    let board = AuthorizationActor::board(Uuid::new_v4(), f.company_id);

    // No companyId → 422 (paperclip `unprocessable`).
    let (status, _) = send(&app, &board, "GET", "/agents/chief-of-staff").await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "shortname lookup without companyId → 422"
    );

    // Unknown key in a known company → 404, never an empty result.
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/no-such-agent?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown urlKey → 404");

    // A reference with no URL-safe characters normalizes to nothing → 404.
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/agents/---?companyId={}", f.company_id),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "reference that normalizes to empty → 404"
    );

    cleanup_fixture(&f).await;
}
