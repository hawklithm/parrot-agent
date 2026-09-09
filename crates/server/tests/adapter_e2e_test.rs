//! Adapter HTTP parity tests: list adapters, get info, list models, detect-model, config schema.
//!
//! Uses `#[sqlx::test]` for isolated DB per test case.

use api::routes::adapters::adapter_routes;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

async fn seed(pool: &PgPool) -> (Uuid, Uuid) {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("AdapterE2E")
        .bind(format!("AE{}", &company_id.simple().to_string()[..6]))
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

fn actor(user_id: Uuid, company_id: Uuid) -> services::auth::AuthorizationActor {
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
async fn list_global_adapters(pool: PgPool) {
    migrate(&pool).await;
    let (_company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, Uuid::nil());
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(&app, &a, "GET", "/adapters", None).await;
    assert_eq!(st, StatusCode::OK, "list global adapters: {:?}", body);
    let arr = body.as_array().expect("should be array");
    assert!(!arr.is_empty(), "should have at least built-in adapters");
    for item in arr {
        assert!(
            item.get("type").is_some() || item.get("adapterType").is_some(),
            "adapter entry missing type: {:?}",
            item
        );
    }
}

#[sqlx::test]
async fn list_company_adapters(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, company_id);
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(&app, &a, "GET", &format!("/companies/{}/adapters", company_id), None).await;
    assert_eq!(st, StatusCode::OK, "list company adapters: {:?}", body);
    let arr = body.get("adapters").and_then(|v| v.as_array()).or_else(|| body.as_array()).expect("should be array or object with adapters key");
    for item in arr {
        assert!(
            item.get("type").is_some() || item.get("adapterType").is_some() || item.get("adapter_type").is_some(),
            "missing type"
        );
    }
}

#[sqlx::test]
async fn get_adapter_info(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, company_id);
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "GET",
        &format!("/companies/{}/adapters/http", company_id),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "get http adapter info: {:?}", body);
    assert!(
        body.get("capabilities").is_some() || body.get("supportsSkills").is_some(),
        "should have capabilities"
    );
}

#[sqlx::test]
async fn list_models(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, company_id);
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "GET",
        &format!("/companies/{}/adapters/http/models", company_id),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "list models for http: {:?}", body);
    match body {
        Value::Array(_) | Value::Object(_) => {}
        _ => panic!("unexpected model response shape: {:?}", body),
    }
}

#[sqlx::test]
async fn detect_model_http(pool: PgPool) {
    migrate(&pool).await;
    let (company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, company_id);
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(
        &app,
        &a,
        "GET",
        &format!("/companies/{}/adapters/http/detect-model", company_id),
        None,
    )
    .await;
    assert!(
        st == StatusCode::OK || st == StatusCode::NOT_FOUND,
        "detect-model: status={}",
        st
    );
}

#[sqlx::test]
async fn get_config_schema(pool: PgPool) {
    migrate(&pool).await;
    let (_company_id, user_id) = seed(&pool).await;
    let a = actor(user_id, Uuid::nil());
    let app = Router::new()
        .merge(adapter_routes())
        .with_state(build_app_state(pool).await.expect("build state"));

    let (st, body) = send(&app, &a, "GET", "/adapters/http/config-schema", None).await;
    assert!(
        st == StatusCode::OK || st == StatusCode::NOT_FOUND,
        "config-schema: status={}, body={:?}",
        st,
        body
    );
}
