//! Integration tests for run ledger (L578).
//!
//! Tests exercise list/get/cancel/isolation/token aggregation/error code
//! persistence through the live Axum router and PostgreSQL.

use api::routes::heartbeat_runs::heartbeat_run_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use parrot_server::build_app_state;
use serde_json::{json, Map, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use uuid::Uuid;
use tower::util::ServiceExt;

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@localhost:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let prefix = format!("RL{}", &company_id.simple().to_string()[..8]);

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_id)
    .bind("RLTest")
    .bind(prefix)
    .execute(pool)
    .await
    .expect("insert company");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'RL User') ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("rl{}@test.com", user_id))
    .execute(pool)
    .await
    .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user'::principal_type, $2, 'owner'::membership_role, 'active'::company_membership_status) ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, 'http') ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("RL Agent")
    .execute(pool)
    .await
    .expect("insert agent");

    Fixture {
        pool: pool.clone(),
        company_id,
        user_id,
        agent_id,
    }
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        fixture.user_id,
        fixture.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            fixture.company_id,
            PrincipalType::User,
            fixture.user_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

async fn send(
    app: &axum::Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request_body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut request = builder.body(request_body).expect("build request");
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
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(json!({}))
    };
    (status, json)
}

#[tokio::test]
async fn test_list_heartbeat_runs_empty() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/heartbeat-runs", fixture.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let runs = body.as_array().unwrap();
    assert_eq!(runs.len(), 0, "body: {:?}", body);
}

#[tokio::test]
async fn test_list_heartbeat_runs_with_runs() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let result_json = json!({
        "inputTokens": 1000,
        "outputTokens": 500,
        "cachedInputTokens": 200,
        "costUsd": 0.01
    });

    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, started_at, finished_at, result_json) VALUES ($1, $2, $3, 'succeeded'::heartbeat_run_status, $4, $5, $6::jsonb)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(now)
    .bind(now)
    .bind(&result_json.to_string())
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/heartbeat-runs", fixture.company_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    let runs = body.as_array().unwrap();
    assert_eq!(runs.len(), 1, "body: {:?}", body);
    let run = runs.get(0).expect("run exists");
    assert_eq!(run["id"], run_id.to_string());
    assert_eq!(run["status"], "succeeded");
    assert!(run["usageJson"].is_object());
    let usage = run["usageJson"].as_object().unwrap();
    assert_eq!(usage["inputTokens"], 1000, "body: {:?}", body);
    assert_eq!(usage["outputTokens"], 500, "body: {:?}", body);
    assert_eq!(usage["cachedInputTokens"], 200, "body: {:?}", body);
    assert_eq!(usage["costUsd"], 0.01, "body: {:?}", body);
}

#[tokio::test]
async fn test_get_heartbeat_run() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'running'::heartbeat_run_status)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["id"], run_id.to_string());
    assert_eq!(body["status"], "running");
}

#[tokio::test]
async fn test_cancel_heartbeat_run() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'running'::heartbeat_run_status)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!("/heartbeat-runs/{}/cancel", run_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {:?}", body);
    assert_eq!(body["status"], "cancelled", "body: {:?}", body);
}

