//! HTTP parity tests for inbox agent policy routes.
//!
//! Tests stand up a real Axum router with a real AppState and inject
//! AuthorizationActor extensions, bypassing the global auth middleware.

use api::routes::automation_misc::automation_misc_routes;
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

async fn seed_company(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let prefix = format!("IA{}", &id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id).bind("Inbox Agent Policy Test").bind(prefix)
        .execute(pool).await.expect("insert company");
    id
}

fn owner_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    let uid = Uuid::new_v4();
    (AuthorizationActor::board_with_source(uid, company_id, ActorSource::Session,
        vec![CompanyMembership::new(company_id, PrincipalType::User, uid, MembershipRole::Owner)], false), uid)
}

fn viewer_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    let uid = Uuid::new_v4();
    (AuthorizationActor::board_with_source(uid, company_id, ActorSource::Session,
        vec![CompanyMembership::new(company_id, PrincipalType::User, uid, MembershipRole::Viewer)], false), uid)
}

fn agent_actor(company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(Uuid::new_v4(), company_id, None)
}

async fn ensure_auth_user(pool: &PgPool, uid: Uuid) {
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'Test User') ON CONFLICT DO NOTHING")
        .bind(uid).bind(format!("{uid}@test.example"))
        .execute(pool).await.expect("insert auth_user");
}

async fn ensure_membership(pool: &PgPool, company_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, $3::text::membership_role, 'active') ON CONFLICT DO NOTHING"
    )
    .bind(company_id).bind(user_id).bind(role)
    .execute(pool).await.expect("insert membership");
}

async fn get(app: &axum::Router, uri: &str, actor: &AuthorizationActor) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn put(app: &axum::Router, uri: &str, actor: &AuthorizationActor, body_val: Value) -> (StatusCode, Value) {
    let mut req = Request::builder().method("PUT").uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body_val).unwrap())).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn inbox_agent_policy_get_self_returns_open_default() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200, "get self policy={body:?}");
    assert_eq!(body["mode"], "open", "default mode is open");
    assert_eq!(body["materialized"], Value::Bool(false), "not materialized");
    assert!(body["allowedAgentIds"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn inbox_agent_policy_update_and_read_self() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;

    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Policy Test Agent') ON CONFLICT DO NOTHING")
        .bind(agent_id).bind(cid)
        .execute(&pool).await.expect("insert agent");

    // Update to allowlist mode with one allowed agent
    let (status, body) = put(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor, json!({
        "mode": "allowlist",
        "allowedAgentIds": [agent_id]
    })).await;
    assert_eq!(status, 200, "update self policy={body:?}");
    assert_eq!(body["mode"], "allowlist");
    let allowed = body["allowedAgentIds"].as_array().unwrap();
    assert_eq!(allowed.len(), 1, "one allowed agent: {allowed:?}");

    // Read back
    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200);
    assert_eq!(body["mode"], "allowlist");
    assert_eq!(body["materialized"], Value::Bool(true));
    assert_eq!(body["allowedAgentIds"][0].as_str().unwrap(), agent_id.to_string());
}

#[tokio::test]
async fn inbox_agent_policy_admin_can_read_other_user() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = owner_actor(cid);
    let (target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "owner").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200, "admin read other={body:?}");
    assert_eq!(body["mode"], "open");
}

#[tokio::test]
async fn inbox_agent_policy_viewer_cannot_read_other_user() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (viewer, vid) = viewer_actor(cid);
    let (target, tid) = owner_actor(cid);
    ensure_auth_user(&pool, vid).await;
    ensure_membership(&pool, cid, vid, "viewer").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "owner").await;

    let (status, _body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &viewer).await;
    assert_eq!(status, 403, "viewer cannot read other user's policy");
}

#[tokio::test]
async fn inbox_agent_policy_agent_is_forbidden() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let agent = agent_actor(cid);

    let (status, _body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &agent).await;
    assert_eq!(status, 403, "agent cannot access");
}
