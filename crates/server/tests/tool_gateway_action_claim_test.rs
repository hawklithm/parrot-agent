//! Tool Gateway action approval concurrency coverage.
//!
//! The approval route must atomically claim a pending action before dispatching
//! a plugin or MCP tool. Two simultaneous approvals may therefore produce one
//! execution result and one conflict, but never two dispatch attempts.

use api::routes::{tool_access::tool_access_routes, tools::tool_routes};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
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

async fn create_session(
    app: &Router,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/tool-gateway/sessions")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "companyId": company_id,
                "agentId": agent_id,
                "runId": run_id
            }))
            .expect("serialize session body"),
        ))
        .expect("build session request");
    let response = app.clone().oneshot(request).await.expect("create session");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read session body");
    serde_json::from_slice::<Value>(&body)
        .expect("parse session body")
        .get("token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .expect("session token")
}

async fn send_idempotent_call(
    app: &Router,
    token: &str,
    idempotency_key: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri("/tool-gateway/tools/call")
        .header("content-type", "application/json")
        .header("x-paperclip-tool-gateway-token", token)
        .body(Body::from(
            serde_json::to_vec(&json!({
                "tool": "mcp.missing:call",
                "parameters": {"value": "same-call"},
                "idempotencyKey": idempotency_key
            }))
            .expect("serialize call body"),
        ))
        .expect("build call request");
    let response = app.clone().oneshot(request).await.expect("dispatch call");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read call body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

fn board_actor_with_role(company_id: Uuid, role: MembershipRole) -> AuthorizationActor {
    let user_id = Uuid::new_v4();
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            role,
        )],
        false,
    )
}

async fn list_named_gateways(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!("/companies/{company_id}/tools/gateways"))
        .body(Body::empty())
        .expect("build gateway list request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("list gateways");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read gateway list body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn create_named_gateway(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/companies/{company_id}/tools/gateways"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({})).expect("serialize gateway body"),
        ))
        .expect("build gateway create request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("create gateway");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read gateway create body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn list_gateway_runtime_slots(
    app: &Router,
    actor: &AuthorizationActor,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri("/tool-gateway/runtime-slots")
        .body(Body::empty())
        .expect("build runtime slots request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("list runtime slots");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read runtime slots body");
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

#[tokio::test]
async fn concurrent_tool_calls_replay_one_idempotency_key() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Gateway Idempotency Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_id)
        .bind(company_id)
        .bind("Tool Gateway Idempotency Agent")
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
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, transport_config, enabled)
         VALUES ($1, $2, 'Missing MCP', 'missing', 'mcp_remote', '{}', true)",
    )
    .bind(connection_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert tool connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_routes().with_state(state);
    let token = create_session(&app, company_id, agent_id, run_id).await;
    let idempotency_key = "concurrent-mcp-call-1";
    let (first, second) = tokio::join!(
        send_idempotent_call(&app, &token, idempotency_key),
        send_idempotent_call(&app, &token, idempotency_key),
    );

    assert_eq!(first.0, StatusCode::FORBIDDEN, "first={:?}", first.1);
    assert_eq!(second.0, StatusCode::FORBIDDEN, "second={:?}", second.1);
    assert!(
        first.1.get("invocationId").is_some() && second.1.get("invocationId").is_some(),
        "both responses must identify the durable invocation: first={:?}, second={:?}",
        first.1,
        second.1
    );
    assert_eq!(
        first.1.get("invocationId"),
        second.1.get("invocationId"),
        "same idempotency key must replay the same invocation"
    );
    assert!(
        first.1.get("replayed").is_some() || second.1.get("replayed").is_some(),
        "one concurrent response must identify the replay path: first={:?}, second={:?}",
        first.1,
        second.1
    );

    let (invocation_count, stored_status, stored_key): (i64, String, Option<String>) =
        sqlx::query_as(
            "SELECT COUNT(*), MIN(status), MIN(idempotency_key)
               FROM tool_invocations
              WHERE company_id = $1 AND tool_name = 'mcp.missing:call'",
        )
        .bind(company_id)
        .fetch_one(&pool)
        .await
        .expect("read idempotent invocations");
    assert_eq!(invocation_count, 1);
    assert_eq!(stored_status, "denied");
    assert_eq!(stored_key.as_deref(), Some(idempotency_key));

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn named_gateway_routes_require_tools_admin_permission() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let state = build_app_state(pool).await.expect("build app state");
    let app = tool_routes().with_state(state);

    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let operator = board_actor_with_role(company_id, MembershipRole::Operator);
    let viewer = board_actor_with_role(company_id, MembershipRole::Viewer);

    let (owner_status, owner_body) = list_named_gateways(&app, &owner, company_id).await;
    assert_eq!(owner_status, StatusCode::OK, "owner={owner_body:?}");
    assert!(owner_body.get("gateways").is_some());

    for (label, actor) in [("operator", operator), ("viewer", viewer)] {
        let (status, body) = list_named_gateways(&app, &actor, company_id).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label}={body:?}");
        assert_eq!(
            body.get("reasonCode").and_then(Value::as_str),
            Some("permission_denied"),
            "{label}={body:?}"
        );
        let (create_status, create_body) = create_named_gateway(&app, &actor, company_id).await;
        assert_eq!(create_status, StatusCode::FORBIDDEN, "{label}={create_body:?}");
        assert_eq!(
            create_body.get("reasonCode").and_then(Value::as_str),
            Some("permission_denied"),
            "{label}={create_body:?}"
        );
    }
}

#[tokio::test]
async fn gateway_runtime_slot_routes_require_manage_runtime_permission() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let state = build_app_state(pool).await.expect("build app state");
    let app = tool_access_routes().with_state(state);

    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let operator = board_actor_with_role(company_id, MembershipRole::Operator);
    let viewer = board_actor_with_role(company_id, MembershipRole::Viewer);

    let (owner_status, owner_body) = list_gateway_runtime_slots(&app, &owner).await;
    assert_eq!(owner_status, StatusCode::OK, "owner={owner_body:?}");
    assert!(owner_body.is_array());

    for (label, actor) in [("operator", operator), ("viewer", viewer)] {
        let (status, body) = list_gateway_runtime_slots(&app, &actor).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label}={body:?}");
    }
}
