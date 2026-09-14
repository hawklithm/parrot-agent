//! HTTP parity test for the deployment health endpoint.
//!
//! Paperclip `tests/e2e/multi-user.spec.ts:509-518` asserts that
//! `GET /api/health` answers 200 with `deploymentMode` and
//! `authReady === true`; `server/src/routes/health.ts:129-282` additionally
//! reports `bootstrapStatus` / `bootstrapInviteActive` and flips to 503
//! `unhealthy` when the database probe fails.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test health_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;

use api::routes::health::{health_check, health_response, DeploymentInfo};
use parrot_server::build_app_state;
use uuid::Uuid;


mod common;
use common::connect_and_migrate;


async fn get_health(app: &Router) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .expect("build request");
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (
        status,
        serde_json::from_slice(&bytes).expect("health body must be JSON"),
    )
}

fn health_router(state: api::FullAppState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .with_state(state)
}

fn body_of((_status, body): (StatusCode, axum::Json<api::routes::health::HealthResponse>)) -> Value {
    serde_json::to_value(body.0).expect("serialize health response")
}

/// Paperclip's E2E contract: 200 + `deploymentMode` + `authReady === true`.
#[tokio::test]
async fn health_reports_deployment_mode_and_auth_ready() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");

    let (status, body) = get_health(&health_router(state)).await;
    assert_eq!(status, StatusCode::OK, "health should answer 200: {body}");

    assert_eq!(body["status"], "ok");
    assert!(
        body.get("deploymentMode").is_some(),
        "deploymentMode is required by Paperclip's E2E assertion: {body}"
    );
    assert_eq!(
        body["authReady"], true,
        "authReady must be true once the server is serving: {body}"
    );
    assert!(
        body["bootstrapStatus"].is_string(),
        "bootstrapStatus must always be present: {body}"
    );
    assert!(
        body["bootstrapInviteActive"].is_boolean(),
        "bootstrapInviteActive must always be present: {body}"
    );
    // Responses must use camelCase, never the legacy snake_case keys.
    assert!(
        body.get("deployment_mode").is_none(),
        "snake_case deployment_mode must not appear: {body}"
    );
}

/// `local_trusted` has no first-admin concept, so bootstrap is always ready and
/// no bootstrap invite can be pending (Paperclip `health.ts` short-circuits the
/// role-count gate for non-authenticated deployments).
#[tokio::test]
async fn local_trusted_reports_bootstrap_ready() {
    let pool = connect_and_migrate().await;
    let (status, body) = health_response(
        &pool,
        DeploymentInfo {
            mode: Some("local_trusted".to_string()),
            exposure: "local".to_string(),
            commit: None,
        },
    )
    .await;
    let body = body_of((status, body));

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deploymentMode"], "local_trusted");
    assert_eq!(body["bootstrapStatus"], "ready");
    assert_eq!(body["bootstrapInviteActive"], false);
    assert_eq!(body["authReady"], true);
}

/// `cloud_managed` instances take identity from the control plane, so the
/// first-admin gate never applies (Paperclip `isCloudManagedInstance` short-circuit).
#[tokio::test]
async fn cloud_managed_reports_bootstrap_ready() {
    let pool = connect_and_migrate().await;
    let (status, body) = health_response(
        &pool,
        DeploymentInfo {
            mode: Some("cloud_managed".to_string()),
            exposure: "public".to_string(),
            commit: Some("deadbeef".to_string()),
        },
    )
    .await;
    let body = body_of((status, body));

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bootstrapStatus"], "ready");
    assert_eq!(body["bootstrapInviteActive"], false);
    assert_eq!(body["commit"], "deadbeef");
}

/// A database that cannot be probed must yield 503 `unhealthy`, not a 200 lie.
#[tokio::test]
async fn unreachable_database_reports_unhealthy() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool).await.expect("build app state");
    // Drop the pool out from under the handler so `SELECT 1` fails.
    state.pool.close().await;

    let (status, body) = get_health(&health_router(state)).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "closed pool must report 503: {body}"
    );
    assert_eq!(body["status"], "unhealthy");
    assert_eq!(body["authReady"], false);
}

/// In `authenticated` mode with no `instance_admin` row, the instance is pending
/// bootstrap; an active `bootstrap_ceo` invite must be surfaced. Asserts the
/// pure resolver so the shared admin row is never mutated.
#[tokio::test]
async fn authenticated_without_admin_reports_bootstrap_pending() {
    let pool = connect_and_migrate().await;

    // Skip (do not fake a pass) when a real admin exists: the gate is only
    // observable without one, and wiping it would corrupt the shared instance.
    let admin_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM instance_user_roles WHERE role = 'instance_admin'")
            .fetch_one(&pool)
            .await
            .expect("count instance admins");
    if admin_count > 0 {
        eprintln!(
            "skipping: instance already has {admin_count} admin(s); \
             bootstrap gate not observable without mutating shared state"
        );
        return;
    }

    let inviter_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let invite_id = Uuid::new_v4();
    let slug = Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(inviter_id)
        .bind(format!("health-{}@example.com", &slug[..8]))
        .bind("Health Inviter")
        .execute(&pool)
        .await
        .expect("insert inviter");
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Health Fixture Co")
        .bind("HFC")
        .execute(&pool)
        .await
        .expect("insert fixture company");
    sqlx::query(
        "INSERT INTO invites (id, company_id, invite_type, invited_by_user_id, token, expires_at,
                              allowed_join_types)
         VALUES ($1, $2, 'bootstrap_ceo', $3, $4, NOW() + INTERVAL '1 day', 'both')",
    )
    .bind(invite_id)
    .bind(company_id)
    .bind(inviter_id)
    .bind(slug)
    .execute(&pool)
    .await
    .expect("insert bootstrap invite");

    let (status, body) = health_response(
        &pool,
        DeploymentInfo {
            mode: Some("authenticated".to_string()),
            exposure: "public".to_string(),
            commit: None,
        },
    )
    .await;
    let body = body_of((status, body));

    sqlx::query("DELETE FROM invites WHERE id = $1")
        .bind(invite_id)
        .execute(&pool)
        .await
        .expect("cleanup invite");
    sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(company_id)
        .execute(&pool)
        .await
        .expect("cleanup company");
    sqlx::query("DELETE FROM auth_users WHERE id = $1")
        .bind(inviter_id)
        .execute(&pool)
        .await
        .expect("cleanup inviter");

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deploymentMode"], "authenticated");
    assert_eq!(
        body["bootstrapStatus"], "bootstrap_pending",
        "instance without an admin must report bootstrap_pending: {body}"
    );
    assert_eq!(
        body["bootstrapInviteActive"], true,
        "active bootstrap_ceo invite must be reported: {body}"
    );
}
