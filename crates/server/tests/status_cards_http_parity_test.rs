//! HTTP parity integration tests for Status Cards (#109, #110).
//!
//! The full Paperclip status-cards page family is exercised: list (with
//! archived filter), detail, create, settings (patch), archive/unarchive,
//! refresh/recompile/dry-run, query/summary writes, updates and
//! summary-revisions. Board write access gates create/patch/archive/delete
//! and cross-company callers get 403; summary writes are agent-only and must
//! match the card's generating issue.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test status_cards_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::status_cards::status_card_routes;
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
    // Session-sourced board with an Operator membership in its own company,
    // matching the other parity suites: a board has no implicit access to
    // companies it is not a member of.
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
        .bind("Status Cards Co")
        .bind(format!("SC{}", &company_a.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Status Card Agent")
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
    let _ = sqlx::query(
        "DELETE FROM status_cards WHERE company_id = $1",
    )
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
        .expect("connect database for status cards HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #109 page family + #110 refresh / archive / authz acceptance.
#[tokio::test]
async fn status_card_lifecycle_matches_paperclip() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_test_writer()
        .try_init();
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = status_card_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let agent = agent_actor(f.agent_a, f.company_a);
    // The refresh pipeline enqueues a generation through the built-in
    // summarizer agent (agents.metadata->>'builtInKey' = 'summarizer').
    sqlx::query(
        "UPDATE agents SET metadata = jsonb_build_object('builtInKey', 'summarizer') WHERE id = $1",
    )
    .bind(f.agent_a)
    .execute(&pool)
    .await
    .expect("mark summarizer built-in agent");

    // 1. Board creates a card → 201, compiling, camelCase shape.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/status-cards", f.company_a),
        Some(json!({
            "title": "Release Status",
            "titlePinned": true,
            "interestPrompt": "Track the release train.",
            "queries": [{ "kind": "issue", "filter": { "status": "in_progress" } }],
            "refreshPolicy": { "mode": "interval", "minutes": 30 },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create status card → 201");
    let created = parse(&body);
    let card_id = created["id"].as_str().expect("card id").to_string();
    assert_eq!(created["title"], "Release Status");
    assert_eq!(created["titlePinned"], true);
    assert_eq!(created["state"], "compiling");
    assert_eq!(created["queryVersion"], 0);
    assert_eq!(created["archivedAt"], Value::Null);

    // 2. List shows the card; get detail matches.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/status-cards", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list → 200");
    let list = parse(&body);
    assert_eq!(list.as_array().map(|a| a.len()).unwrap_or(0), 1, "one card listed");
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/status-cards/{card_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "detail → 200");
    let detail = parse(&body);
    assert_eq!(detail["id"], card_id);
    assert_eq!(detail["refreshPolicy"]["minutes"], 30);

    // 3. Settings: patch title / queries / refresh policy and bump version.
    let (status, body) = send(
        &app,
        &board,
        "PATCH",
        &format!("/status-cards/{card_id}"),
        Some(json!({
            "title": "Release Status v2",
            "queries": [{ "kind": "issue", "filter": { "status": "done" } }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "patch settings → 200");
    let patched = parse(&body);
    assert_eq!(patched["title"], "Release Status v2");
    assert_eq!(patched["queries"][0]["filter"]["status"], "done");

    // 4. Refresh enqueues a run; dry-run resolves queries; query write bumps
    //    the version.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/status-cards/{card_id}/refresh"),
        None,
    )
    .await;
    assert!(
        status == StatusCode::OK || status == StatusCode::ACCEPTED,
        "refresh → 200/202, got {status}"
    );
    let refreshed = parse(&body);
    assert_eq!(refreshed["card"]["id"], card_id);
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/status-cards/{card_id}/dry-run"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "dry-run → 200");
    let dry = parse(&body);
    assert_eq!(dry["cardId"], card_id);
    assert!(dry["mentionedIssues"].is_array(), "dry run resolves mentioned issues");
    // Query writes are agent-only (the agent compiles and writes the query
    // back) and must match the card's generating issue.
    let gid = Uuid::new_v4();
    sqlx::query("UPDATE status_cards SET generating_issue_id = $1 WHERE id = $2")
        .bind(gid)
        .bind(Uuid::parse_str(&card_id).expect("card uuid"))
        .execute(&pool)
        .await
        .expect("set generating issue");
    let (status, _) = send(
        &app,
        &board,
        "PUT",
        &format!("/status-cards/{card_id}/query"),
        Some(json!({ "queries": [], "generationIssueId": gid })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "board query write → 403");
    let (status, body) = send(
        &app,
        &agent,
        "PUT",
        &format!("/status-cards/{card_id}/query"),
        Some(json!({
            "queries": [{ "kind": "issue", "filter": { "status": "in_review" } }],
            "queryVersion": 1,
            "generationIssueId": gid,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent query write → 200");
    assert_eq!(parse(&body)["queryVersion"], 1, "query version bumped");

    // 5. Summary is agent-only and must match the generating issue.
    let (status, _) = send(
        &app,
        &board,
        "PUT",
        &format!("/status-cards/{card_id}/summary"),
        Some(json!({ "generationIssueId": Uuid::new_v4(), "changeSummary": "x", "summary": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "board summary write → 403");
    let (status, body) = send(
        &app,
        &agent,
        "PUT",
        &format!("/status-cards/{card_id}/summary"),
        Some(json!({
            "generationIssueId": gid,
            "changeSummary": "Ship it.",
            "summary": "## Release Status\n\nAll green.",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "agent summary write → 200");
    let written = parse(&body);
    assert!(
        written["summaryMarkdown"].as_str().unwrap_or_default().contains("All green"),
        "summary markdown persisted"
    );

    // 6. Updates and summary-revisions are listed.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/status-cards/{card_id}/summary-revisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "summary revisions → 200");
    assert!(
        parse(&body).as_array().map(|a| a.len()).unwrap_or(0) >= 1,
        "at least one summary revision"
    );
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/status-cards/{card_id}/updates"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "updates → 200");
    assert!(parse(&body).is_array(), "updates is an array");

    // 7. Archive and unarchive.
    let (status, body) = send(
        &app,
        &board,
        "PATCH",
        &format!("/status-cards/{card_id}"),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "archive → 200");
    assert!(!parse(&body)["archivedAt"].is_null(), "archivedAt set");
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/status-cards?archived=true", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "archived list → 200");
    assert_eq!(
        parse(&body).as_array().map(|a| a.len()).unwrap_or(0),
        1,
        "archived card visible with filter"
    );
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/status-cards", f.company_a),
        None,
    )
    .await;
    assert_eq!(
        parse(&body).as_array().map(|a| a.len()).unwrap_or(0),
        0,
        "default list hides archived cards"
    );
    let (status, body) = send(
        &app,
        &board,
        "PATCH",
        &format!("/status-cards/{card_id}"),
        Some(json!({ "archived": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "unarchive → 200");
    assert!(parse(&body)["archivedAt"].is_null(), "archivedAt cleared");

    // 8. Cross-company board cannot list/create/patch (403).
    let outsider = board_actor(Uuid::new_v4(), Uuid::new_v4());
    for (method, uri, body) in [
        ("GET", format!("/companies/{}/status-cards", f.company_a), None),
        (
            "POST",
            format!("/companies/{}/status-cards", f.company_a),
            Some(json!({ "title": "x" })),
        ),
        ("PATCH", format!("/status-cards/{card_id}"), Some(json!({ "title": "x" }))),
    ] {
        let (status, _) = send(&app, &outsider, method, &uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "cross-company {method} → 403");
    }

    // 9. Delete removes the card (204) and it disappears from the list.
    let (status, _) = send(
        &app,
        &board,
        "DELETE",
        &format!("/status-cards/{card_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete → 204");
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/status-cards/{card_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "deleted card → 404");

    cleanup_fixture(&f).await;
}
