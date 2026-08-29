//! HTTP and PostgreSQL parity coverage for the create -> checkout handoff.
//!
//! The test starts with the same public route a Board uses to create an Issue,
//! then checks it out through the agent/run-scoped route and verifies the
//! persisted ownership transition.

use api::routes::issues::issue_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::AuthorizationActor;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    board_user_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let prefix = format!("IC{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Create checkout parity")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, 'running')")
        .bind(agent_id)
        .bind(company_id)
        .bind("Create checkout agent")
        .execute(pool)
        .await
        .expect("insert agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(json!({"source": "test"}))
    .execute(pool)
    .await
    .expect("insert heartbeat run");

    Fixture {
        pool: pool.clone(),
        company_id,
        board_user_id,
        agent_id,
        run_id,
    }
}

fn board_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.board_user_id, fixture.company_id)
}

fn agent_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::agent(fixture.agent_id, fixture.company_id, Some(fixture.run_id))
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serialize body")))
        .expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
    (status, value)
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM heartbeat_runs WHERE id = $1")
        .bind(fixture.run_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(fixture.agent_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

#[sqlx::test]
async fn create_then_agent_checkout_persists_run_ownership(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (status, created) = send(
        &app,
        &board_actor(&fixture),
        "POST",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({"title": "Create then checkout", "status": "todo"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let issue_id = Uuid::parse_str(created["id"].as_str().expect("created issue id"))
        .expect("created issue UUID");
    assert_eq!(created["status"], "todo");

    let (status, checked_out) = send(
        &app,
        &agent_actor(&fixture),
        "POST",
        &format!("/issues/{issue_id}/checkout"),
        json!({
            "agentId": fixture.agent_id,
            "expectedStatuses": ["todo"],
            "checkoutRunId": fixture.run_id
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(checked_out["id"], created["id"]);
    assert_eq!(checked_out["status"], "in_progress");
    assert_eq!(checked_out["assigneeAgentId"], fixture.agent_id.to_string());
    assert_eq!(checked_out["checkoutRunId"], fixture.run_id.to_string());

    let heartbeat_run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM heartbeat_runs WHERE company_id = $1 AND agent_id = $2",
    )
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .fetch_one(&pool)
    .await
    .expect("heartbeat run count");
    assert_eq!(heartbeat_run_count, 1, "same-run checkout must not create a second wakeup run");

    let persisted: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT status::text, assignee_agent_id, checkout_run_id
         FROM issues WHERE id = $1 AND company_id = $2",
    )
    .bind(issue_id)
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("persisted checked-out issue");
    assert_eq!(persisted.0, "in_progress");
    assert_eq!(persisted.1, Some(fixture.agent_id));
    assert_eq!(persisted.2, Some(fixture.run_id));

    cleanup(&fixture).await;
}
