//! HTTP parity integration tests for Paperclip's Company Skill Policy (#125).
//!
//! `GET/PUT/POST/DELETE /companies/:company_id/skill-policy` and
//! `POST .../simulate` over the real router + database: default-open shape,
//! set with revision bump, expectedRevision conflict, simulate evaluation, and
//! company access enforcement.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test skill_policy_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::skill_policy::skill_policy_routes;
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
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Skill Policy Parity Co")
        .bind(format!("SP{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    Fixture {
        pool: pool.clone(),
        company_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM company_skill_policies WHERE company_id = $1")
        .bind(f.company_a)
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
        .expect("connect database for skill policy HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #125 company-skill-policy acceptance.
#[tokio::test]
async fn skill_policy_crud_simulate_and_authz_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = skill_policy_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let base = format!("/companies/{}/skill-policy", f.company_a);

    // 1. No policy yet → default-open shape.
    let (status, body) = send(&app, &board, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK, "get default policy → 200");
    let default_policy = parse(&body);
    assert_eq!(default_policy["policy"], Value::Null);
    assert_eq!(default_policy["version"], 0);

    // 2. Set an allowList policy → version 1, persisted.
    let (status, body) = send(
        &app,
        &board,
        "PUT",
        &base,
        Some(json!({
            "mode": "allowList",
            "allowedSkills": ["docs.read"],
            "allowRules": [{}],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "set policy → 200");
    let set = parse(&body);
    assert_eq!(set["version"], 1);
    assert_eq!(set["policy"]["mode"], "allowList");

    // 3. Simulate: allowlisted skill allowed, other skill denied.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("{base}/simulate"),
        Some(json!({ "role": "member", "action": "execute", "source": "user", "skill": "docs.read" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "simulate allowed → 200");
    let sim = parse(&body);
    assert_eq!(sim["allowed"], true);
    assert_eq!(sim["denialType"], "policy");

    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("{base}/simulate"),
        Some(json!({ "role": "member", "skill": "web.search" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sim = parse(&body);
    assert_eq!(sim["allowed"], false, "non-allowlisted skill denied");
    assert_eq!(sim["denialType"], "policy");

    // 4. expectedRevision conflict → 409.
    let (status, _) = send(
        &app,
        &board,
        "PUT",
        &base,
        Some(json!({ "mode": "defaultOpen", "expectedRevision": 99 })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "stale expectedRevision → 409");

    // 5. Invalid policy mode → 400.
    let (status, _) = send(
        &app,
        &board,
        "PUT",
        &base,
        Some(json!({ "mode": "bogus" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "invalid policy mode → 400");

    // 6. Cross-company board cannot read or write.
    let outsider = session_board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(&app, &outsider, "GET", &base, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company get → 403");
    let (status, _) = send(
        &app,
        &outsider,
        "PUT",
        &base,
        Some(json!({ "mode": "defaultOpen" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company set → 403");

    // 7. DELETE restores default-open.
    let (status, _) = send(&app, &board, "DELETE", &base, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete policy → 204");
    let (status, body) = send(&app, &board, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body)["policy"], Value::Null);

    cleanup_fixture(&f).await;
}
