//! HTTP parity integration tests for Paperclip's Company Skills (#124).
//!
//! Covers the company-scoped skill surface: list (with version / source /
//! install stats / release status fields), detail, versions, categories, and
//! company access enforcement (Paperclip `assertCompanyAccess`).
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test skills_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::skills::skill_routes;
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
    skill_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let skill_a = Uuid::new_v4();
    let prefix = format!("SK{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Skills Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query(
        "INSERT INTO company_skills \
            (id, company_id, name, slug, description, category, version, status, \
             update_available, latest_version, install_count) \
         VALUES ($1, $2, 'Release cut skill', 'release-cut', 'Cut the release.', \
                 'releases', '2.1.0', 'active', true, '2.2.0', 7)",
    )
    .bind(skill_a)
    .bind(company_a)
    .execute(pool)
    .await
    .expect("insert company skill");

    Fixture {
        pool: pool.clone(),
        company_a,
        skill_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM company_skills WHERE company_id = $1")
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
        .expect("connect database for skills HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #124 company-skills acceptance.
#[tokio::test]
async fn company_skills_list_detail_versions_and_authz_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = skill_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // 1. List carries version / source / release status / install stats.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/skills", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "skill list → 200");
    let list = parse(&body);
    let skills = list.as_array().expect("list is an array");
    assert_eq!(skills.len(), 1);
    let skill = &skills[0];
    assert_eq!(skill["name"], "Release cut skill");
    assert_eq!(skill["slug"], "release-cut");
    assert_eq!(skill["version"], "2.1.0", "current version");
    assert_eq!(skill["latestVersion"], "2.2.0");
    assert_eq!(skill["updateAvailable"], true);
    assert_eq!(skill["status"], "active", "release status");
    assert_eq!(skill["category"], "releases");
    assert_eq!(skill["installCount"], 7, "install statistics (migration 54)");

    // 2. Detail returns the same projection.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/skills/{}", f.company_a, f.skill_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "skill detail → 200");
    let detail = parse(&body);
    assert_eq!(detail["id"], f.skill_a.to_string());
    assert_eq!(detail["installCount"], 7);

    // 3. Versions list is reachable (empty here).
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/skills/{}/versions", f.company_a, f.skill_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "skill versions → 200");

    // 4. Categories are company-scoped and reachable.
    let (status, _) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/skills/categories", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "skill categories → 200");

    // 5. Cross-company board cannot list (403, Paperclip assertCompanyAccess).
    let outsider = session_board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/skills", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company skill list → 403");

    // 6. Cross-company detail and versions are also denied.
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/skills/{}", f.company_a, f.skill_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company skill detail → 403");
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/skills/{}/versions", f.company_a, f.skill_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company skill versions → 403");

    cleanup_fixture(&f).await;
}
