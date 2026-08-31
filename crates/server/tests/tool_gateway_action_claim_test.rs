//! Tool Gateway action approval concurrency coverage.
//!
//! The approval route must atomically claim a pending action before dispatching
//! a plugin or MCP tool. Two simultaneous approvals may therefore produce one
//! execution result and one conflict, but never two dispatch attempts.

use api::routes::{tool_access::tool_access_routes, tools::tool_routes};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use chrono::{Duration, Utc};
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use services::secret_provider::{decrypt_secret_material, encrypt_secret_material};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
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

async fn spawn_oauth_token_endpoint() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind OAuth token endpoint");
    let address = listener.local_addr().expect("OAuth token endpoint address");
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept OAuth token request");
        let mut buffer = vec![0_u8; 8192];
        let read = socket
            .read(&mut buffer)
            .await
            .expect("read OAuth token request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        assert!(request.contains("POST /oauth/token"), "unexpected request: {request}");
        assert!(
            request.contains("grant_type=authorization_code")
                && request.contains("code_verifier=verifier")
                && request.contains("client_id=client-id"),
            "PKCE token exchange fields missing: {request}"
        );
        let body = r#"{"access_token":"access-token-from-provider","refresh_token":"refresh-token-from-provider","token_type":"Bearer","scope":"repo read","expires_in":3600}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write OAuth token response");
    });
    (format!("http://{address}/oauth/token"), handle)
}

async fn spawn_connection_token_exchange_endpoint(
    expected_parent_token: &str,
    status_line: &str,
    body: &str,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind connection token exchange endpoint");
    let address = listener
        .local_addr()
        .expect("connection token exchange endpoint address");
    let expected_parent_token = expected_parent_token.to_string();
    let status_line = status_line.to_string();
    let body = body.to_string();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener
            .accept()
            .await
            .expect("accept connection token exchange request");
        let mut buffer = vec![0_u8; 16_384];
        let read = socket
            .read(&mut buffer)
            .await
            .expect("read connection token exchange request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /token/exchange"), "unexpected exchange request: {request}");
        assert!(
            request_lower.contains(&format!("authorization: bearer {expected_parent_token}")),
            "parent credential was not sent as a bearer token: {request}"
        );
        assert!(
            request.contains("\"scope\":[\"repo\"]")
                && request.contains("\"ttlSeconds\":120"),
            "exchange request did not carry the requested scope/ttl: {request}"
        );
        let response = format!(
            "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write connection token exchange response");
    });
    (format!("http://{address}/token/exchange"), handle)
}

async fn insert_oauth_callback_fixture(
    pool: &PgPool,
    company_id: Uuid,
    connection_id: Uuid,
    state_token: &str,
    token_uri: &str,
    user_id: Uuid,
) {
    let issue_prefix = format!("OA{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("OAuth Callback Test")
        .bind(issue_prefix)
        .execute(pool)
        .await
        .expect("insert OAuth callback company");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, auth_kind, status, enabled, config)
         VALUES ($1, $2, 'OAuth callback connection', 'oauth-callback',
                 'mcp_remote', 'oauth', 'draft', false, $3)",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(json!({
        "oauth": {
            "authorizationUrl": "https://provider.example/authorize",
            "tokenUrl": token_uri,
            "clientId": "client-id",
            "redirectUri": "https://parrot.example/api/tools/oauth/callback"
        }
    }))
    .execute(pool)
    .await
    .expect("insert OAuth callback connection");
    sqlx::query(
        "INSERT INTO tool_oauth_states
            (state, company_id, connection_id, code_verifier,
             created_by_actor_type, created_by_actor_id, subject_user_id,
             requested_scopes, expires_at)
         VALUES ($1, $2, $3, 'verifier', 'agent', $4, $5, $6, $7)",
    )
    .bind(state_token)
    .bind(company_id)
    .bind(connection_id)
    .bind(Uuid::new_v4().to_string())
    .bind(user_id.to_string())
    .bind(json!(["repo"]))
    .bind(Utc::now() + Duration::minutes(10))
    .execute(pool)
    .await
    .expect("insert OAuth callback state");
}

async fn insert_token_exchange_fixture(
    pool: &PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    run_id: Uuid,
    connection_id: Uuid,
    parent_secret_id: Uuid,
    token_uri: &str,
) {
    let issue_prefix = format!("TX{}", &company_id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Token Exchange Test")
        .bind(issue_prefix)
        .execute(pool)
        .await
        .expect("insert token exchange company");
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_id)
        .bind(company_id)
        .bind("Token Exchange Agent")
        .execute(pool)
        .await
        .expect("insert token exchange agent");
    sqlx::query(
        "INSERT INTO heartbeat_runs
            (id, company_id, agent_id, status)
         VALUES ($1, $2, $3, 'running')",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert token exchange run");

    let (material, digest) = encrypt_secret_material("parent-token")
        .expect("encrypt token exchange parent credential");
    sqlx::query(
        "INSERT INTO company_secrets
            (id, company_id, key, name, provider, status, scope, managed_mode)
         VALUES ($1, $2, 'token-exchange-parent', 'Token exchange parent',
                 'local_encrypted', 'active', 'company', 'paperclip_managed')",
    )
    .bind(parent_secret_id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert token exchange parent secret");
    sqlx::query(
        "INSERT INTO company_secret_versions
            (secret_id, version, material, value_sha256, fingerprint_sha256, status)
         VALUES ($1, 1, $2, $3, $3, 'current')",
    )
    .bind(parent_secret_id)
    .bind(material)
    .bind(digest)
    .execute(pool)
    .await
    .expect("insert token exchange parent secret version");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, auth_kind, status, enabled,
             config, credential_secret_refs)
         VALUES ($1, $2, 'Token exchange connection', 'token-exchange',
                 'rest_api', 'api_key', 'active', true, $3, $4)",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(json!({
        "tokenBroker": {
            "enabled": true,
            "path": "exchange",
            "protocol": "generic",
            "tokenUrl": token_uri,
            "parentScopes": ["repo", "read"],
            "defaultScopes": ["repo"]
        }
    }))
    .bind(json!([{
        "secretId": parent_secret_id,
        "versionSelector": "latest",
        "configPath": "credentials.deploy_token",
        "required": true,
        "label": "Deploy token"
    }]))
    .execute(pool)
    .await
    .expect("insert token exchange connection");
}

