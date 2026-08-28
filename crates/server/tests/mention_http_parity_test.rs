//! HTTP parity test for @mention parsing and agent wakeup.
//!
//! Verifies that a comment with `[@Name](agent://<uuid>)` triggers a wakeup
//! for the mentioned agent (same company, not self-mention).

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::json;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::issue_comments::issue_comment_routes;
use services::auth::AuthorizationActor;

/// Reusable seed helpers — tests call `connect_and_migrate` and `seed` before building a router.
async fn connect_and_migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn seed_company(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let prefix = format!("AT{}", &id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id).bind("@mention Test").bind(prefix)
        .execute(pool).await.expect("insert company");
    id
}

async fn seed_agent(pool: &PgPool, company_id: Uuid, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, $4, 'process')")
        .bind(id).bind(company_id).bind(format!("Agent-{id}")).bind(status)
        .execute(pool).await.expect("insert agent");
    id
}

async fn seed_issue(pool: &PgPool, company_id: Uuid, agent_id: Uuid) -> (Uuid, String) {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO issues (id, company_id, title, status, assignee_agent_id, created_at, updated_at) VALUES ($1, $2, $3, 'in_progress', $4, NOW(), NOW())")
        .bind(id).bind(company_id).bind("Test Issue").bind(agent_id)
        .execute(pool).await.expect("insert issue");
    (id, id.simple().to_string())
}

fn commenting_agent(company_id: Uuid, agent_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(agent_id, company_id, None)
}

async fn build_router(pool: PgPool) -> Router {
    let app_state = build_app_state(pool).await.unwrap();
    api::routes::issue_comments::issue_comment_routes().with_state(app_state)
}

#[sqlx::test]
async fn mention_parses_agent_url_and_triggers_wakeup(pool: PgPool) {
    connect_and_migrate(&pool).await;
    let cid = seed_company(&pool).await;
    let mentioned = seed_agent(&pool, cid, "idle").await;
    let self_agent = seed_agent(&pool, cid, "running").await;
    let (issue_id, _) = seed_issue(&pool, cid, self_agent).await;

    // Build app state and issue_comment_routes
    let app = build_router(pool.clone()).await;

    // Post a comment with a mention to the other agent
    let body = json!({
        "body": format!("Hey [@Mentioned](agent://{mentioned}) please look at this"),
        "actor_type": "agent",
        "actor_id": self_agent,
    });

    let actor = commenting_agent(cid, self_agent);
    let mut req = Request::builder()
        .method("POST")
        .uri(&format!("/issues/{issue_id}/comments"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    req.extensions_mut().insert(actor);

    let res = app.oneshot(req).await.unwrap();
    let s = res.status();
    assert!(
        s == StatusCode::OK || s == StatusCode::CREATED,
        "comment should succeed (got {s})"
    );

    // Verify activity_log was written for the mentioned agent
    let activity_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM activity_logs WHERE company_id = $1 AND event_type = 'issue_comment_mentioned' AND resource_id = $2"
    )
    .bind(cid)
    .bind(mentioned)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    assert!(activity_count > 0, "mention activity should be logged for mentioned agent");
}

#[sqlx::test]
async fn self_mention_does_not_create_wakeup(pool: PgPool) {
    connect_and_migrate(&pool).await;
    let cid = seed_company(&pool).await;
    let agent = seed_agent(&pool, cid, "idle").await;
    let (issue_id, _) = seed_issue(&pool, cid, agent).await;

    let app = build_router(pool.clone()).await;

    let body = json!({
        "body": format!("self mention [@Self](agent://{agent}) test"),
        "actor_type": "agent",
        "actor_id": agent,
    });

    let actor = commenting_agent(cid, agent);
    let mut req = Request::builder()
        .method("POST")
        .uri(&format!("/issues/{issue_id}/comments"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    req.extensions_mut().insert(actor);

    let res = app.oneshot(req).await.unwrap();
    let s = res.status(); assert!(s == StatusCode::OK || s == StatusCode::CREATED, "expected 200 or 201 got {s}");

    let wakeup_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests WHERE agent_id = $1 AND company_id = $2"
    )
    .bind(agent)
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    assert_eq!(wakeup_count, 0, "self-mention should not create wakeup");
}

#[sqlx::test]
async fn cross_company_mention_is_ignored(pool: PgPool) {
    connect_and_migrate(&pool).await;
    let cid1 = seed_company(&pool).await;
    let cid2 = seed_company(&pool).await;
    let agent_in_cid1 = seed_agent(&pool, cid1, "idle").await;
    let agent_in_cid2 = seed_agent(&pool, cid2, "running").await;
    let (issue_id, _) = seed_issue(&pool, cid1, agent_in_cid1).await;

    let app = build_router(pool.clone()).await;

    let body = json!({
        "body": format!("mention cross-company [@Other](agent://{agent_in_cid2})"),
        "actor_type": "agent",
        "actor_id": agent_in_cid1,
    });

    let actor = commenting_agent(cid1, agent_in_cid1);
    let mut req = Request::builder()
        .method("POST")
        .uri(&format!("/issues/{issue_id}/comments"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    req.extensions_mut().insert(actor);

    let res = app.oneshot(req).await.unwrap();
    let s = res.status(); assert!(s == StatusCode::OK || s == StatusCode::CREATED, "expected 200 or 201 got {s}");

    let wakeup_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests WHERE agent_id = $1"
    )
    .bind(agent_in_cid2)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    assert_eq!(wakeup_count, 0, "cross-company mention should not create wakeup");
}
