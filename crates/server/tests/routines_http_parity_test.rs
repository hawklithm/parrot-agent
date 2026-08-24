//! HTTP parity integration test proving independent Routine persistence
//! (#paperclip routines migration). Creates a routine through the real router
//! and reads it back, asserting the row actually landed in `routines` — the
//! "独立 Routine 实际持久化" acceptance.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::routines::routine_routes;
use parrot_server::build_app_state;
use services::auth::AuthorizationActor;

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

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    agent_a: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let prefix = format!("RT{}", &company_a.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a)
        .bind("Routines Parity Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
        .bind(agent_a)
        .bind(company_a)
        .bind("Backup agent")
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
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(f.company_a)
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
        .expect("connect database for routines HTTP parity tests");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

/// Independent routine creation persists a `routines` row and reads back.
#[tokio::test]
async fn create_independent_routine_persists_row() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = routine_routes().with_state(state);
    let board = board_actor(Uuid::new_v4(), f.company_a);

    let payload = json!({
        "agent_id": f.agent_a.to_string(),
        "name": "Nightly backup",
        "description": "Run the backup job every night.",
    });

    let (status, body) = send(
        &app,
        &board,
        "POST",
        &format!("/companies/{}/routines", f.company_a),
        Some(payload),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create independent routine → 201");
    let created = parse(&body);
    let routine_id = created["id"].as_str().expect("routine id").to_string();
    assert_eq!(created["name"], "Nightly backup");
    assert_eq!(created["assigneeAgentId"], f.agent_a.to_string(), "assignee persisted");

    // Detail read-back proves the row actually landed.
    let (status, body) = send(&app, &board, "GET", &format!("/routines/{}", routine_id), None).await;
    assert_eq!(status, StatusCode::OK, "routine detail → 200");
    let detail = parse(&body);
    assert_eq!(detail["name"], "Nightly backup");

    // Direct DB assertion (belt-and-suspenders for "实际持久化").
    let db_name: Option<String> = sqlx::query_scalar("SELECT name FROM routines WHERE id = $1")
        .bind(Uuid::parse_str(&routine_id).expect("parse routine id"))
        .fetch_optional(&pool)
        .await
        .expect("query routines");
    assert_eq!(db_name.as_deref(), Some("Nightly backup"), "row present in DB");

    cleanup_fixture(&f).await;
}
