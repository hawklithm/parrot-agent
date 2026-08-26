//! HTTP parity integration tests for Paperclip's Company Search Extract (#4C.3 `/search/extract`).
//!
//! `GET /companies/:company_id/search/extract` returns the `CompanySearchExtractResponse`
//! shape (issue title/description + comment + document hit excerpts), with
//! `contains`/`kind`/`scope`/`limit`/`offset`/`matchesPerIssue`, tenant isolation,
//! `contains` length validation, and cross-company 403.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test company_search_extract_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::{automation_misc_routes, company_routes};
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
    issue_a_title: Uuid,
    issue_a_desc: Uuid,
    issue_a_comment: Uuid,
    issue_a_doc: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let prefix_a = format!("EX{}", &company_a.simple().to_string()[..8]);
    let prefix_b = format!("EX{}", &company_b.simple().to_string()[..8]);

    for (id, name) in [(company_a, "Extract Parity Co A"), (company_b, "Extract Parity Co B")] {
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

    let issue_a_title = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(issue_a_title)
    .bind(company_a)
    .bind("Login button broken on homepage")
    .bind(format!("{}-1", prefix_a))
    .bind("unrelated body text")
    .execute(pool)
    .await
    .expect("insert title issue");

    let issue_a_desc = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'backlog', 'low', $5)",
    )
    .bind(issue_a_desc)
    .bind(company_a)
    .bind("Another task")
    .bind(format!("{}-2", prefix_a))
    .bind("the migration broke the login endpoint last night")
    .execute(pool)
    .await
    .expect("insert description issue");

    // Issue with a comment containing the keyword (scope=comments target).
    let issue_a_comment = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'in_progress', 'high', $5)",
    )
    .bind(issue_a_comment)
    .bind(company_a)
    .bind("Triage ticket")
    .bind(format!("{}-3", prefix_a))
    .bind("see attached notes")
    .execute(pool)
    .await
    .expect("insert comment issue");
    sqlx::query(
        "INSERT INTO issue_comments (id, company_id, issue_id, body, actor_type) \
         VALUES ($1, $2, $3, $4, 'user')",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(issue_a_comment)
    .bind("customer reported the login loop again")
    .execute(pool)
    .await
    .expect("insert comment");


    // Issue with a document containing the keyword (scope=documents target).
    let issue_a_doc = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, identifier, status, priority, description) \
         VALUES ($1, $2, $3, $4, 'todo', 'medium', $5)",
    )
    .bind(issue_a_doc)
    .bind(company_a)
    .bind("Doc task")
    .bind(format!("{}-4", prefix_a))
    .bind("body")
    .execute(pool)
    .await
    .expect("insert doc issue");
    let doc_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO documents (id, company_id, title, content, content_type) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(doc_id)
    .bind(company_a)
    .bind("Login runbook")
    .bind("runbook: restart the login service after deploy")
    .bind("text/markdown")
    .execute(pool)
    .await
    .expect("insert document");
    sqlx::query(
        "INSERT INTO issue_documents (id, company_id, issue_id, document_id, key) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(company_a)
    .bind(issue_a_doc)
    .bind(doc_id)
    .bind("runbook")
    .execute(pool)
    .await
    .expect("link document");

    // Company B issue with the same keyword must NOT appear in company A extract.
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
        issue_a_title,
        issue_a_desc,
        issue_a_comment,
        issue_a_doc,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issue_documents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM documents WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issue_comments WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    for cid in [f.company_a, f.company_b] {
        let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
            .bind(cid)
            .execute(&f.pool)
            .await;
        let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
            .bind(cid)
            .execute(&f.pool)
            .await;
    }
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for company search extract HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #4C.3 `/search/extract` acceptance — issue/comment/document hits, scopes,
/// tenant isolation, validation, and cross-company 403.
#[tokio::test]
async fn company_search_extract_matches_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let user = Uuid::new_v4();
    let actor = board_actor(user, f.company_a);
    let app = Router::new()
        .merge(company_routes())
        .merge(automation_misc_routes())
        .with_state(build_app_state(pool.clone()).await.expect("app state"));

    // --- issue scope (default all includes issues) ---
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/search/extract?contains=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "extract status");
    let r = parse(&body);
    assert_eq!(r["contains"], json!("login"));
    assert_eq!(r["kind"], json!("literal"));
    assert_eq!(r["scope"], json!("all"));
    let results = r["results"].as_array().expect("results array");
    // Company B issue must be excluded (tenant isolation).
    let ids: Vec<String> = results
        .iter()
        .map(|x| x["issueId"].as_str().unwrap().to_string())
        .collect();
    assert!(
        !ids.contains(&f.company_b.to_string()),
        "company B issue leaked into company A extract"
    );
    // At least the title issue and description issue must surface with a match.
    let mut saw_title = false;
    let mut saw_desc = false;
    for res in results {
        for m in res["matches"].as_array().unwrap() {
            assert!(m["excerpt"].is_string(), "match has excerpt");
            assert!(m["value"].as_str().unwrap().to_lowercase().contains("login"));
            match m["field"].as_str() {
                Some("title") => saw_title = true,
                Some("description") => saw_desc = true,
                _ => {}
            }
        }
    }
    assert!(saw_title, "expected a title-scope match for 'login'");
    assert!(saw_desc, "expected a description-scope match for 'login'");
    assert_eq!(r["hasMore"], json!(false));
    assert_eq!(r["truncated"], json!(false));

    // --- scope=issues excludes comment/document hits ---
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/search/extract?contains=login&scope=issues", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let r = parse(&body);
    for res in r["results"].as_array().unwrap() {
        for m in res["matches"].as_array().unwrap() {
            assert_ne!(m["field"].as_str(), Some("comment"));
            assert_ne!(m["field"].as_str(), Some("document_body"));
        }
    }

    // --- scope=comments surfaces the comment hit, not issue title/description ---
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/search/extract?contains=login&scope=comments", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let r = parse(&body);
    let results = r["results"].as_array().expect("results array");
    let has_comment = results
        .iter()
        .flat_map(|x| x["matches"].as_array().unwrap())
        .any(|m| m["field"].as_str() == Some("comment"));
    assert!(has_comment, "expected a comment-scope match for 'login'");

    // --- scope=documents surfaces the document hit ---
    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/search/extract?contains=login&scope=documents", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let r = parse(&body);
    let results = r["results"].as_array().expect("results array");
    let has_doc = results
        .iter()
        .flat_map(|x| x["matches"].as_array().unwrap())
        .any(|m| m["field"].as_str().map(|s| s.starts_with("document")).unwrap_or(false));
    assert!(has_doc, "expected a document-scope match for 'login'");

    // --- contains shorter than 2 chars -> 400 ---
    let (status, _) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/search/extract?contains=a", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "short contains must 400");

    // --- cross-company 403 ---
    let intruder = session_board_actor(Uuid::new_v4(), f.company_b);
    let (status, _) = send(
        &app,
        &intruder,
        "GET",
        &format!("/companies/{}/search/extract?contains=login", f.company_a),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company extract must 403");

    cleanup_fixture(&f).await;
}
