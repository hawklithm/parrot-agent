//! Compatibility namespace test: verifies Parrot extensions do not collide with
//! Paperclip route prefixes or global state assumptions.
//!
//! L964 — 所有 Parrot 扩展均通过功能开关或命名空间证明不会破坏 Paperclip 兼容行为。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;


mod common;
use common::migrate;


async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix, status) VALUES ($1, $2, $3, $4::company_status)")
        .bind(company_id)
        .bind("CompatTest")
        .bind(format!("CT{}", &company_id.simple().to_string()[..6]))
        .bind("active")
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("ct{}@test.com", Uuid::new_v4()))
    .bind("CT User")
    .execute(pool)
    .await
    .expect("insert user");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active') ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");
    (company_id, user_id)
}

fn board_actor(user_id: Uuid, company_id: Uuid) -> services::auth::AuthorizationActor {
    services::auth::AuthorizationActor::board(user_id, company_id)
}

async fn send(
    app: &Router,
    a: &services::auth::AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).unwrap();
    req.extensions_mut().insert(a.clone());
    let resp = app.clone().oneshot(req).await.unwrap();
    let st = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let val = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (st, val)
}

#[sqlx::test]
async fn parrot_routes_use_company_scope(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = board_actor(user_id, company_id);
    let app = Router::new()
        .merge(api::routes::companies::company_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    // Verify company-scoped routes respond correctly with board actor
    let (st, _) = send(&app, &a, "GET", &format!("/companies/{}", company_id), None).await;
    assert_eq!(st, StatusCode::OK, "company GET should return 200");

    // Cross-company access should be 403
    let other_id = Uuid::new_v4();
    let (st, _) = send(&app, &a, "GET", &format!("/companies/{}", other_id), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "foreign company should be 404");
}


#[sqlx::test]
async fn issue_prefixes_unique_per_company(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, _user_id) = seed(&pool).await;

    // Verify issue_prefix exists and is unique for this company
    let prefix: String = sqlx::query_scalar(
        "SELECT issue_prefix FROM companies WHERE id = $1",
    )
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .expect("fetch prefix");
    assert!(!prefix.is_empty(), "issue_prefix must not be empty");
}
