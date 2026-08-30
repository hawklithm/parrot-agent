//! HTTP parity integration tests for Paperclip's Issue Work Product routes.
//!
//! Stands up the real Axum router (`work_product_routes()`) wired to a real
//! `AppState` built by `parrot_server::build_app_state` against a live
//! PostgreSQL pool. The global auth middleware is bypassed: each request
//! injects an `AuthorizationActor` via a request extension, exactly as the auth
//! middleware would after resolving the caller.
//!
//! These tests prove the production `PgWorkProductService` is wired (not the
//! `MockWorkProductService` used only in unit fixtures) and that the route
//! behaviour — status codes, validation, company scoping, run attribution and
//! primary-mutex — matches Paperclip.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test work_product_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::work_products::work_product_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

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
    company_b: Uuid,
    issue_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let issue_a = Uuid::new_v4();
    let prefix_a = format!("RT{}", &company_a.simple().to_string()[..8]);
    let prefix_b = format!("RT{}", &company_b.simple().to_string()[..8]);

    for (id, name, prefix) in [
        (company_a, "Work Product Parity A", prefix_a),
        (company_b, "Work Product Parity B", prefix_b),
    ] {
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(prefix)
            .execute(pool)
            .await
            .expect("insert company");
    }
    sqlx::query("INSERT INTO issues (id, company_id, title, identifier) VALUES ($1, $2, $3, $4)")
        .bind(issue_a)
        .bind(company_a)
        .bind("Work product parity issue")
        .bind(format!("{}-1", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert issue");

    Fixture {
        pool: pool.clone(),
        company_a,
        company_b,
        issue_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issue_work_products WHERE company_id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(f.issue_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = ANY($1)")
        .bind(&[f.company_a, f.company_b])
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

#[tokio::test]
async fn work_product_crud_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let app = Router::new().merge(work_product_routes());
    let app = app.with_state(build_app_state(pool.clone()).await.unwrap());
    let actor = session_board_actor(Uuid::new_v4(), f.company_a);

    // Create -> 201 with normalized title/summary and default status.
    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({
            "type": "pull_request",
            "provider": "github",
            "title": "Add migrations",
            "url": "https://github.com/example/repo/pull/1",
            "status": "active",
            "reviewState": "needs_board_review",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create body: {}", String::from_utf8_lossy(&body));
    let created = parse(&body);
    let wp_id = created["id"].as_str().expect("work product id").to_string();
    assert_eq!(created["type"], "pull_request");
    assert_eq!(created["provider"], "github");
    assert_eq!(created["status"], "active");
    assert_eq!(created["reviewState"], "needs_board_review");
    assert_eq!(created["issueId"], f.issue_a.to_string());
    assert_eq!(created["companyId"], f.company_a.to_string());


    // List -> 200 and contains the created product.
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/work-products", f.issue_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = parse(&body);
    assert!(list.as_array().unwrap().iter().any(|w| w["id"] == created["id"]));

    // Update -> 200 and reflects the new status/reviewState.
    let (status, body) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/work-products/{}", wp_id),
        Some(json!({ "status": "merged", "reviewState": "approved" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "update body: {}", String::from_utf8_lossy(&body));
    let updated = parse(&body);
    assert_eq!(updated["status"], "merged");
    assert_eq!(updated["reviewState"], "approved");

    // Delete -> 204.
    let (status, _) = send(
        &app,
        &actor,
        "DELETE",
        &format!("/work-products/{}", wp_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Gone after delete.
    let (status, _) = send(
        &app,
        &actor,
        "PATCH",
        &format!("/work-products/{}", wp_id),
        Some(json!({ "status": "open" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup_fixture(&f).await;
}

#[tokio::test]
async fn work_product_validation_rejects_bad_enum() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let app = Router::new().merge(work_product_routes());
    let app = app.with_state(build_app_state(pool.clone()).await.unwrap());
    let actor = session_board_actor(Uuid::new_v4(), f.company_a);

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({
            "type": "not_a_real_type",
            "provider": "github",
            "title": "bad type",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup_fixture(&f).await;
}

#[tokio::test]
async fn work_product_is_company_scoped() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let app = Router::new().merge(work_product_routes());
    let app = app.with_state(build_app_state(pool.clone()).await.unwrap());

    // Board actor from company B must not see or mutate company A's work products.
    let actor_b = session_board_actor(Uuid::new_v4(), f.company_b);
    let (status, body) = send(
        &app,
        &actor_b,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({ "type": "pull_request", "provider": "github", "title": "cross company" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {}", String::from_utf8_lossy(&body));

    cleanup_fixture(&f).await;
}

#[tokio::test]
async fn work_product_primary_mutex_enforced() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let app = Router::new().merge(work_product_routes());
    let app = app.with_state(build_app_state(pool.clone()).await.unwrap());
    let actor = session_board_actor(Uuid::new_v4(), f.company_a);
    // Two primary work products of the same type: the service demotes the
    // earlier one rather than rejecting (Paperclip-style primary mutex).
    let first = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({ "type": "artifact", "provider": "parrot", "title": "primary one", "isPrimary": true })),
    )
    .await;
    assert_eq!(first.0, StatusCode::CREATED);
    let first_id = parse(&first.1)["id"].as_str().unwrap().to_string();
    assert_eq!(parse(&first.1)["isPrimary"], true);

    let second = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({ "type": "artifact", "provider": "parrot", "title": "primary two", "isPrimary": true })),
    )
    .await;
    assert_eq!(second.0, StatusCode::CREATED, "body: {}", String::from_utf8_lossy(&second.1));

    // Exactly one of the two is primary after the second create demotes the first.
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/work-products", f.issue_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let list = parse(&body);
    let primaries = list
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["isPrimary"] == true)
        .count();
    assert_eq!(primaries, 1, "exactly one primary expected: {}", String::from_utf8_lossy(&body));

    // Demoting via PATCH frees the slot for a third primary.
    let demote = send(
        &app,
        &actor,
        "PATCH",
        &format!("/work-products/{}", first_id),
        Some(json!({ "isPrimary": false })),
    )
    .await;
    assert_eq!(demote.0, StatusCode::OK);

    let third = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/work-products", f.issue_a),
        Some(json!({ "type": "artifact", "provider": "parrot", "title": "primary three", "isPrimary": true })),
    )
    .await;
    assert_eq!(third.0, StatusCode::CREATED);

    cleanup_fixture(&f).await;
}
