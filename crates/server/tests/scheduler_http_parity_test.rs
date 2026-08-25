//! Scheduler observability parity test (PAPERCLIP_MIGRATION_PLAN §4B.3 line 357).
//!
//! Verifies the runtime observability surface for background tasks:
//!   - GET /scheduler/jobs lists registered job metadata (name + schedule)
//!   - POST /scheduler/jobs/:name/trigger runs a manual trigger through the
//!     lease + running-guard + execution-history path
//!   - GET /scheduler/jobs/:name/executions surfaces recent persisted executions
//!   - GET /scheduler/leases surfaces persisted leases
//!   - routes return 404 when no scheduler is wired (embedded/test context)

use api::routes::scheduler_routes::scheduler_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::Value;
use sqlx::PgPool;
use tower::util::ServiceExt;

async fn send(app: &Router, method: &str, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(uri).body(Body::empty()).expect("request");
    let resp = app.clone().oneshot(req).await.expect("dispatch");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("body");
    let parsed = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
    (status, parsed)
}

#[tokio::test]
async fn scheduler_routes_expose_jobs_and_404_without_scheduler() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL is not set");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("connect");

    // Without a scheduler (None): every route returns 404.
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = scheduler_routes().with_state(state);
    let (status, _) = send(&app, "GET", "/scheduler/jobs").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no scheduler -> 404");
    let (status, _) = send(&app, "GET", "/scheduler/leases").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no scheduler -> 404 for leases");

    // With a scheduler: register a lightweight job and observe it.
    let scheduler = std::sync::Arc::new(services::JobScheduler::new().with_pool(pool.clone()));
    let job = std::sync::Arc::new(services::StuckRunDetector::new(pool.clone()));
    scheduler.register(job).await;

    let mut state = build_app_state(pool.clone()).await.expect("build_app_state");
    state.scheduler = Some(scheduler.clone());
    let app = scheduler_routes().with_state(state);

    let (status, jobs) = send(&app, "GET", "/scheduler/jobs").await;
    assert_eq!(status, StatusCode::OK, "jobs list: {jobs}");
    let names: Vec<&str> = jobs.as_array().map(|a| a.iter().filter_map(|j| j["jobName"].as_str()).collect()).unwrap_or_default();
    assert!(
        names.contains(&"stuck_run_detector"),
        "registered job visible in metadata, got {names:?}"
    );

    // Manual trigger routes through the execution-history path.
    let (status, triggered) = send(&app, "POST", "/scheduler/jobs/stuck_run_detector/trigger").await;
    assert_eq!(status, StatusCode::OK, "manual trigger: {triggered}");
    assert_eq!(triggered["jobName"], "stuck_run_detector");

    // Recent executions must include the triggered run.
    let (status, executions) = send(&app, "GET", "/scheduler/jobs/stuck_run_detector/executions").await;
    assert_eq!(status, StatusCode::OK, "executions: {executions}");
    assert!(
        executions["executions"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "triggered job must leave an execution record, got {executions}"
    );

    // Leases endpoint is reachable (may be empty or populated depending on timing).
    let (status, _) = send(&app, "GET", "/scheduler/leases").await;
    assert_eq!(status, StatusCode::OK, "leases endpoint reachable");

    // Unknown job trigger -> error surfaced, not silent success.
    let (status, _) = send(&app, "POST", "/scheduler/jobs/does_not_exist/trigger").await;
    assert!(status.is_server_error() || status == StatusCode::NOT_FOUND, "unknown job trigger must fail, got {status}");
}
