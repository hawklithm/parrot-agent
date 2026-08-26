//! HTTP parity integration tests for Paperclip's Company Search (#4C.3).
//!
//! `GET /companies/:company_id/search` returns the `CompanySearchResponse`
//! shape (issue scope: title/identifier/description full-text + token match),
//! with `q`/`scope`/`limit`/`offset`/`sort`, tenant isolation, and cross-company 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test company_search_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::company_routes;
use parrot_server::build_app_state;
use services::auth::{ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("build request");
    let mut req = req;
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
    company_b: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let prefix_a = format!("CS{}", &company_a.simple().to_string()[..8]);
    let prefix_b = format!("CS{}", &company_b.simple().to_string()[..8]);

    for (id, name) in [(company_a, "Search Parity Co A"), (company_b, "Search Parity Co B")] {
        sqlx::query(
            "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(name)
        .bind(if id == company_a { &prefix_a } else { &prefix_b })
        .execute(pool)
        .await
        .expect("insert company");
    }

    // Company A issues: one title match, one identifier match, one description match.
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Login button broken on homepage")
    .bind(format!("{}-1", prefix_a))
    .bind("unrelated body text")
    .execute(pool)
    .await
    .expect("insert title issue");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'in_progress', 'high', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Random task")
    .bind(format!("{}-loginflow", prefix_a))
    .bind("unrelated body text")
    .execute(pool)
    .await
    .expect("insert identifier issue");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'backlog', 'low', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind("Another task")
    .bind(format!("{}-3", prefix_a))
    .bind("the migration broke the login endpoint last night")
    .execute(pool)
    .await
    .expect("insert description issue");

    // Company B issue with the same keyword must NOT appear in company A search.
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_b)
    .bind("login outage on prod")
    .bind(format!("{}-1", prefix_b))
    .bind("leak")
    .execute(pool)
    .await
    .expect("insert company B issue");

    Fixture {
        pool: pool.clone(),
        company_a,
        company_b,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_b)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_b)
        .execute(&f.pool)
        .await;
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for company search HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #4C.3 Company Search acceptance — issue scope, tenant isolation, pagination, cross-company 403.
#[tokio::test]
async fn company_search_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = company_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // Title phrase match + tenant isolation (company B "login" issue excluded).
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "search → 200");
    let resp = parse(&body);
    assert_eq!(resp["query"], json!("login"));
    assert_eq!(resp["scope"], json!("all"));
    assert_eq!(resp["sort"], json!("relevance"));
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3, "title + identifier + description match, no cross-company leak");
    for r in results {
        assert_eq!(r["type"], json!("issue"));
        assert!(r["issue"].is_object(), "issue summary present");
        assert!(r["matchedFields"].as_array().unwrap().len() >= 1);
    }
    assert_eq!(resp["countsByType"]["issue"], json!(3));
    assert_eq!(resp["hasMore"], json!(false));

    // Identifier match via scoped token (loginflow).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=loginflow", f.company_a),
    )
    .await;
    let resp = parse(&body);
    let results = resp["results"].as_array().expect("results array");
    assert_eq!(results.len(), 1, "identifier token match");
    assert_eq!(results[0]["matchedFields"][0], json!("identifier"));

    // Scope=issues restricts to issue text hits (same as all for this dataset).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&scope=issues", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["scope"], json!("issues"));
    assert_eq!(resp["results"].as_array().unwrap().len(), 3);

    // Scope=agents/projects (not yet implemented) returns empty results, not an error.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&scope=agents", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["results"].as_array().unwrap().len(), 0);

    // Empty query → empty results (Paperclip contract).
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search", f.company_a),
    )
    .await;
    let resp = parse(&body);
    assert_eq!(resp["results"].as_array().unwrap().len(), 0);

    // Pagination: limit + offset.
    let (_, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&limit=1&offset=1", f.company_a),
    )
    .await;
    let resp = parse(&body);
    let page = resp["results"].as_array().expect("results array");
    assert_eq!(page.len(), 1, "limit=1");
    assert_eq!(resp["limit"], json!(1));
    assert_eq!(resp["offset"], json!(1));
    assert_eq!(resp["hasMore"], json!(true), "more results beyond offset+limit");

    // Sort=updated returns 200 and preserves the response shape.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/search?q=login&sort=updated", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(parse(&body)["sort"], json!("updated"));

    // Cross-company board cannot read company A search.
    let outsider = session_board_actor(Uuid::new_v4(), f.company_b);
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/companies/{}/search?q=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company search → 403");

    cleanup_fixture(&f).await;
}
