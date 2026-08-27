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

async fn list_company_runtime_slots(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!("/companies/{company_id}/tools/runtime-slots"))
        .body(Body::empty())
        .expect("build company runtime slots request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("list company runtime slots");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read company runtime slots body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn post_runtime_slot_action(
    app: &Router,
    actor: &AuthorizationActor,
    uri: String,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(uri)
        .body(Body::empty())
        .expect("build runtime slot action request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch runtime slot action");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read runtime slot action body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn list_company_stdio_templates(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!("/companies/{company_id}/tools/stdio-templates"))
        .body(Body::empty())
        .expect("build stdio template list request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("list stdio templates");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read stdio template list body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn create_company_stdio_template(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/companies/{company_id}/tools/stdio-templates"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "templateId": "admin.test-template",
                "name": "Test template",
                "description": "A persistent test template",
                "command": "echo",
                "args": ["hello"],
                "envKeys": ["TEST_VALUE"],
                "tools": [{"name": "echo"}]
            }))
                .expect("serialize stdio template body"),
        ))
        .expect("build stdio template create request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("create stdio template");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read stdio template create body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn disable_company_stdio_template(
    app: &Router,
    actor: &AuthorizationActor,
    company_id: Uuid,
    template_id: &str,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!(
            "/companies/{company_id}/tools/stdio-templates/{template_id}/disable"
        ))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "reason": "test cleanup" }))
                .expect("serialize stdio template disable body"),
        ))
        .expect("build stdio template disable request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("disable stdio template");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read stdio template disable body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn request_connection_route(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: String,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(
                serde_json::to_vec(&body).expect("serialize connection route body"),
            ))
            .expect("build connection route request")
    } else {
        builder
            .body(Body::empty())
            .expect("build connection route request")
    };
    let mut request = request;
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch connection route");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read connection route body");
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

    let (company_owner_status, company_owner_body) =
        list_company_runtime_slots(&app, &owner, company_id).await;
    assert_eq!(
        company_owner_status,
        StatusCode::OK,
        "company owner={company_owner_body:?}"
    );
    assert!(company_owner_body.is_array());

    let slot_id = Uuid::new_v4();
    for (label, uri) in [
        (
            "company stop",
            format!("/companies/{company_id}/tools/runtime-slots/{slot_id}/stop"),
        ),
        (
            "company restart",
            format!("/companies/{company_id}/tools/runtime-slots/{slot_id}/restart"),
        ),
        (
            "gateway stop",
            format!("/tool-gateway/runtime-slots/{slot_id}/stop"),
        ),
        (
            "gateway restart",
            format!("/tool-gateway/runtime-slots/{slot_id}/restart"),
        ),
    ] {
        let (status, body) = post_runtime_slot_action(&app, &owner, uri).await;
        assert_eq!(status, StatusCode::OK, "{label} owner={body:?}");
    }

    for (label, actor) in [("operator", operator), ("viewer", viewer)] {
        let (status, body) = list_gateway_runtime_slots(&app, &actor).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label}={body:?}");

        let (status, body) = list_company_runtime_slots(&app, &actor, company_id).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{label} company={body:?}");

        for (action, uri) in [
            (
                "company stop",
                format!("/companies/{company_id}/tools/runtime-slots/{slot_id}/stop"),
            ),
            (
                "company restart",
                format!("/companies/{company_id}/tools/runtime-slots/{slot_id}/restart"),
            ),
            (
                "gateway stop",
                format!("/tool-gateway/runtime-slots/{slot_id}/stop"),
            ),
            (
                "gateway restart",
                format!("/tool-gateway/runtime-slots/{slot_id}/restart"),
            ),
        ] {
            let (status, body) = post_runtime_slot_action(&app, &actor, uri).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{label} {action}={body:?}");
        }
    }
}

