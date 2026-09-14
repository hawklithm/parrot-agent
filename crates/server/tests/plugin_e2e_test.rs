//! Plugin E2E: install / get / enable / disable / list / remove.
//!
//! Uses `#[sqlx::test]` for isolated DB per test case.

use api::routes::plugins::plugin_routes;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;


mod common;
use common::migrate;


async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("PluginE2E")
        .bind(format!("PE{}", &company_id.simple().to_string()[..6]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("e2e{}@test.com", Uuid::new_v4()))
    .bind("E2E User")
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

fn admin_actor(user_id: Uuid, company_id: Uuid) -> services::auth::AuthorizationActor {
    services::auth::AuthorizationActor::board_with_memberships(
        user_id,
        company_id,
        vec![services::auth::CompanyMembership::new(
            company_id,
            services::auth::PrincipalType::User,
            user_id,
            services::auth::MembershipRole::Operator,
        )],
        true,  // is_instance_admin = true
    )
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
async fn install_and_get_plugin(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = admin_actor(user_id, company_id);
    let app = Router::new()
        .merge(plugin_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "POST",
        "/plugins/install",
        Some(json!({
            "pluginKey": "test/plugin-alpha",
            "name": "Test Plugin Alpha",
            "version": "1.0.0",
            "packageName": "test-plugin-alpha",
            "manifest": { "capabilities": ["dashboardWidget"] }
        })),
    )
    .await;
    eprintln!("INSTALL ST={:?} BODY={}", st, body);
    assert_eq!(st, StatusCode::CREATED, "install should return 201: {:?}", body);
    let pid = body["id"].as_str().expect("plugin id");
    assert_eq!(body["status"].as_str().unwrap(), "ready");
    assert_eq!(body["pluginKey"].as_str().unwrap(), "test/plugin-alpha");

    let (st, body) = send(&app, &a, "GET", &format!("/plugins/{}", pid), None).await;
    assert_eq!(st, StatusCode::OK, "get should return 200: {:?}", body);
    assert_eq!(body["id"].as_str().unwrap(), pid);
    assert_eq!(body["name"].as_str().unwrap(), "Test Plugin Alpha");

    let (st, body) = send(&app, &a, "GET", "/plugins", None).await;
    assert_eq!(st, StatusCode::OK);
    let arr = body.as_array().expect("list should be array");
    assert!(arr.iter().any(|p| p["id"].as_str() == Some(pid)), "plugin not in list");
}

#[sqlx::test]
async fn enable_disable_cycle(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = admin_actor(user_id, company_id);
    let app = Router::new()
        .merge(plugin_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "POST",
        "/plugins/install",
        Some(json!({
            "pluginKey": "test/plugin-beta",
            "name": "Beta",
            "version": "2.0.0"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "install beta: {:?}", body);
    let pid = body["id"].as_str().unwrap();

    let (st, body) = send(&app, &a, "POST", &format!("/plugins/{}/disable", pid), None).await;
    assert_eq!(st, StatusCode::OK, "disable: {:?}", body);
    assert_eq!(body["status"].as_str().unwrap(), "disabled");

    let (st, body) = send(&app, &a, "POST", &format!("/plugins/{}/enable", pid), None).await;
    assert_eq!(st, StatusCode::OK, "enable: {:?}", body);
    assert_eq!(body["status"].as_str().unwrap(), "ready");

    // Status filter not supported, just verify install + enable works
}

#[sqlx::test]
async fn remove_plugin(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = admin_actor(user_id, company_id);
    let app = Router::new()
        .merge(plugin_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "POST",
        "/plugins/install",
        Some(json!({
            "pluginKey": "test/plugin-gamma",
            "name": "Gamma",
            "version": "3.0.0"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let pid = body["id"].as_str().unwrap();

    let (st, _) = send(&app, &a, "DELETE", &format!("/plugins/{}", pid), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT, "remove should return 204");

}

#[sqlx::test]
async fn install_duplicate_uses_upsert(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = admin_actor(user_id, company_id);
    let app = Router::new()
        .merge(plugin_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let payload = json!({"pluginKey": "test/plugin-dup", "name": "Dup", "version": "1.0.0"});
    let (st1, body1) = send(&app, &a, "POST", "/plugins/install", Some(payload.clone())).await;
    assert_eq!(st1, StatusCode::CREATED);
    let pid = body1["id"].as_str().unwrap();

    let (st2, body2) = send(
        &app,
        &a,
        "POST",
        "/plugins/install",
        Some(json!({"pluginKey": "test/plugin-dup", "name": "Dup v2", "version": "2.0.0"})),
    )
    .await;
    assert_eq!(st2, StatusCode::CREATED);
    assert_eq!(body2["id"].as_str().unwrap(), pid);
    assert_eq!(body2["version"].as_str().unwrap(), "2.0.0");
}
