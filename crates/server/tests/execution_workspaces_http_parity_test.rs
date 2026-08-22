//! HTTP parity integration tests for Paperclip's Execution Workspace routes
//! (#165 read model / #172 runtime control).
//!
//! The workspace list/get responses carry `runtimeServices` (provision status,
//! service ports, health state) backed by `workspace_runtime_services`, and the
//! `runtime-services/:action` control endpoint accepts start/stop/restart,
//! records a `workspace_operations` row and transitions the service state.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile \
//!     cargo test -p parrot-server --test execution_workspaces_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::execution_workspaces::execution_workspace_routes;
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
    project_a: Uuid,
    workspace_a: Uuid,
    service_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let project_a = Uuid::new_v4();
    let workspace_a = Uuid::new_v4();
    let service_a = Uuid::new_v4();
    let prefix = format!("EX{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Execution Workspace Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO projects (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(project_a)
        .bind(company_a)
        .bind("Parity project")
        .execute(pool)
        .await
        .expect("insert project");
    sqlx::query(
        "INSERT INTO execution_workspaces \
            (id, company_id, project_id, mode, strategy_type, name) \
         VALUES ($1, $2, $3, 'local', 'isolated', $4)",
    )
    .bind(workspace_a)
    .bind(company_a)
    .bind(project_a)
    .bind("Parity workspace")
    .execute(pool)
    .await
    .expect("insert execution workspace");
    sqlx::query(
        "INSERT INTO workspace_runtime_services \
            (id, company_id, execution_workspace_id, scope_type, service_name, status, \
             lifecycle, provider, port, health_status) \
         VALUES ($1, $2, $3, 'execution_workspace', 'api', 'running', 'ephemeral', \
                 'local_process', 8080, 'healthy')",
    )
    .bind(service_a)
    .bind(company_a)
    .bind(workspace_a)
    .execute(pool)
    .await
    .expect("insert runtime service");

    Fixture {
        pool: pool.clone(),
        company_a,
        project_a,
        workspace_a,
        service_a,
    }
}

async fn cleanup_fixture(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM workspace_operations WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM workspace_runtime_services WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM execution_workspaces WHERE company_id = $1")
        .bind(f.company_a)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(f.project_a)
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
        .expect("connect database for execution workspace HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// #165 (runtime-services read model) + #172 (runtime control) acceptance.
#[tokio::test]
async fn execution_workspace_read_model_and_runtime_control_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = execution_workspace_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    // 1. List carries the workspace with its runtimeServices (port/status/health).
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/companies/{}/execution-workspaces", f.company_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "workspace list → 200");
    let list = parse(&body);
    let workspaces = list.as_array().expect("list is an array");
    assert_eq!(workspaces.len(), 1);
    let services = workspaces[0]["runtimeServices"].as_array().expect("runtimeServices");
    assert_eq!(services.len(), 1);
    assert_eq!(services[0]["serviceName"], "api");
    assert_eq!(services[0]["port"], 8080);
    assert_eq!(services[0]["status"], "running");
    assert_eq!(services[0]["healthStatus"], "healthy");

    // 2. Detail returns the same runtime-services projection.
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/execution-workspaces/{}", f.workspace_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "workspace detail → 200");
    let detail = parse(&body);
    assert_eq!(detail["name"], "Parity workspace");
    let services = detail["runtimeServices"].as_array().expect("detail runtimeServices");
    assert_eq!(services.len(), 1);
    assert_eq!(services[0]["id"], f.service_a.to_string());
    assert_eq!(services[0]["scopeType"], "execution_workspace");
    assert_eq!(services[0]["provider"], "local_process");

    // 3. stop transitions the service state and records an operation.
    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/execution-workspaces/{}/runtime-services/stop", f.workspace_a),
        Some(json!({ "runtimeServiceId": f.service_a })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "runtime stop → 200");
    let stop = parse(&body);
    assert_eq!(stop["action"], "stop");
    assert_eq!(stop["accepted"], true);
    assert!(!stop["operationId"].as_str().unwrap_or("").is_empty());
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/execution-workspaces/{}", f.workspace_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parse(&body)["runtimeServices"][0]["status"], "stopped",
        "stop transitions the runtime service to stopped"
    );

    // 4. start brings it back to running.
    let (status, _) = send(
        &app,
        &board,
        "POST",
        &format!("/execution-workspaces/{}/runtime-services/start", f.workspace_a),
        Some(json!({ "runtimeServiceId": f.service_a })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "runtime start → 200");
    let (status, body) = send(
        &app,
        &board,
        "GET",
        &format!("/execution-workspaces/{}", f.workspace_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        parse(&body)["runtimeServices"][0]["status"], "running",
        "start transitions the runtime service back to running"
    );

    // 5. Unsupported action is rejected.
    let (status, _) = send(
        &app,
        &board,
        "POST",
        &format!("/execution-workspaces/{}/runtime-services/frobnicate", f.workspace_a),
        Some(json!({ "runtimeServiceId": f.service_a })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown runtime action → 404");

    // 6. A board from another company cannot read the workspace (403).
    let outsider = session_board_actor(Uuid::new_v4(), Uuid::new_v4());
    let (status, _) = send(
        &app,
        &outsider,
        "GET",
        &format!("/execution-workspaces/{}", f.workspace_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company workspace read → 403");

    cleanup_fixture(&f).await;
}