#[tokio::test]
async fn stdio_template_routes_require_tools_admin_permission() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Gateway Stdio Template Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    let state = build_app_state(pool.clone())
        .await
        .expect("build app state");
    let app = tool_access_routes().with_state(state);

    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let operator = board_actor_with_role(company_id, MembershipRole::Operator);
    let viewer = board_actor_with_role(company_id, MembershipRole::Viewer);

    let (owner_list_status, owner_list_body) =
        list_company_stdio_templates(&app, &owner, company_id).await;
    assert_eq!(
        owner_list_status,
        StatusCode::OK,
        "owner list={owner_list_body:?}"
    );
    assert!(owner_list_body.is_array());
    let (owner_create_status, owner_create_body) =
        create_company_stdio_template(&app, &owner, company_id).await;
    assert_eq!(
        owner_create_status,
        StatusCode::CREATED,
        "owner create={owner_create_body:?}"
    );
    assert_eq!(
        owner_create_body.get("templateId").and_then(Value::as_str),
        Some("admin.test-template")
    );
    assert_eq!(
        owner_create_body.get("status").and_then(Value::as_str),
        Some("active")
    );
    let (owner_disable_status, owner_disable_body) = disable_company_stdio_template(
        &app,
        &owner,
        company_id,
        "admin.test-template",
    )
    .await;
    assert_eq!(
        owner_disable_status,
        StatusCode::OK,
        "owner disable={owner_disable_body:?}"
    );
    assert_eq!(
        owner_disable_body.get("status").and_then(Value::as_str),
        Some("disabled")
    );
    let (owner_after_disable_status, owner_after_disable_body) =
        list_company_stdio_templates(&app, &owner, company_id).await;
    assert_eq!(owner_after_disable_status, StatusCode::OK);
    assert_eq!(
        owner_after_disable_body
            .as_array()
            .and_then(|templates| templates.first())
            .and_then(|template| template.get("status"))
            .and_then(Value::as_str),
        Some("disabled")
    );

    for (label, actor) in [("operator", operator), ("viewer", viewer)] {
        let (list_status, list_body) = list_company_stdio_templates(&app, &actor, company_id).await;
        assert_eq!(list_status, StatusCode::FORBIDDEN, "{label} list={list_body:?}");
        let (create_status, create_body) =
            create_company_stdio_template(&app, &actor, company_id).await;
        assert_eq!(
            create_status,
            StatusCode::FORBIDDEN,
            "{label} create={create_body:?}"
        );
        let (disable_status, disable_body) = disable_company_stdio_template(
            &app,
            &actor,
            company_id,
            "admin.test-template",
        )
        .await;
        assert_eq!(
            disable_status,
            StatusCode::FORBIDDEN,
            "{label} disable={disable_body:?}"
        );
    }

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn connection_management_routes_require_tool_permissions() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Gateway Permission Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status) VALUES ($1, $2, $3, 'running')",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("Tool Gateway Permission Agent")
    .execute(&pool)
    .await
    .expect("insert agent");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, transport_config, enabled)
         VALUES ($1, $2, 'Permission MCP', 'permission-mcp', 'mcp_remote', '{}', true)",
    )
    .bind(connection_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert tool connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let operator = board_actor_with_role(company_id, MembershipRole::Operator);
    let viewer = board_actor_with_role(company_id, MembershipRole::Viewer);

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-connections/{connection_id}/test-agents"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test agents={body:?}");
    assert!(body.is_array());

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-connections/{connection_id}/test-calls/test-call-1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "test call status={body:?}");

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-connections/{connection_id}/test-calls"),
        Some(json!({ "tool": "echo" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "test call create={body:?}");

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-connections/{connection_id}/grants/installations"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "grant install={body:?}");

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "PUT",
        format!("/tool-connections/{connection_id}/installs"),
        Some(json!({
            "installs": [{
                "targetType": "agent",
                "targetId": agent_id
            }]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "install sync={body:?}");
    assert_eq!(
        body.get("connectionId").and_then(Value::as_str),
        Some(connection_id.to_string().as_str())
    );
    assert_eq!(body["installs"].as_array().map(Vec::len), Some(1));
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-connections/{connection_id}/installs"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "install list={body:?}");
    assert_eq!(body["installs"].as_array().map(Vec::len), Some(1));
    let grant_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tool_connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND agent_id = $3",
    )
    .bind(company_id)
    .bind(connection_id)
    .bind(agent_id)
    .fetch_one(&pool)
    .await
    .expect("read installed grant");
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-connections/{connection_id}/grants/{grant_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "grant delete={body:?}");

    for (label, actor) in [("operator", operator), ("viewer", viewer)] {
        for (route, method, body) in [
            (
                format!("/tool-connections/{connection_id}/test-agents"),
                "GET",
                None,
            ),
            (
                format!("/tool-connections/{connection_id}/test-calls/test-call-1"),
                "GET",
                None,
            ),
            (
                format!("/tool-connections/{connection_id}/test-calls"),
                "POST",
                Some(json!({ "tool": "echo" })),
            ),
            (
                format!("/tool-connections/{connection_id}/grants/installations"),
                "POST",
                None,
            ),
            (
                format!(
                    "/tool-connections/{connection_id}/grants/{}",
                    Uuid::new_v4()
                ),
                "DELETE",
                None,
            ),
            (
                format!("/tool-connections/{connection_id}/installs"),
                "PUT",
                Some(json!({ "installs": [] })),
            ),
        ] {
            let (status, response_body) =
                request_connection_route(&app, &actor, method, route, body).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{label} {method} response={response_body:?}"
            );
        }
    }

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn connection_subresources_are_company_scoped() {
    let pool = connect_and_migrate().await;
    let owner_company_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();

    for (company_id, name) in [
        (owner_company_id, "Tool Gateway Resource Owner"),
        (other_company_id, "Tool Gateway Resource Other"),
    ] {
        let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(company_id)
            .bind(name)
            .bind(issue_prefix)
            .execute(&pool)
            .await
            .expect("insert resource-scope company");
    }
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, transport_config, enabled)
         VALUES ($1, $2, 'Scoped MCP', 'scoped-mcp', 'mcp_remote', '{}', true)",
    )
    .bind(connection_id)
    .bind(owner_company_id)
    .execute(&pool)
    .await
    .expect("insert scoped tool connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let other_owner = board_actor_with_role(other_company_id, MembershipRole::Owner);

    for (method, uri) in [
        (
            "GET",
            format!("/tool-connections/{connection_id}/grants"),
        ),
        (
            "GET",
            format!("/tool-connections/{connection_id}/usage"),
        ),
        (
            "GET",
            format!("/tool-connections/{connection_id}/installs"),
        ),
        (
            "GET",
            format!("/tool-connections/{connection_id}/catalog"),
        ),
        (
            "GET",
            format!("/tool-connections/{connection_id}/activity"),
        ),
        (
            "POST",
            format!("/tool-connections/{connection_id}/health-check"),
        ),
        (
            "POST",
            format!("/agents/me/connections/{connection_id}/start-authorization"),
        ),
        (
            "POST",
            format!("/agents/me/connections/{connection_id}/token"),
        ),
    ] {
        let (status, body) = request_connection_route(&app, &other_owner, method, uri, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-company {method} response={body:?}"
        );
    }

    let _ = sqlx::query("DELETE FROM companies WHERE id IN ($1, $2)")
        .bind(owner_company_id)
        .bind(other_company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn profile_subresources_are_company_scoped() {
    let pool = connect_and_migrate().await;
    let owner_company_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let entry_id = Uuid::new_v4();

    for (company_id, name) in [
        (owner_company_id, "Tool Profile Resource Owner"),
        (other_company_id, "Tool Profile Resource Other"),
    ] {
        let issue_prefix = format!("TP{}", &company_id.simple().to_string()[..8]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(company_id)
            .bind(name)
            .bind(issue_prefix)
            .execute(&pool)
            .await
            .expect("insert profile-scope company");
    }
    sqlx::query(
        "INSERT INTO tool_profiles (id, company_id, profile_key, name, description)
         VALUES ($1, $2, 'scoped-profile', 'Scoped profile', 'profile scope test')",
    )
    .bind(profile_id)
    .bind(owner_company_id)
    .execute(&pool)
    .await
    .expect("insert scoped tool profile");
    sqlx::query(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect, tool_name)
         VALUES ($1, $2, $3, 'tool_name', 'include', 'scoped.tool')",
    )
    .bind(entry_id)
    .bind(owner_company_id)
    .bind(profile_id)
    .execute(&pool)
    .await
    .expect("insert scoped tool profile entry");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let other_owner = board_actor_with_role(other_company_id, MembershipRole::Owner);

    for (method, uri, body) in [
        (
            "GET",
            format!("/tool-profiles/{profile_id}/new-tools"),
            None,
        ),
        (
            "PATCH",
            format!("/tool-profiles/{profile_id}"),
            Some(json!({"name": "cross-company mutation"})),
        ),
        (
            "DELETE",
            format!("/tool-profiles/{profile_id}"),
            None,
        ),
        (
            "PATCH",
            format!("/tool-profile-entries/{entry_id}"),
            Some(json!({"enabled": false})),
        ),
        (
            "DELETE",
            format!("/tool-profile-entries/{entry_id}"),
            None,
        ),
        (
            "POST",
            format!("/tool-profiles/{profile_id}/duplicate"),
            None,
        ),
        (
            "POST",
            format!("/tool-profiles/{profile_id}/entries"),
            Some(json!({"tool": "cross-company.tool", "enabled": true})),
        ),
        (
            "POST",
            format!("/tool-profiles/{profile_id}/new-tools/review"),
            None,
        ),
    ] {
        let (status, body) = request_connection_route(&app, &other_owner, method, uri, body).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-company profile {method} response={body:?}"
        );
    }

    let (profile_count, entry_count): (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM tool_profiles WHERE id = $1 AND company_id = $2),
            (SELECT COUNT(*) FROM tool_profile_entries WHERE id = $3 AND company_id = $2)",
    )
    .bind(profile_id)
    .bind(owner_company_id)
    .bind(entry_id)
    .fetch_one(&pool)
    .await
    .expect("verify cross-company profile isolation");
    assert_eq!(profile_count, 1);
    assert_eq!(entry_count, 1);

    let owner = board_actor_with_role(owner_company_id, MembershipRole::Owner);
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-profiles/{profile_id}/new-tools"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner new tools={body:?}");
    assert!(body.is_array());

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "PATCH",
        format!("/tool-profile-entries/{entry_id}"),
        Some(json!({"enabled": false})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner entry update={body:?}");
    assert_eq!(body.get("enabled").and_then(Value::as_bool), Some(false));

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/entries"),
        Some(json!({"tool": "owner.tool", "enabled": true})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "owner entry create={body:?}");
    let created_entry_id = body
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok())
        .expect("created profile entry id");

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/duplicate"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner profile duplicate={body:?}");
    assert_eq!(
        body.get("duplicatedFrom").and_then(Value::as_str),
        Some(profile_id.to_string().as_str())
    );

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/new-tools/review"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner profile review={body:?}");

    for id in [created_entry_id, entry_id] {
        let (status, body) = request_connection_route(
            &app,
            &owner,
            "DELETE",
            format!("/tool-profile-entries/{id}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "owner entry delete={body:?}");
    }
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-profiles/{profile_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "owner profile delete={body:?}");

    let _ = sqlx::query("DELETE FROM companies WHERE id IN ($1, $2)")
        .bind(owner_company_id)
        .bind(other_company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn effective_profile_projection_is_scoped_and_uses_aligned_entry_schema() {
    let pool = connect_and_migrate().await;
    let owner_company_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let binding_id = Uuid::new_v4();
    let entry_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();

    for (company_id, name) in [
        (owner_company_id, "Effective Profile Owner"),
        (other_company_id, "Effective Profile Other"),
    ] {
        let issue_prefix = format!("EP{}", &company_id.simple().to_string()[..8]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(company_id)
            .bind(name)
            .bind(issue_prefix)
            .execute(&pool)
            .await
            .expect("insert effective-profile company");
    }
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Effective Profile Agent')")
        .bind(agent_id)
        .bind(owner_company_id)
        .execute(&pool)
        .await
        .expect("insert effective-profile agent");
    sqlx::query(
        "INSERT INTO tool_profiles (id, company_id, profile_key, name, description)
         VALUES ($1, $2, 'effective-profile', 'Effective profile', 'projection test')",
    )
    .bind(profile_id)
    .bind(owner_company_id)
    .execute(&pool)
    .await
    .expect("insert effective-profile profile");
    sqlx::query(
        "INSERT INTO tool_profile_bindings
            (id, company_id, profile_id, target_type, target_id)
         VALUES ($1, $2, $3, 'agent', $4)",
    )
    .bind(binding_id)
    .bind(owner_company_id)
    .bind(profile_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert effective-profile binding");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, transport_config, enabled)
         VALUES ($1, $2, 'Effective MCP', 'effective-mcp', 'mcp_remote', '{}', true)",
    )
    .bind(connection_id)
    .bind(owner_company_id)
    .execute(&pool)
    .await
    .expect("insert effective-profile connection");
    sqlx::query(
        "INSERT INTO tool_profile_entries
            (id, company_id, profile_id, selector_type, effect, connection_id, tool_name)
         VALUES ($1, $2, $3, 'tool_name', 'include', $4, 'effective.tool')",
    )
    .bind(entry_id)
    .bind(owner_company_id)
    .bind(profile_id)
    .bind(connection_id)
    .execute(&pool)
    .await
    .expect("insert effective-profile entry");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_routes().with_state(state);
    let owner = board_actor_with_role(owner_company_id, MembershipRole::Owner);
    let other_owner = board_actor_with_role(other_company_id, MembershipRole::Owner);
    let agent_actor = AuthorizationActor::agent(agent_id, owner_company_id, None);

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/companies/{owner_company_id}/tools/connections"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner connections={body:?}");
    assert_eq!(body["connections"].as_array().map(Vec::len), Some(1));

    let (status, body) = request_connection_route(
        &app,
        &other_owner,
        "GET",
        format!("/companies/{owner_company_id}/tools/connections"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-company connections={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!(
            "/companies/{owner_company_id}/tools/profiles/effective/agents/{agent_id}"
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "owner effective profiles={body:?}");
    assert_eq!(
        body["profiles"].as_array().map(Vec::len),
        Some(1),
        "effective profile body={body:?}"
    );
    assert_eq!(
        body["entries"].as_array().map(Vec::len),
        Some(1),
        "effective profile entries body={body:?}"
    );
    assert_eq!(
        body["entries"][0].get("effect").and_then(Value::as_str),
        Some("include")
    );
    assert!(body["entries"][0].get("selectorValue").is_none());
    assert_eq!(
        body["allowedToolNames"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        body["allowedToolNames"][0].as_str(),
        Some("effective.tool")
    );
    assert_eq!(
        body["installedConnections"].as_array().map(Vec::len),
        Some(1)
    );

    let (status, body) = request_connection_route(
        &app,
        &other_owner,
        "GET",
        format!(
            "/companies/{owner_company_id}/tools/profiles/effective/agents/{agent_id}"
        ),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-company effective profiles={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &agent_actor,
        "GET",
        format!("/companies/{owner_company_id}/tools/connections"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "agent connections lookup={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &agent_actor,
        "GET",
        format!(
            "/companies/{owner_company_id}/tools/profiles/effective/agents/{agent_id}"
        ),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "agent effective profiles lookup={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &agent_actor,
        "GET",
        format!("/companies/{owner_company_id}/tools/policies"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "agent policies lookup={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &other_owner,
        "GET",
        format!(
            "/companies/{other_company_id}/tools/profiles/effective/agents/{agent_id}"
        ),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "wrong-company agent effective profiles={body:?}"
    );

    let _ = sqlx::query("DELETE FROM companies WHERE id IN ($1, $2)")
        .bind(owner_company_id)
        .bind(other_company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn run_decision_lookup_enforces_board_company_and_run_scope() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    for (id, name) in [
        (company_id, "Run Decision Owner"),
        (other_company_id, "Run Decision Other"),
    ] {
        let issue_prefix = format!("RD{}", &id.simple().to_string()[..8]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(issue_prefix)
            .execute(&pool)
            .await
            .expect("insert run-decision company");
    }
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Run Decision Agent')")
        .bind(agent_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("insert run-decision agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status)
         VALUES ($1, $2, $3, 'succeeded')",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert run-decision run");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let other_owner = board_actor_with_role(other_company_id, MembershipRole::Owner);
    let agent_actor = AuthorizationActor::agent_with_source(
        agent_id,
        company_id,
        Some(run_id),
        ActorSource::Session,
    );

    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/companies/{company_id}/tools/runs/{run_id}/decisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "same-company run lookup={body:?}");
    assert_eq!(body["runId"].as_str(), Some(run_id.to_string().as_str()));
    assert_eq!(body["decisions"].as_array().map(Vec::len), Some(0));

    let (status, body) = request_connection_route(
        &app,
        &other_owner,
        "GET",
        format!("/companies/{company_id}/tools/runs/{run_id}/decisions"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "cross-company run lookup={body:?}"
    );

    let (status, body) = request_connection_route(
        &app,
        &agent_actor,
        "GET",
        format!("/companies/{company_id}/tools/runs/{run_id}/decisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "agent run lookup={body:?}");

    let missing_run_id = Uuid::new_v4();
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/companies/{company_id}/tools/runs/{missing_run_id}/decisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "missing run lookup={body:?}");

    let _ = sqlx::query("DELETE FROM companies WHERE id IN ($1, $2)")
        .bind(company_id)
        .bind(other_company_id)
        .execute(&pool)
        .await;
}
