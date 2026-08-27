//! HTTP parity tests for inbox dismissal routes.

use api::routes::inbox_dismissals::inbox_dismissal_routes;
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

async fn seed(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let prefix = format!("ID{}", &id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id).bind("Inbox Dismissal Test").bind(prefix)
        .execute(pool).await.expect("insert company");
    id
}

fn owner(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    let uid = Uuid::new_v4();
    (AuthorizationActor::board_with_source(uid, company_id, ActorSource::Session,
        vec![CompanyMembership::new(company_id, PrincipalType::User, uid, MembershipRole::Owner)], false), uid)
}

fn viewer(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    let uid = Uuid::new_v4();
    (AuthorizationActor::board_with_source(uid, company_id, ActorSource::Session,
        vec![CompanyMembership::new(company_id, PrincipalType::User, uid, MembershipRole::Viewer)], false), uid)
}

async fn ensure_auth_user(pool: &PgPool, uid: Uuid) {
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'Test User') ON CONFLICT DO NOTHING")
        .bind(uid).bind(format!("{uid}@test.example"))
        .execute(pool).await.expect("insert auth_user");
}

async fn get(app: &axum::Router, uri: &str, actor: &AuthorizationActor) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn post(app: &axum::Router, uri: &str, actor: &AuthorizationActor, body_val: Value) -> (StatusCode, Value) {
    let mut req = Request::builder().method("POST").uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body_val).unwrap())).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn delete(app: &axum::Router, uri: &str, actor: &AuthorizationActor) -> StatusCode {
    let mut req = Request::builder().method("DELETE").uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut().insert(actor.clone());
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn inbox_dismissal_lifecycle() {
    let pool = connect_and_migrate().await;
    let cid = seed(&pool).await;
    let (actor, uid) = owner(cid);
    ensure_auth_user(&pool, uid).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = inbox_dismissal_routes().with_state(state);
    let base = format!("/companies/{cid}/inbox-dismissals");

    let (status, body) = get(&app, &base, &actor).await;
    assert_eq!(status, 200);
    assert!(body.as_array().unwrap().is_empty());

    let (status, body) = post(&app, &base, &actor, json!({"itemKey": "approval:test-123", "kind": "dismiss"})).await;
    if status != 201 {
        panic!("create failed with {status}: {body:?}");
    }
    assert_eq!(body["itemKey"], "approval:test-123");

    let (status, body) = get(&app, &base, &actor).await;
    assert_eq!(status, 200);
    assert_eq!(body.as_array().unwrap().len(), 1);

    let status = delete(&app, &format!("{base}/approval:test-123"), &actor).await;
    assert_eq!(status, 204);

    let (status, body) = get(&app, &base, &actor).await;
    assert_eq!(status, 200);
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn inbox_dismissal_rejects_invalid_key() {
    let pool = connect_and_migrate().await;
    let cid = seed(&pool).await;
    let (actor, uid) = owner(cid);
    ensure_auth_user(&pool, uid).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = inbox_dismissal_routes().with_state(state);
    let base = format!("/companies/{cid}/inbox-dismissals");

    let (status, _) = post(&app, &base, &actor, json!({"itemKey": "invalid:", "kind": "dismiss"})).await;
    assert_eq!(status, 400);
}

#[tokio::test]
async fn inbox_dismissal_user_isolation() {
    let pool = connect_and_migrate().await;
    let cid = seed(&pool).await;
    let (user_a, uida) = owner(cid);
    let (user_b, uidb) = viewer(cid);
    ensure_auth_user(&pool, uida).await;
    ensure_auth_user(&pool, uidb).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = inbox_dismissal_routes().with_state(state);
    let base = format!("/companies/{cid}/inbox-dismissals");

    let (status, _) = post(&app, &base, &user_a, json!({"itemKey": "approval:a-only", "kind": "dismiss"})).await;
    assert_eq!(status, 201);

    let (status, body) = get(&app, &base, &user_b).await;
    assert_eq!(status, 200);
    assert!(body.as_array().unwrap().is_empty(), "user b should not see user a's dismissals");
}

#[tokio::test]
async fn inbox_dismissal_agent_is_forbidden() {
    let pool = connect_and_migrate().await;
    let cid = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = inbox_dismissal_routes().with_state(state);
    let base = format!("/companies/{cid}/inbox-dismissals");
    let agent = AuthorizationActor::agent(Uuid::new_v4(), cid, None);

    let (s, _) = get(&app, &base, &agent).await;
    assert_eq!(s, 403);
    let (s, _) = post(&app, &base, &agent, json!({"itemKey": "approval:x", "kind": "dismiss"})).await;
    assert_eq!(s, 403);
    let s = delete(&app, &format!("{base}/approval:x"), &agent).await;
    assert_eq!(s, 403);
}
