//! HTTP parity integration tests for the built-in agent surface (#built-in).
//!
//! Covers the `/companies/:companyId/built-in-agents` router: every `:key`
//! route must resolve its path parameters (the `CompanyIdOrShortname`
//! extractor consumes the whole param map, so a `Path<String>` extractor on a
//! two-parameter route rejected every request), and provisioning must persist
//! the bundle plus a canonical `company_skills.key` rather than migration 71's
//! random `legacy/<uuid>` default.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test built_in_agents_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::built_in_agents::built_in_agent_routes;
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


mod common;
use common::connect_and_migrate;


async fn seed_company(pool: &PgPool) -> Uuid {
    let company_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Built-in Parity Co")
        .bind(format!("BI{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    company_id
}

async fn cleanup_company(pool: &PgPool, company_id: Uuid) {
    let _ = sqlx::query("DELETE FROM builtin_managed_resources WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM skill_files WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM company_skills WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(pool)
        .await;
}

/// A `:key` route resolves its path params, and provisioning persists both the
/// instructions bundle and the canonical company skill key.
#[tokio::test]
async fn built_in_agent_key_routes_resolve_and_provision_canonical_skill_key() {
    let pool = connect_and_migrate().await;
    let company_id = seed_company(&pool).await;
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    let app = built_in_agent_routes().with_state(state);
    let board = AuthorizationActor::board(Uuid::new_v4(), company_id);
    let base = format!("/companies/{company_id}/built-in-agents");

    // 1. Provision the summarizer's bundled resources.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("{base}/summarizer/provision"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "provision → 200: {}",
        String::from_utf8_lossy(&body)
    );
    let provisioned = parse(&body);
    let agent_id = provisioned["agent"]["id"]
        .as_str()
        .expect("provision returns the agent")
        .to_string();

    // 2. The status route used to 500 with
    //    "Wrong number of path arguments for `Path`".
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("{base}/summarizer/status"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "status → 200: {}",
        String::from_utf8_lossy(&body)
    );
    assert_eq!(parse(&body)["definition"]["key"], "summarizer");

    // 3. Reconcile resolves too, and is idempotent for the bundle.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("{base}/summarizer/reconcile"),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "reconcile → 200: {}",
        String::from_utf8_lossy(&body)
    );

    // 4. The provisioned skill carries its canonical key, not the random
    //    `legacy/<uuid>` default.
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT key FROM company_skills WHERE company_id = $1 ORDER BY key")
            .bind(company_id)
            .fetch_all(&pool)
            .await
            .expect("read company skill keys");
    assert!(
        keys.iter().any(|k| k == "parrot/bundled/summarize-status"),
        "canonical key persisted, got {keys:?}"
    );
    assert!(
        keys.iter().all(|k| !k.starts_with("legacy/")),
        "no skill falls back to the random legacy key, got {keys:?}"
    );

    // 5. The materialized instructions bundle reads back flat, so the
    //    Instructions tab is not blanked by a 400.
    let agent_uuid: Uuid = agent_id.parse().expect("agent id is a uuid");
    let stored_bundle: Value =
        sqlx::query_scalar("SELECT metadata->'instructionsBundle' FROM agents WHERE id = $1")
            .bind(agent_uuid)
            .fetch_one(&pool)
            .await
            .expect("read stored bundle");
    assert!(
        stored_bundle.get("instructions").is_none(),
        "bundle is persisted flat, got {stored_bundle}"
    );
    assert!(
        stored_bundle["files"].is_object(),
        "bundle carries a files object, got {stored_bundle}"
    );

    // 6. Unknown keys still 404 rather than 500.
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("{base}/not_a_key/status"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown key → 404");

    cleanup_company(&pool, company_id).await;
}
