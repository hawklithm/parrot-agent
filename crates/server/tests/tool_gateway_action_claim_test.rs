//! Tool Gateway action approval concurrency coverage.
//!
//! The approval route must atomically claim a pending action before dispatching
//! a plugin or MCP tool. Two simultaneous approvals may therefore produce one
//! execution result and one conflict, but never two dispatch attempts.

use api::routes::tools::tool_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::AuthorizationActor;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn send_approval(
    app: &Router,
    actor: &AuthorizationActor,
    action_id: Uuid,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/tool-gateway/action-requests/{action_id}/approve"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({"companyId": company_id})).expect("serialize body"),
        ))
        .expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app.clone().oneshot(request).await.expect("dispatch request");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database for Tool Gateway action claim test");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

#[tokio::test]
async fn concurrent_approval_has_one_database_claimant() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let invocation_id = Uuid::new_v4();
    let action_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Gateway Claim Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_id)
        .bind(company_id)
        .bind("Tool Gateway Claim Agent")
        .execute(&pool)
        .await
        .expect("insert agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'running')",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert heartbeat run");
    sqlx::query(
        "INSERT INTO tool_invocations
            (id, company_id, actor_type, actor_id, agent_id, run_id, tool_name, status)
         VALUES ($1, $2, 'agent', $3, $4, $5, 'mcp.missing', 'pending')",
    )
    .bind(invocation_id)
    .bind(company_id)
    .bind(agent_id.to_string())
    .bind(agent_id)
    .bind(run_id)
    .execute(&pool)
    .await
    .expect("insert tool invocation");
    sqlx::query(
        "INSERT INTO tool_action_requests
            (id, company_id, invocation_id, status, canonical_arguments_hash,
             canonical_arguments_summary, signed_arguments, preview_markdown,
             requested_by_agent_id)
         VALUES ($1, $2, $3, 'pending', 'claim-test', '{}', '{}', 'claim test', $4)",
    )
    .bind(action_id)
    .bind(company_id)
    .bind(invocation_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert action request");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_routes().with_state(state);
    let actor = AuthorizationActor::board(Uuid::new_v4(), company_id);
    let (first, second) = tokio::join!(
        send_approval(&app, &actor, action_id, company_id),
        send_approval(&app, &actor, action_id, company_id),
    );

    let statuses = [first.0, second.0];
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "one concurrent approval must lose the claim: {statuses:?}, first={:?}, second={:?}",
        first.1,
        second.1
    );
    assert!(
        statuses.iter().any(|status| status.is_server_error()),
        "the winning mcp dispatch should settle with an execution error: {statuses:?}, first={:?}, second={:?}",
        first.1,
        second.1
    );

    let (request_status, invocation_status): (String, String) = sqlx::query_as(
        "SELECT ar.status, i.status
           FROM tool_action_requests ar
           JOIN tool_invocations i ON i.id = ar.invocation_id
          WHERE ar.id = $1 AND ar.company_id = $2",
    )
    .bind(action_id)
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("read settled action");
    assert_eq!(request_status, "failed");
    assert_eq!(invocation_status, "failed");

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}