fn oauth_callback_actor(company_id: Uuid, user_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

async fn send_oauth_callback(
    app: &Router,
    actor: &AuthorizationActor,
    state_token: &str,
    code: &str,
    accept: &str,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!(
            "/tools/oauth/callback?state={state_token}&code={code}"
        ))
        .header("accept", accept)
        .body(Body::empty())
        .expect("build OAuth callback request");
    request.extensions_mut().insert(actor.clone());
    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("dispatch OAuth callback");
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read OAuth callback body");
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, headers, value)
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
async fn concurrent_oauth_callbacks_consume_state_once_and_persist_encrypted_user_grant() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let state_token = format!("oauth-state-{connection_id}");
    let (token_uri, endpoint) = spawn_oauth_token_endpoint().await;
    insert_oauth_callback_fixture(
        &pool,
        company_id,
        connection_id,
        &state_token,
        &token_uri,
        user_id,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let actor = oauth_callback_actor(company_id, user_id);
    let (first, second) = tokio::join!(
        send_oauth_callback(&app, &actor, &state_token, "provider-code", "application/json"),
        send_oauth_callback(&app, &actor, &state_token, "provider-code", "application/json"),
    );
    let statuses = [first.0, second.0];
    assert!(statuses.contains(&StatusCode::OK), "one callback must connect: {statuses:?}");
    assert!(
        statuses.contains(&StatusCode::BAD_REQUEST),
        "the replay must lose the atomic state consume: {statuses:?}"
    );
    let connected = if first.0 == StatusCode::OK { first.2 } else { second.2 };
    assert_eq!(connected["status"], "connected");
    assert!(connected.get("accessToken").is_none());
    assert!(connected["connection"]["credentialSecretRefs"].is_array());

    endpoint.await.expect("OAuth token endpoint task");
    let (connection_status, enabled): (String, bool) = sqlx::query_as(
        "SELECT status, enabled
           FROM tool_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("read connected OAuth connection");
    assert_eq!(connection_status, "active");
    assert!(enabled);

    let materials: Vec<Value> = sqlx::query_scalar(
        "SELECT v.material
           FROM company_secret_versions v
           JOIN company_secrets s ON s.id = v.secret_id
          WHERE s.company_id = $1
            AND s.key LIKE $2
            AND v.status = 'current'",
    )
    .bind(company_id)
    .bind(format!("tool_connection_{connection_id}_oauth_%"))
    .fetch_all(&pool)
    .await
    .expect("read OAuth secret versions");
    assert_eq!(materials.len(), 2);
    let mut plaintexts = materials
        .iter()
        .map(|material| {
            assert!(material.get("ciphertext").is_some());
            decrypt_secret_material(material).expect("decrypt OAuth secret")
        })
        .collect::<Vec<_>>();
    plaintexts.sort();
    assert_eq!(
        plaintexts,
        vec![
            "access-token-from-provider".to_string(),
            "refresh-token-from-provider".to_string()
        ]
    );

    let (grant_status, grant_subject, grant_secret_refs): (String, Option<String>, Value) = sqlx::query_as(
        "SELECT status, subject_user_id, credential_secret_refs
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'user'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read user OAuth grant");
    assert_eq!(grant_status, "active");
    assert_eq!(grant_subject.as_deref(), Some(user_id.to_string().as_str()));
    assert_eq!(grant_secret_refs.as_array().map(Vec::len), Some(2));
    let refs_text = serde_json::to_string(&grant_secret_refs).expect("serialize secret refs");
    assert!(!refs_text.contains("access-token-from-provider"));
    assert!(!refs_text.contains("refresh-token-from-provider"));
    let state_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_oauth_states WHERE state = $1",
    )
    .bind(&state_token)
    .fetch_one(&pool)
    .await
    .expect("count consumed OAuth state");
    assert_eq!(state_count, 0);

    let (replay_status, _, _) =
        send_oauth_callback(&app, &actor, &state_token, "provider-code", "application/json")
            .await;
    assert_eq!(replay_status, StatusCode::BAD_REQUEST);

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn oauth_callback_returns_paperclip_html_redirect() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let state_token = format!("oauth-html-state-{connection_id}");
    let (token_uri, endpoint) = spawn_oauth_token_endpoint().await;
    insert_oauth_callback_fixture(
        &pool,
        company_id,
        connection_id,
        &state_token,
        &token_uri,
        user_id,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let actor = oauth_callback_actor(company_id, user_id);
    let (status, headers, body) =
        send_oauth_callback(&app, &actor, &state_token, "provider-code", "text/html")
            .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "body={body:?}");
    let expected_location = format!("/OA{}/apps/{connection_id}/setup?oauth=connected", &company_id.simple().to_string()[..8]);
    assert_eq!(
        headers.get("location").and_then(|value| value.to_str().ok()),
        Some(expected_location.as_str())
    );
    assert_eq!(
        headers.get("cache-control").and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    endpoint.await.expect("OAuth token endpoint task");

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn oauth_callback_rejects_wrong_subject_without_consuming_state() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let subject_user_id = Uuid::new_v4();
    let state_token = format!("oauth-authz-state-{connection_id}");
    insert_oauth_callback_fixture(
        &pool,
        company_id,
        connection_id,
        &state_token,
        "http://127.0.0.1:9/oauth/token",
        subject_user_id,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let wrong_actor = oauth_callback_actor(company_id, Uuid::new_v4());
    let (status, _, _) =
        send_oauth_callback(&app, &wrong_actor, &state_token, "provider-code", "application/json")
            .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let state_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_oauth_states WHERE state = $1",
    )
    .bind(&state_token)
    .fetch_one(&pool)
    .await
    .expect("count unconsumed OAuth state");
    assert_eq!(state_count, 1);

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

    let owner_user_id = Uuid::new_v4();
    let owner = AuthorizationActor::board_with_source(
        owner_user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            owner_user_id,
            MembershipRole::Owner,
        )],
        false,
    );
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
async fn company_tool_control_plane_routes_require_board_actor() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let resource_id = Uuid::new_v4();
    let state = build_app_state(pool).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(Uuid::new_v4()));

    let mut routes = vec![
        ("GET", format!("/companies/{company_id}/tools/gallery"), None),
        ("GET", format!("/companies/{company_id}/tools/examples"), None),
        (
            "GET",
            format!("/companies/{company_id}/tools/apps/attention"),
            None,
        ),
        (
            "GET",
            format!("/companies/{company_id}/tools/action-requests"),
            None,
        ),
        (
            "GET",
            format!("/companies/{company_id}/tools/applications"),
            None,
        ),
        ("GET", format!("/companies/{company_id}/tools/profiles"), None),
        (
            "GET",
            format!("/companies/{company_id}/tools/runtime-health"),
            None,
        ),
        (
            "GET",
            format!("/companies/{company_id}/tools/trust-rules"),
            None,
        ),
        ("GET", format!("/tool-connections/{resource_id}"), None),
        ("GET", format!("/tool-profiles/{resource_id}/new-tools"), None),
        (
            "POST",
            format!("/companies/{company_id}/tools/connections"),
            Some(json!({
                "name": "agent connection",
                "transport": "mcp_remote"
            })),
        ),
        (
            "POST",
            format!("/companies/{company_id}/tools/profiles"),
            Some(json!({ "name": "agent profile" })),
        ),
        (
            "POST",
            format!("/companies/{company_id}/tools/examples/example/install"),
            None,
        ),
        (
            "POST",
            format!("/companies/{company_id}/tools/mcp/import-json"),
            Some(json!({ "mcpJson": { "mcpServers": {} } })),
        ),
        (
            "PATCH",
            format!("/companies/{company_id}/tools/policies/{resource_id}"),
            Some(json!({ "enabled": true })),
        ),
        (
            "POST",
            format!("/companies/{company_id}/tools/policies/reorder"),
            None,
        ),
        (
            "POST",
            format!("/tool-connections/{resource_id}/health-check"),
            None,
        ),
        (
            "PATCH",
            format!("/tool-profiles/{resource_id}"),
            Some(json!({})),
        ),
        (
            "PATCH",
            format!("/tool-profile-entries/{resource_id}"),
            Some(json!({})),
        ),
    ];

    for (method, uri, body) in routes.drain(..) {
        let (status, response_body) =
            request_connection_route(&app, &agent, method, uri, body).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "Agent must not reach company tool control plane: {method} {response_body:?}"
        );
    }
}

