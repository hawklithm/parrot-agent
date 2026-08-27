//! HTTP parity tests for activity/audit routes.
//!
//! Tests stand up the real Axum router with a real AppState and inject
//! AuthorizationActor extensions, bypassing the global auth middleware.

use api::routes::activity::activity_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@127.0.0.1:5433/parrot_agent_compile".to_string()
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

async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    let cid = Uuid::new_v4();
    let uid = Uuid::new_v4();
    let prefix = format!("AC{}", &cid.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(cid).bind("Activity Test").bind(prefix)
        .execute(pool).await.expect("insert company");
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'Activity User') ON CONFLICT DO NOTHING")
        .bind(uid).bind(format!("{uid}@test.example"))
        .execute(pool).await.expect("insert auth_user");
    sqlx::query("INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active') ON CONFLICT DO NOTHING")
        .bind(cid).bind(uid)
        .execute(pool).await.expect("insert membership");
    (cid, uid)
}

fn owner_actor(company_id: Uuid, user_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id, company_id, ActorSource::Session,
        vec![CompanyMembership::new(company_id, PrincipalType::User, user_id, MembershipRole::Owner)],
        false,
    )
}

async fn get(app: &axum::Router, uri: &str, actor: &AuthorizationActor) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let s = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (s, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn post(app: &axum::Router, uri: &str, actor: &AuthorizationActor, body_val: Value) -> (StatusCode, Value) {
    let mut req = Request::builder().method("POST").uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body_val).unwrap())).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let s = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (s, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn activity_list_company_requires_auth() {
    let pool = connect_and_migrate().await;
    let (cid, _uid) = seed(&pool).await;
    let state = build_app_state(pool).await.unwrap();
    let app = activity_routes().with_state(state);
    let anonymous = AuthorizationActor::none();

    let (status, _body) = get(&app, &format!("/companies/{cid}/activity"), &anonymous).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "anonymous should be rejected, got {status}"
    );
}

#[tokio::test]
async fn activity_list_company_returns_empty_list() {
    let pool = connect_and_migrate().await;
    let (cid, uid) = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = activity_routes().with_state(state);
    let actor = owner_actor(cid, uid);

    let (status, body) = get(&app, &format!("/companies/{cid}/activity"), &actor).await;
    assert_eq!(status, StatusCode::OK, "list activity={body:?}");
    assert!(body.as_array().map_or(true, |a| a.is_empty()), "empty list: {body:?}");
}

#[tokio::test]
async fn activity_create_and_list() {
    let pool = connect_and_migrate().await;
    let (cid, uid) = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = activity_routes().with_state(state);
    let actor = owner_actor(cid, uid);

    let (status, body) = post(&app, &format!("/companies/{cid}/activity"), &actor, json!({
        "actor_type": "user",
        "actor_id": uid.to_string(),
        "action": "test.event",
        "entity_type": "issue",
        "entity_id": Uuid::new_v4().to_string(),
        "details": {"key": "value"}
    })).await;
    if status != 201 {
        panic!("create failed with {status}: payload={body:?}");
    }

    let (status, body) = get(&app, &format!("/companies/{cid}/activity"), &actor).await;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().unwrap();
    assert!(!items.is_empty(), "list has items: {body:?}");
    assert!(items.iter().any(|i| i.get("action").and_then(Value::as_str) == Some("test.event")),
        "created event visible in list");
}

#[tokio::test]
async fn activity_cross_company_isolation() {
    let pool = connect_and_migrate().await;
    let (cid_a, uid_a) = seed(&pool).await;
    let (cid_b, uid_b) = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = activity_routes().with_state(state);
    let actor_a = owner_actor(cid_a, uid_a);
    let actor_b = owner_actor(cid_b, uid_b);

    let (status, _) = post(&app, &format!("/companies/{cid_a}/activity"), &actor_a, json!({
        "actor_type": "user",
        "actor_id": uid_a,
        "action": "company_a.event",
        "entity_type": "issue",
        "entity_id": Uuid::new_v4(),
        "details": {}
    })).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = get(&app, &format!("/companies/{cid_a}/activity"), &actor_a).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_array().unwrap().iter().any(|i|
        i.get("action").and_then(Value::as_str) == Some("company_a.event")));

    let (status, body) = get(&app, &format!("/companies/{cid_b}/activity"), &actor_b).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_array().unwrap().iter().all(|i|
        i.get("action").and_then(Value::as_str) != Some("company_a.event")),
        "company_b should not see company_a events");
}