#[tokio::test]
async fn test_company_isolation() {
    let pool = connect_and_migrate().await;
    let company_a_id = Uuid::new_v4();
    let company_b_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_a_id = Uuid::new_v4();
    let agent_b_id = Uuid::new_v4();
    let prefix_a = format!("RA{}", &company_a_id.simple().to_string()[..8]);
    let prefix_b = format!("RB{}", &company_b_id.simple().to_string()[..8]);

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_a_id)
    .bind("CmpA")
    .bind(&prefix_a)
    .execute(&pool)
    .await
    .expect("insert company A");

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_b_id)
    .bind("CmpB")
    .bind(&prefix_b)
    .execute(&pool)
    .await
    .expect("insert company B");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'User A') ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind("a@test.com")
    .execute(&pool)
    .await
    .expect("insert user A");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user'::principal_type, $2, 'owner'::membership_role, 'active'::company_membership_status) ON CONFLICT DO NOTHING",
    )
    .bind(company_a_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert membership A");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user'::principal_type, $2, 'owner'::membership_role, 'active'::company_membership_status) ON CONFLICT DO NOTHING",
    )
    .bind(company_b_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert membership B");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, 'http') ON CONFLICT DO NOTHING",
    )
    .bind(agent_a_id)
    .bind(company_a_id)
    .bind("Agent A")
    .execute(&pool)
    .await
    .expect("insert agent A");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, 'http') ON CONFLICT DO NOTHING",
    )
    .bind(agent_b_id)
    .bind(company_b_id)
    .bind("Agent B")
    .execute(&pool)
    .await
    .expect("insert agent B");

    let run_a_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'succeeded'::heartbeat_run_status)",
    )
    .bind(run_a_id)
    .bind(company_a_id)
    .bind(agent_a_id)
    .execute(&pool)
    .await
    .expect("insert run A");

    let run_b_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'running'::heartbeat_run_status)",
    )
    .bind(run_b_id)
    .bind(company_b_id)
    .bind(agent_b_id)
    .execute(&pool)
    .await
    .expect("insert run B");

    let app_state = build_app_state(pool.clone()).await.unwrap();

    let actor_a = AuthorizationActor::board_with_source(
        user_id,
        company_a_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_a_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    );

    let app_a = heartbeat_run_routes().with_state(app_state.clone());
    let (status, body) = send(
        &app_a,
        &actor_a,
        "GET",
        &format!("/companies/{}/heartbeat-runs", company_a_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "isolation A: {:?}", body);
    let runs = body.as_array().unwrap();
    assert_eq!(runs.len(), 1, "isolation A: {:?}", body);
    assert_eq!(runs[0]["id"], run_a_id.to_string());

    let actor_b = AuthorizationActor::board_with_source(
        user_id,
        company_b_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_b_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    );

    let app_b = heartbeat_run_routes().with_state(app_state);
    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/companies/{}/heartbeat-runs", company_b_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "isolation B: {:?}", body);
    let runs = body.as_array().unwrap();
    assert_eq!(runs.len(), 1, "isolation B: {:?}", body);
    assert_eq!(runs[0]["id"], run_b_id.to_string());
}

#[tokio::test]
async fn test_token_aggregation_in_usage_json() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    let result_json = json!({
        "inputTokens": 100,
        "outputTokens": 50,
        "cacheReadInputTokens": 25,
        "totalCostUsd": 0.005
    });

    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, result_json) VALUES ($1, $2, $3, 'succeeded'::heartbeat_run_status, $4::jsonb)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind(&result_json.to_string())
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (_status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

let usage = body.get("usageJson").and_then(|v| v.as_object()).expect("usageJson");
    assert_eq!(usage["inputTokens"], 100, "body: {:?}", body);
    assert_eq!(usage["outputTokens"], 50, "body: {:?}", body);
    assert_eq!(usage["cachedInputTokens"], 25, "body: {:?}", body);
    assert_eq!(usage["costUsd"], 0.005, "body: {:?}", body);
}

#[tokio::test]
async fn test_usage_json_null_when_no_tokens() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status) VALUES ($1, $2, $3, 'running'::heartbeat_run_status)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (_status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

    assert!(body["usageJson"].is_null(), "body: {:?}", body);
}

#[tokio::test]
async fn test_error_code_persistence() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;
    let pool = &fixture.pool;

    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, error) VALUES ($1, $2, $3, 'failed'::heartbeat_run_status, $4)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(fixture.agent_id)
    .bind("AdaptersHttpNotFound")
    .execute(pool)
    .await
    .expect("insert run");

    let app_state = build_app_state(pool.clone()).await.unwrap();
    let app = heartbeat_run_routes().with_state(app_state);
    let actor = owner_actor(&fixture);

    let (_status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/heartbeat-runs/{}", run_id),
        None,
    )
    .await;

    assert_eq!(body["status"], "failed", "body: {:?}", body);
    assert_eq!(body["error"], "AdaptersHttpNotFound", "body: {:?}", body);
}