#[tokio::test]
async fn tool_application_crud_matches_board_definition_contract() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let application_id;
    let connection_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Application Contract Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert application company");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let agent = AuthorizationActor::agent(Uuid::new_v4(), company_id, Some(Uuid::new_v4()));

    let (create_status, create_body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/companies/{company_id}/tools/applications"),
        Some(json!({
            "applicationKey": "github",
            "name": "GitHub",
            "description": "GitHub tool application",
            "type": "mcp_http",
            "metadata": {"provider": "github"}
        })),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED, "create={create_body:?}");
    assert_eq!(create_body["companyId"].as_str(), Some(company_id.to_string().as_str()));
    assert_eq!(create_body["applicationKey"].as_str(), Some("github"));
    assert_eq!(create_body["name"].as_str(), Some("GitHub"));
    assert_eq!(create_body["type"].as_str(), Some("mcp_http"));
    assert_eq!(create_body["status"].as_str(), Some("active"));
    application_id = Uuid::parse_str(
        create_body["id"]
            .as_str()
            .expect("created application id"),
    )
    .expect("parse created application id");

    let (list_status, list_body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/companies/{company_id}/tools/applications"),
        None,
    )
    .await;
    assert_eq!(list_status, StatusCode::OK, "list={list_body:?}");
    assert_eq!(list_body["applications"].as_array().map(Vec::len), Some(1));
    assert_eq!(list_body["applications"][0]["id"], create_body["id"]);

    let (agent_list_status, agent_list_body) = request_connection_route(
        &app,
        &agent,
        "GET",
        format!("/companies/{company_id}/tools/applications"),
        None,
    )
    .await;
    assert_eq!(agent_list_status, StatusCode::FORBIDDEN, "agent list={agent_list_body:?}");
    let (agent_create_status, agent_create_body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/companies/{company_id}/tools/applications"),
        Some(json!({ "name": "Agent app", "type": "mcp_http" })),
    )
    .await;
    assert_eq!(
        agent_create_status,
        StatusCode::FORBIDDEN,
        "agent create={agent_create_body:?}"
    );

    let (update_status, update_body) = request_connection_route(
        &app,
        &owner,
        "PATCH",
        format!("/tool-applications/{application_id}"),
        Some(json!({
            "name": "GitHub Cloud",
            "status": "disabled",
            "metadata": {"provider": "github", "mode": "cloud"}
        })),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK, "update={update_body:?}");
    assert_eq!(update_body["name"].as_str(), Some("GitHub Cloud"));
    assert_eq!(update_body["status"].as_str(), Some("disabled"));
    assert_eq!(update_body["metadata"]["mode"].as_str(), Some("cloud"));

    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, transport, transport_config, enabled)
         VALUES ($1, $2, $3, 'GitHub connection', 'github-connection', 'mcp_remote', '{}', true)",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(application_id)
    .execute(&pool)
    .await
    .expect("insert linked application connection");

    let (protected_delete_status, protected_delete_body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-applications/{application_id}"),
        None,
    )
    .await;
    assert_eq!(
        protected_delete_status,
        StatusCode::CONFLICT,
        "protected delete={protected_delete_body:?}"
    );

    sqlx::query("DELETE FROM tool_connections WHERE id = $1 AND company_id = $2")
        .bind(connection_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("delete linked application connection");
    let (delete_status, delete_body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-applications/{application_id}"),
        None,
    )
    .await;
    assert_eq!(delete_status, StatusCode::OK, "delete={delete_body:?}");
    assert_eq!(delete_body["id"], create_body["id"]);

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn tool_connection_lifecycle_matches_definition_contract() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Connection Contract Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert connection company");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);
    let agent = AuthorizationActor::agent(Uuid::new_v4(), company_id, Some(Uuid::new_v4()));

    let (create_status, create_body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/companies/{company_id}/tools/connections"),
        Some(json!({
            "applicationName": "GitHub",
            "name": "GitHub connection",
            "transport": "mcp_remote",
            "authKind": "oauth",
            "config": {"provider": "github"},
            "transportConfig": {"url": "https://example.test/mcp"},
            "credentialRefs": [],
            "credentialSecretRefs": []
        })),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED, "create={create_body:?}");
    assert_eq!(create_body["companyId"].as_str(), Some(company_id.to_string().as_str()));
    assert_eq!(create_body["transport"].as_str(), Some("mcp_remote"));
    assert_eq!(create_body["toolType"].as_str(), Some("mcp_remote"));
    assert_eq!(create_body["authKind"].as_str(), Some("oauth"));
    assert_eq!(create_body["status"].as_str(), Some("draft"));
    assert_eq!(create_body["enabled"], false);
    assert_eq!(create_body["transportConfig"]["url"], "https://example.test/mcp");
    let connection_id = Uuid::parse_str(
        create_body["id"]
            .as_str()
            .expect("created connection id"),
    )
    .expect("parse created connection id");
    assert!(create_body["applicationId"].as_str().is_some());

    let (get_status, get_body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-connections/{connection_id}"),
        None,
    )
    .await;
    assert_eq!(get_status, StatusCode::OK, "get={get_body:?}");
    assert_eq!(get_body["id"], create_body["id"]);
    assert_eq!(get_body["name"], "GitHub connection");

    let (agent_get_status, agent_get_body) = request_connection_route(
        &app,
        &agent,
        "GET",
        format!("/tool-connections/{connection_id}"),
        None,
    )
    .await;
    assert_eq!(agent_get_status, StatusCode::FORBIDDEN, "agent get={agent_get_body:?}");

    let (update_status, update_body) = request_connection_route(
        &app,
        &owner,
        "PATCH",
        format!("/tool-connections/{connection_id}"),
        Some(json!({
            "name": "GitHub cloud",
            "status": "active",
            "enabled": true,
            "transportConfig": {"url": "https://example.test/mcp/v2"}
        })),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK, "update={update_body:?}");
    assert_eq!(update_body["name"], "GitHub cloud");
    assert_eq!(update_body["status"], "active");
    assert_eq!(update_body["enabled"], true);
    assert_eq!(
        update_body["transportConfig"]["url"],
        "https://example.test/mcp/v2"
    );

    let (agent_update_status, agent_update_body) = request_connection_route(
        &app,
        &agent,
        "PATCH",
        format!("/tool-connections/{connection_id}"),
        Some(json!({"name": "agent overwrite"})),
    )
    .await;
    assert_eq!(
        agent_update_status,
        StatusCode::FORBIDDEN,
        "agent update={agent_update_body:?}"
    );

    let (archive_status, archive_body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-connections/{connection_id}"),
        None,
    )
    .await;
    assert_eq!(archive_status, StatusCode::OK, "archive={archive_body:?}");
    assert_eq!(archive_body["status"], "archived");
    assert_eq!(archive_body["enabled"], false);

    let (retained_status, retained_body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-connections/{connection_id}"),
        None,
    )
    .await;
    assert_eq!(retained_status, StatusCode::OK, "retained={retained_body:?}");
    assert_eq!(retained_body["status"], "archived");

    let (repeat_archive_status, repeat_archive_body) = request_connection_route(
        &app,
        &owner,
        "DELETE",
        format!("/tool-connections/{connection_id}"),
        None,
    )
    .await;
    assert_eq!(
        repeat_archive_status,
        StatusCode::OK,
        "repeat archive={repeat_archive_body:?}"
    );
    assert_eq!(repeat_archive_body["status"], "archived");

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn agent_connection_broker_routes_require_active_run_context() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let wrong_agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let subject_user_id = "broker-subject-user";
    let issue_prefix = format!("TG{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Tool Gateway Agent Broker Test")
        .bind(issue_prefix)
        .execute(&pool)
        .await
        .expect("insert broker company");
    for (agent_id, name) in [
        (agent_id, "Tool Gateway Broker Agent"),
        (wrong_agent_id, "Tool Gateway Wrong Agent"),
    ] {
        sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
            .bind(agent_id)
            .bind(company_id)
            .bind(name)
            .execute(&pool)
            .await
            .expect("insert broker agent");
    }
    sqlx::query(
        "INSERT INTO heartbeat_runs
            (id, company_id, agent_id, status, responsible_user_id)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(subject_user_id)
    .execute(&pool)
    .await
    .expect("insert running broker heartbeat");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, name, uid, transport, auth_kind, status,
             config, transport_config, enabled)
         VALUES ($1, $2, 'Agent Broker MCP', 'agent-broker-mcp', 'mcp_remote',
                 'oauth', 'active', $3, '{}', true)",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(json!({
        "oauth": {
            "authorizationUrl": "https://provider.example/oauth/authorize",
            "clientId": "broker-client",
            "redirectUri": "https://parrot.example/api/tools/oauth/callback",
            "scopes": ["openid"]
        },
        "tokenBroker": {
            "enabled": true,
            "path": "static",
            "parentScopes": ["openid", "repo"]
        }
    }))
    .execute(&pool)
    .await
    .expect("insert broker connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let routes = [
        format!("/agents/me/connections/{connection_id}/start-authorization"),
        format!("/agents/me/connections/{connection_id}/token"),
    ];

    let active_agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));
    let (start_status, start_body) = request_connection_route(
        &app,
        &active_agent,
        "POST",
        routes[0].clone(),
        Some(json!({
            "subjectUserId": subject_user_id,
            "scopes": ["openid", "repo"],
            "returnTo": "/TG123/apps"
        })),
    )
    .await;
    assert_eq!(start_status, StatusCode::OK, "start response={start_body:?}");
    let authorization_url = start_body["url"]
        .as_str()
        .expect("authorization URL in Agent response");
    assert!(authorization_url.starts_with("https://provider.example/oauth/authorize?"));
    assert!(authorization_url.contains("response_type=code"));
    assert!(authorization_url.contains("client_id=broker-client"));
    assert!(authorization_url.contains("code_challenge_method=S256"));
    let (stored_subject, stored_actor, stored_scopes, stored_return_to): (
        String,
        String,
        Value,
        String,
    ) = sqlx::query_as(
        "SELECT subject_user_id, created_by_actor_id, requested_scopes, return_to
           FROM tool_oauth_states
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("load persisted Agent OAuth state");
    assert_eq!(stored_subject, subject_user_id);
    assert_eq!(stored_actor, agent_id.to_string());
    assert_eq!(stored_scopes, json!(["openid", "repo"]));
    assert_eq!(stored_return_to, "/TG123/apps");

    let (token_status, token_body) = request_connection_route(
        &app,
        &active_agent,
        "POST",
        routes[1].clone(),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        token_status,
        StatusCode::CONFLICT,
        "token response={token_body:?}"
    );
    assert_eq!(token_body["status"], "use_env_lease");
    assert_eq!(token_body["code"], "use_env_lease");
    assert_eq!(token_body["path"], "static");
    let (issuance_outcome, issuance_error): (String, String) = sqlx::query_as(
        "SELECT outcome, error_code
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("load persisted connection token issuance");
    assert_eq!(issuance_outcome, "use_env_lease");
    assert_eq!(issuance_error, "use_env_lease");

    let (wrong_subject_status, wrong_subject_body) = request_connection_route(
        &app,
        &active_agent,
        "POST",
        routes[0].clone(),
        Some(json!({ "subjectUserId": "another-user" })),
    )
    .await;
    assert_eq!(
        wrong_subject_status,
        StatusCode::FORBIDDEN,
        "wrong subject response={wrong_subject_body:?}"
    );

    let missing_run = AuthorizationActor::agent(agent_id, company_id, None);
    for uri in &routes {
        let body = if uri.contains("start-authorization") {
            Some(json!({ "subjectUserId": subject_user_id }))
        } else {
            Some(json!({}))
        };
        let (status, response_body) =
            request_connection_route(&app, &missing_run, "POST", uri.clone(), body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "missing run response={response_body:?}");
    }

    let wrong_agent = AuthorizationActor::agent(wrong_agent_id, company_id, Some(run_id));
    for uri in &routes {
        let body = if uri.contains("start-authorization") {
            Some(json!({ "subjectUserId": subject_user_id }))
        } else {
            Some(json!({}))
        };
        let (status, response_body) =
            request_connection_route(&app, &wrong_agent, "POST", uri.clone(), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "wrong Agent response={response_body:?}");
    }

    let board = board_actor_with_role(company_id, MembershipRole::Owner);
    for uri in &routes {
        let body = if uri.contains("start-authorization") {
            Some(json!({ "subjectUserId": subject_user_id }))
        } else {
            Some(json!({}))
        };
        let (status, response_body) =
            request_connection_route(&app, &board, "POST", uri.clone(), body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "Board response={response_body:?}");
    }

    sqlx::query("UPDATE heartbeat_runs SET status = 'succeeded' WHERE id = $1")
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("finish broker heartbeat");
    for uri in &routes {
        let body = if uri.contains("start-authorization") {
            Some(json!({ "subjectUserId": subject_user_id }))
        } else {
            Some(json!({}))
        };
        let (status, response_body) =
            request_connection_route(&app, &active_agent, "POST", uri.clone(), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "inactive run response={response_body:?}");
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
    ] {
        let (status, body) = request_connection_route(&app, &other_owner, method, uri, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-company {method} response={body:?}"
        );
    }

    for uri in [
        format!("/agents/me/connections/{connection_id}/start-authorization"),
        format!("/agents/me/connections/{connection_id}/token"),
    ] {
        let (status, body) = request_connection_route(&app, &other_owner, "POST", uri, None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "non-Agent cross-company response={body:?}"
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
            Some(json!({"decisions": [{"catalogEntryId": Uuid::new_v4(), "decision": "allow"}]})),
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

    // Paperclip reviewProfileNewTools: reviewing with no pending tools is a
    // bad request (the review must cover every pending tool exactly once).
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/new-tools/review"),
        Some(json!({"decisions": [{"catalogEntryId": Uuid::new_v4(), "decision": "allow"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "owner profile review={body:?}");

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

// ================= Connection Token Exchange Tests =================

/// POST /agents/me/connections/:connection_id/token exchange path: success (generic protocol).
#[tokio::test]
async fn connection_token_exchange_success_generic() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"minted-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool,
        company_id,
        agent_id,
        run_id,
        connection_id,
        parent_secret_id,
        &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({
            "requestedTtlSeconds": 120,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "exchange response={body:?}");
    assert_eq!(body["status"], "minted");
    assert_eq!(body["path"], "exchange");
    assert_eq!(body["token"], "minted-token");
    assert_eq!(body["tokenType"], "Bearer");
    assert_eq!(body["scope"], json!(["repo"]));
    let ttl = body["ttlSeconds"].as_i64().expect("ttlSeconds");
    assert!(ttl <= 900, "ttlSeconds={ttl} exceeds 900 max");
    assert!(ttl >= 1, "ttlSeconds={ttl} below minimum");
    let expires_at = body["expiresAt"].as_str().expect("expiresAt");
    let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
        .expect("parse expiresAt")
        .with_timezone(&Utc);
    let remaining = (expires_at - Utc::now()).num_seconds();
    assert!(remaining >= 1 && remaining <= 900, "expiresAt remaining={remaining}s outside 1..=900");
    assert_eq!(body["connectionId"].as_str(), Some(connection_id.to_string().as_str()));
    assert_eq!(body["grantId"].as_str().map(|s| !s.is_empty()), Some(true));
    assert_eq!(body["attribution"]["agentId"].as_str(), Some(agent_id.to_string().as_str()));
    assert_eq!(body["attribution"]["runId"].as_str(), Some(run_id.to_string().as_str()));

    // Verify issuance record
    let (issuance_count, issuance_outcome, issuance_token_hash): (i64, String, Option<String>) =
        sqlx::query_as(
            "SELECT COUNT(*), MIN(outcome), MIN(token_hash)
               FROM connection_token_issuances
              WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'",
        )
        .bind(company_id)
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .expect("read issuance");
    assert_eq!(issuance_count, 1, "exactly one issuance record");
    assert_eq!(issuance_outcome, "success");
    let token_hash = issuance_token_hash.expect("token_hash present");
    assert_eq!(token_hash.len(), 64, "token_hash is SHA-256 hex");
    assert!(token_hash.chars().all(|c| c.is_ascii_hexdigit()), "token_hash is hex");

    // No plaintext token in grant secret refs
    let grant_refs: Option<Value> = sqlx::query_scalar(
        "SELECT credential_secret_refs
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'workspace'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_optional(&pool)
    .await
    .expect("read grant refs");
    let refs_text = serde_json::to_string(&grant_refs.unwrap_or(json!([]))).expect("serialize refs");
    assert!(!refs_text.contains("minted-token"), "grant secret refs contain plaintext token");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Upstream 401 is mapped to credential_revoked (CONFLICT).
#[tokio::test]
async fn connection_token_exchange_upstream_401() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "401 Unauthorized",
        r#"{"error":"invalid_token"}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "expected CONFLICT for credential_revoked, got {status} body={body:?}");

    let (issuance_outcome, issuance_error): (String, String) = sqlx::query_as(
        "SELECT outcome, error_code
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance");
    assert_eq!(issuance_outcome, "upstream_error");
    assert_eq!(issuance_error, "credential_revoked");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Upstream returns invalid JSON → upstream_error (BAD_GATEWAY).
#[tokio::test]
async fn connection_token_exchange_invalid_upstream_json() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        "this is not valid json",
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "body={body:?}");

    let (issuance_outcome, _issuance_error): (String, String) = sqlx::query_as(
        "SELECT outcome, error_code
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance");
    assert_eq!(issuance_outcome, "upstream_error");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Upstream 200 with no token/access_token field → upstream_token_missing.
#[tokio::test]
async fn connection_token_exchange_missing_token() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"status":"ok"}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "body={body:?}");

    let (issuance_outcome, issuance_error): (String, String) = sqlx::query_as(
        "SELECT outcome, error_code
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance");
    assert_eq!(issuance_outcome, "upstream_error");
    assert_eq!(issuance_error, "upstream_token_missing");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Upstream returns wider scope than requested → upstream_scope_exceeds_requested.
#[tokio::test]
async fn connection_token_exchange_scope_expansion_rejected() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"admin-token","token_type":"Bearer","scope":["repo","admin"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "body={body:?}");

    let (issuance_outcome, issuance_error): (String, String) = sqlx::query_as(
        "SELECT outcome, error_code
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance");
    assert_eq!(issuance_outcome, "upstream_error");
    assert_eq!(issuance_error, "upstream_scope_exceeds_requested");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Requested TTL above 900s is truncated to 900s.
#[tokio::test]
async fn connection_token_exchange_ttl_truncation() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    // Custom endpoint that checks for ttlSeconds=900 (clamped, not 3600)
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind TTL test endpoint");
    let address = listener.local_addr().expect("TTL test endpoint address");
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept TTL test request");
        let mut buffer = vec![0_u8; 16_384];
        let read = socket.read(&mut buffer).await.expect("read TTL test request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        assert!(
            request.contains("\"ttlSeconds\":900"),
            "expected ttlSeconds=900 (clamped), got: {request}"
        );
        let body = r#"{"token":"ttl-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.expect("write TTL test response");
    });
    let token_uri = format!("http://{address}/token/exchange");

    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    // Request TTL=3600, but max is 900
    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 3600 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body:?}");
    let ttl = body["ttlSeconds"].as_i64().expect("ttlSeconds");
    // Effective TTL may be 899 due to sub-second clock drift during exchange
    assert!(ttl >= 895 && ttl <= 900, "expected TTL near 900, got {ttl}");
    let expires_at = body["expiresAt"].as_str().expect("expiresAt");
    let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
        .expect("parse expiresAt")
        .with_timezone(&Utc);
    let remaining = (expires_at - Utc::now()).num_seconds();
    assert!(remaining >= 1 && remaining <= 900, "expiresAt remaining={remaining}s outside 1..=900");

    let (issuance_ttl,): (Option<i32>,) = sqlx::query_as(
        "SELECT ttl_seconds
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance ttl");
    assert!(issuance_ttl.is_some());
    let stored_ttl = issuance_ttl.unwrap();
    // effective TTL may be 899 due to sub-second clock drift during exchange
    assert!(stored_ttl >= 895 && stored_ttl <= 900, "issuance ttl_seconds={stored_ttl}");

    handle.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// User-grant binding: subject_user_id on request binds to user grant.
/// User grants must be pre-created (from prior OAuth callback); the token exchange
/// path binds to an existing user grant rather than auto-creating one.
#[tokio::test]
async fn connection_token_exchange_user_grant_binding() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();
    let user_id = "user-123".to_string();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"user-bound-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;
    // Set responsible_user_id on the heartbeat run
    sqlx::query("UPDATE heartbeat_runs SET responsible_user_id = $1 WHERE id = $2")
        .bind(&user_id)
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("update run responsible user");
    // Pre-create user grant (normally created during OAuth callback)
    let user_grant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO connection_grants
            (id, company_id, connection_id, kind, subject_user_id, status, is_default,
             credential_secret_refs, created_by_agent_id)
         VALUES ($1, $2, $3, 'user', $4, 'active', false, $5, $6)",
    )
    .bind(user_grant_id)
    .bind(company_id)
    .bind(connection_id)
    .bind(&user_id)
    .bind(json!([{
        "secretId": parent_secret_id,
        "versionSelector": "latest",
        "configPath": "credentials.deploy_token",
        "required": true,
        "label": "Deploy token"
    }]))
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("insert user grant");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({
            "requestedTtlSeconds": 120,
            "subject": { "type": "user", "userId": user_id }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body:?}");
    assert_eq!(body["status"], "minted");
    assert_eq!(body["grantId"].as_str(), Some(user_grant_id.to_string().as_str()));

    let (grant_kind, grant_subject): (String, Option<String>) = sqlx::query_as(
        "SELECT kind, subject_user_id
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'user'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read user grant");
    assert_eq!(grant_kind, "user");
    assert_eq!(grant_subject.as_deref(), Some(user_id.as_str()));

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// No subject → workspace grant is auto-created and bound.
#[tokio::test]
async fn connection_token_exchange_workspace_grant_binding() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"workspace-bound-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body:?}");
    assert_eq!(body["status"], "minted");

    let (grant_kind, grant_default): (String, bool) = sqlx::query_as(
        "SELECT kind, is_default
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'workspace'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read workspace grant");
    assert_eq!(grant_kind, "workspace");
    assert!(grant_default, "workspace grant must be is_default");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// After successful exchange, grant last_used_at is updated.
#[tokio::test]
async fn connection_token_exchange_last_used_at_updated() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"last-used-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, _body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "exchange must succeed to test last_used_at");

    let last_used_at: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        "SELECT last_used_at
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'workspace'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read grant last_used_at");
    assert!(last_used_at.is_some(), "last_used_at must be set after exchange");
    let age = (Utc::now() - last_used_at.unwrap()).num_seconds();
    assert!(age >= 0 && age <= 30, "last_used_at {age}s ago, expected recent");

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Database never stores the plaintext bearer token, only a SHA-256 hash.
#[tokio::test]
async fn connection_token_exchange_token_hash_only_no_plaintext() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"secret-minted-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, _body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({ "requestedTtlSeconds": 120 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    endpoint.await.expect("token exchange endpoint task");

    // Issuance has token_hash (SHA-256), no column for plaintext token
    let token_hash: Option<String> = sqlx::query_scalar(
        "SELECT token_hash
           FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance token_hash");
    let hash = token_hash.expect("token_hash present");
    assert_eq!(hash.len(), 64, "SHA-256 hex length");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "hex only");

    // No column in connection_token_issuances for storing plain token (schema enforced)
    // Grant secret refs should not contain the minted token value
    let grant_refs: Value = sqlx::query_scalar(
        "SELECT credential_secret_refs
           FROM connection_grants
          WHERE company_id = $1 AND connection_id = $2 AND kind = 'workspace'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("read grant");
    let refs_text = serde_json::to_string(&grant_refs).expect("serialize");
    assert!(!refs_text.contains("secret-minted-token"), "plaintext token leaked into grant refs");

    // Connection config should not contain the minted token
    let config: Value = sqlx::query_scalar(
        "SELECT config FROM tool_connections WHERE id = $1 AND company_id = $2",
    )
    .bind(connection_id)
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("read connection config");
    let config_text = serde_json::to_string(&config).expect("serialize config");
    assert!(!config_text.contains("secret-minted-token"), "plaintext token leaked into connection config");

    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Issuance audit record has all required fields after success.
#[tokio::test]
async fn connection_token_exchange_issuance_audit_fields() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();
    let user_id = "audit-user".to_string();

    let (token_uri, endpoint) = spawn_connection_token_exchange_endpoint(
        "parent-token",
        "200 OK",
        r#"{"token":"audit-token","token_type":"Bearer","scope":["repo"],"expires_in":3600}"#,
    )
    .await;
    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;
    sqlx::query("UPDATE heartbeat_runs SET responsible_user_id = $1 WHERE id = $2")
        .bind(&user_id)
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("update run responsible user");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));

    let (status, body) = request_connection_route(
        &app,
        &agent,
        "POST",
        format!("/agents/me/connections/{connection_id}/token"),
        Some(json!({
            "requestedTtlSeconds": 120,
            "scope": ["repo"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body:?}");

    // Verify issuance record
    let issuance: (String, String, String, String, String, String, Option<String>, String) = sqlx::query_as(
        "SELECT connection_id::text, agent_id::text, run_id::text, path,
                outcome, COALESCE(error_code, ''), token_hash, metadata::text
           FROM connection_token_issuances
          WHERE company_id = $1 AND path = 'exchange'
          ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("read issuance");
    assert_eq!(issuance.0, connection_id.to_string(), "connection_id");
    assert_eq!(issuance.1, agent_id.to_string(), "agent_id");
    assert_eq!(issuance.2, run_id.to_string(), "run_id");
    assert_eq!(issuance.3, "exchange", "path");
    assert_eq!(issuance.4, "success", "outcome");
    assert!(issuance.5.is_empty() || issuance.5 == "", "error_code for success (got '{}')", issuance.5);
    assert!(issuance.6.is_some(), "token_hash present");
    assert!(issuance.7.contains("grantId"), "metadata has grantId");

    // Also test a denied issuance has correct error fields
    // (the failed scope-expansion test already covers this implicitly)

    endpoint.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

/// Concurrent exchange requests both succeed and create separate issuance records.
#[tokio::test]
async fn connection_token_exchange_concurrent_requests() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let parent_secret_id = Uuid::new_v4();

    // Multi-accept endpoint for concurrent tests
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind concurrent exchange endpoint");
    let address = listener.local_addr().expect("concurrent exchange address");
    let handle = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.expect("accept concurrent request");
            let mut buffer = vec![0_u8; 16_384];
            let read = socket.read(&mut buffer).await.expect("read concurrent request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /token/exchange"), "expected exchange: {request}");
            let body = r#"{"token":"concurrent-token","token_type":"Bearer","scope":["repo"],"expires_in":900}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.expect("write concurrent response");
        }
    });
    let token_uri = format!("http://{address}/token/exchange");

    insert_token_exchange_fixture(
        &pool, company_id, agent_id, run_id, connection_id, parent_secret_id, &token_uri,
    )
    .await;

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let agent = AuthorizationActor::agent(agent_id, company_id, Some(run_id));
    let uri = format!("/agents/me/connections/{connection_id}/token");
    let body = Some(json!({ "requestedTtlSeconds": 120 }));

    let (first, second) = tokio::join!(
        request_connection_route(&app, &agent, "POST", uri.clone(), body.clone()),
        request_connection_route(&app, &agent, "POST", uri.clone(), body.clone()),
    );
    assert_eq!(first.0, StatusCode::OK, "first={:?}", first.1);
    assert_eq!(second.0, StatusCode::OK, "second={:?}", second.1);
    assert_eq!(first.1["status"], "minted");
    assert_eq!(second.1["status"], "minted");

    // Both should reference the same grant (workspace default)
    let first_grant = first.1["grantId"].as_str().map(ToOwned::to_owned);
    let second_grant = second.1["grantId"].as_str().map(ToOwned::to_owned);
    assert_eq!(first_grant, second_grant, "same grantId for concurrent requests");

    // Two separate issuance records
    let issuance_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM connection_token_issuances
          WHERE company_id = $1 AND connection_id = $2 AND path = 'exchange'",
    )
    .bind(company_id)
    .bind(connection_id)
    .fetch_one(&pool)
    .await
    .expect("count issuances");
    assert_eq!(issuance_count, 2, "two concurrent requests produce two issuance records");

    handle.await.expect("token exchange endpoint task");
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn review_new_tools_persists_decisions_and_entries() {
    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let profile_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();
    let application_id = Uuid::new_v4();
    let allow_entry_id = Uuid::new_v4();
    let block_entry_id = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Review Tools Company")
        .bind(format!("RT{}", &company_id.simple().to_string()[..8]))
        .execute(&pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO tool_applications (id, company_id, application_key, name)
         VALUES ($1, $2, 'review-app', 'Review App')",
    )
    .bind(application_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert application");
    sqlx::query(
        "INSERT INTO tool_profiles (id, company_id, profile_key, name, description)
         VALUES ($1, $2, 'review-profile', 'Review profile', 'review test')",
    )
    .bind(profile_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .expect("insert profile");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, transport, auth_kind, status, enabled, config)
         VALUES ($1, $2, $3, 'Review connection', 'review-conn', 'mcp_remote', 'none', 'active', true, '{}'::jsonb)",
    )
    .bind(connection_id)
    .bind(company_id)
    .bind(application_id)
    .execute(&pool)
    .await
    .expect("insert connection");
    for (entry_id, tool_name) in [(allow_entry_id, "review.allow_me"), (block_entry_id, "review.block_me")] {
        sqlx::query(
            "INSERT INTO tool_catalog_entries
                (id, company_id, connection_id, name, tool_name, status, risk_level, version_hash, first_seen_at, last_seen_at)
             VALUES ($1, $2, $3, $4, $5, 'active', 'low', 'review-test-hash', NOW(), NOW())",
        )
        .bind(entry_id)
        .bind(company_id)
        .bind(connection_id)
        .bind(tool_name)
        .bind(tool_name)
        .execute(&pool)
        .await
        .expect("insert catalog entry");
    }

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner_user_id = Uuid::new_v4();
    let owner = AuthorizationActor::board_with_source(
        owner_user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            owner_user_id,
            MembershipRole::Owner,
        )],
        false,
    );

    // Partial coverage must be rejected (Paperclip: decisions must cover every
    // pending tool exactly once).
    let (status, _) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/new-tools/review"),
        Some(json!({"decisions": [
            {"catalogEntryId": allow_entry_id, "decision": "allow"}
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "partial coverage must 400");

    // Duplicates must be rejected.
    let (status, _) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/new-tools/review"),
        Some(json!({"decisions": [
            {"catalogEntryId": allow_entry_id, "decision": "allow"},
            {"catalogEntryId": allow_entry_id, "decision": "allow"}
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "duplicate decisions must 400");

    // Full coverage succeeds: allow one, keep the other blocked.
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "POST",
        format!("/tool-profiles/{profile_id}/new-tools/review"),
        Some(json!({"decisions": [
            {"catalogEntryId": allow_entry_id, "decision": "allow"},
            {"catalogEntryId": block_entry_id, "decision": "keep_blocked"}
        ]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "review={body:?}");
    assert_eq!(body.get("allowedCount").and_then(Value::as_i64), Some(1));
    assert_eq!(body.get("keptBlockedCount").and_then(Value::as_i64), Some(1));
    assert_eq!(body.get("entriesCreated").and_then(Value::as_i64), Some(1));

    // Allowed entry created a catalog_entry/include profile entry.
    let entry_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_profile_entries
          WHERE profile_id = $1 AND catalog_entry_id = $2 AND selector_type = 'catalog_entry' AND effect = 'include'",
    )
    .bind(profile_id)
    .bind(allow_entry_id)
    .fetch_one(&pool)
    .await
    .expect("count created profile entries");
    assert_eq!(entry_count, 1);

    // Both entries carry reviewed_at; attribution went to the board user.
    let (reviewed, by_agent, by_user): (Option<chrono::DateTime<chrono::Utc>>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT reviewed_at, reviewed_by_agent_id, reviewed_by_user_id FROM tool_catalog_entries WHERE id = $1",
    )
    .bind(allow_entry_id)
    .fetch_one(&pool)
    .await
    .expect("load reviewed entry");
    assert!(reviewed.is_some(), "reviewed_at must be set");
    assert!(by_agent.is_none(), "board review must not set agent attribution");
    assert_eq!(
        by_user.as_deref(),
        Some(owner_user_id.to_string().as_str()),
        "board review must set user attribution"
    );

    // Paperclip also stamps the profile-level review timestamp.
    let profile_reviewed: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT new_tools_reviewed_at FROM tool_profiles WHERE id = $1",
    )
    .bind(profile_id)
    .fetch_one(&pool)
    .await
    .expect("load profile review timestamp");
    assert!(profile_reviewed.is_some(), "new_tools_reviewed_at must be set");

    // GET /new-tools no longer lists the reviewed entries.
    let (status, body) = request_connection_route(
        &app,
        &owner,
        "GET",
        format!("/tool-profiles/{profile_id}/new-tools"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "new tools after review={body:?}");
    assert_eq!(body.as_array().map(Vec::len), Some(0), "pending list must be empty");
}

/// Deterministic fake MCP upstream serving a fixed tools/list response; each
/// call consumes one request.
async fn spawn_tools_list_endpoint(response_body: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind tools/list endpoint");
    let address = listener.local_addr().expect("tools/list endpoint address");
    let handle = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.expect("accept tools/list request");
            let mut buffer = vec![0_u8; 16_384];
            let _ = socket.read(&mut buffer).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.expect("write tools/list response");
        }
    });
    (format!("http://{address}/mcp"), handle)
}

/// Catalog refresh must derive version_hash/schema_hash from descriptor
/// content (Paperclip stableHash), so two refreshes of an unchanged upstream
/// produce identical hashes — the change-detection invariant the previous
/// random-UUID implementation broke.
#[tokio::test]
async fn catalog_refresh_version_hash_is_content_stable() {
    let (base_url, server) = spawn_tools_list_endpoint(
        r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"kv_get","title":"Get value","description":"Reads a key","inputSchema":{"type":"object","properties":{"key":{"type":"string"}}},"annotations":{"readOnlyHint":true}}]}}"#,
    ).await;

    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let application_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id).bind("Snapshot Company").bind(format!("SC{}", &company_id.simple().to_string()[..8]))
        .execute(&pool).await.expect("insert company");
    sqlx::query(
        "INSERT INTO tool_applications (id, company_id, name) VALUES ($1, $2, 'Snapshot App')",
    ).bind(application_id).bind(company_id).execute(&pool).await.expect("insert application");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, transport, auth_kind, status, enabled, config, transport_config)
         VALUES ($1, $2, $3, 'Snapshot connection', 'snapshot-conn', 'mcp_remote', 'none', 'active', true, '{}'::jsonb, $4)",
    )
    .bind(connection_id).bind(company_id).bind(application_id)
    .bind(json!({"endpoint": base_url}))
    .execute(&pool).await.expect("insert connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);

    let mut first: Option<(String, String)> = None;
    for round in 1..=2 {
        let (status, body) = request_connection_route(
            &app,
            &owner,
            "POST",
            format!("/tool-connections/{connection_id}/catalog/refresh"),
            None,
        ).await;
        assert_eq!(status, StatusCode::OK, "refresh round {round}={body:?}");
        let row: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT version_hash, schema_hash FROM tool_catalog_entries WHERE connection_id = $1",
        )
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .expect("load catalog entry");
        let hash = row.0.expect("version_hash must be persisted");
        let schema = row.1.expect("schema_hash must be persisted");
        assert_eq!(hash.len(), 64, "sha256 hex");
        if round == 1 {
            first = Some((hash, schema));
        } else {
            // Same upstream content -> identical hashes across refreshes
            // (the invariant broken by the previous random-UUID hash).
            assert_eq!(first, Some((hash, schema)));
        }
    }

    server.abort();
}

/// Paperclip refreshCatalog: with config.quarantineNewEntries, new entries land
/// as quarantined/pending_review; re-refresh of unchanged content keeps them
/// quarantined; safeDefault exempts read-only tools.
#[tokio::test]
async fn catalog_refresh_quarantines_new_entries_per_paperclip() {
    let (base_url, server) = spawn_tools_list_endpoint(
        r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"kv_get","title":"Get value","description":"Reads a key","inputSchema":{"type":"object","properties":{"key":{"type":"string"}}},"annotations":{"readOnlyHint":true}},{"name":"kv_delete","title":"Delete value","description":"Removes a key","inputSchema":{"type":"object","properties":{"key":{"type":"string"}}},"annotations":{}}]}}"#,
    ).await;

    let pool = connect_and_migrate().await;
    let company_id = Uuid::new_v4();
    let application_id = Uuid::new_v4();
    let connection_id = Uuid::new_v4();

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id).bind("Quarantine Company").bind(format!("QC{}", &company_id.simple().to_string()[..8]))
        .execute(&pool).await.expect("insert company");
    sqlx::query(
        "INSERT INTO tool_applications (id, company_id, name) VALUES ($1, $2, 'Quarantine App')",
    ).bind(application_id).bind(company_id).execute(&pool).await.expect("insert application");
    sqlx::query(
        "INSERT INTO tool_connections
            (id, company_id, application_id, name, uid, transport, auth_kind, status, enabled, config, transport_config)
         VALUES ($1, $2, $3, 'Quarantine connection', 'quarantine-conn', 'mcp_remote', 'none', 'active', true, $4, $5)",
    )
    .bind(connection_id).bind(company_id).bind(application_id)
    .bind(json!({"quarantineNewEntries": true}))
    .bind(json!({"endpoint": base_url}))
    .execute(&pool).await.expect("insert connection");

    let state = build_app_state(pool.clone()).await.expect("build app state");
    let app = tool_access_routes().with_state(state);
    let owner = board_actor_with_role(company_id, MembershipRole::Owner);

    let (status, body) = request_connection_route(
        &app, &owner, "POST",
        format!("/tool-connections/{connection_id}/catalog/refresh"),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "refresh={body:?}");

    // Both new entries are quarantined pending review (read-only does NOT get
    // exempted without safeDefault).
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT tool_name, status FROM tool_catalog_entries WHERE connection_id = $1 ORDER BY tool_name",
    ).bind(connection_id).fetch_all(&pool).await.expect("load entries");
    assert_eq!(rows.len(), 2, "both tools persisted");
    for (tool_name, status) in &rows {
        assert_eq!(status, "quarantined", "{tool_name} must be quarantined");
    }
    let reason: Vec<String> = sqlx::query_scalar(
        "SELECT quarantine_reason FROM tool_catalog_entries WHERE connection_id = $1 AND quarantine_reason IS NOT NULL",
    ).bind(connection_id).fetch_all(&pool).await.expect("load reasons");
    assert_eq!(reason.len(), 2);
    for r in reason {
        assert_eq!(r, "pending_review");
    }

    // Second refresh of identical content: still quarantined (no state churn).
    let (status, _) = request_connection_route(
        &app, &owner, "POST",
        format!("/tool-connections/{connection_id}/catalog/refresh"),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK);
    let still: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_catalog_entries WHERE connection_id = $1 AND status = 'quarantined'",
    ).bind(connection_id).fetch_one(&pool).await.expect("count quarantined");
    assert_eq!(still, 2, "unchanged entries keep quarantine state");

    server.abort();
}
