//! HTTP and PostgreSQL parity coverage for Issue creation deduplication.
//!
//! The route keeps the company scope authoritative, serializes idempotent
//! creates with a session advisory lock, and reports whether a 200 response
//! was caused by an idempotency replay or a recent open title.

use api::routes::issues::issue_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::AuthorizationActor;
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    board_user_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Issue creation parity")
        .bind(format!("IC{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");

    Fixture {
        pool: pool.clone(),
        company_id,
        board_user_id,
    }
}

fn board_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.board_user_id, fixture.company_id)
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/companies/{}/issues", actor.company_id().expect("company")))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).expect("serialize request")))
        .expect("build request");
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
    let value = serde_json::from_slice(&bytes).expect("response body must be JSON");
    (status, value)
}

fn issue_id(value: &Value) -> Uuid {
    value["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("issue id")
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

#[sqlx::test]
async fn create_reports_status_and_deduplication_reason(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (status, first) = send(&app, &actor, json!({"title": "Repeated title"})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(first.get("deduplicated").is_none());
    let first_id = issue_id(&first);

    let (status, title_replay) = send(&app, &actor, json!({"title": "  repeated   title "})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(issue_id(&title_replay), first_id);
    assert_eq!(title_replay["deduplicated"], true);
    assert_eq!(title_replay["deduplicationReason"], "recent_open_title");

    let (status, keyed) = send(
        &app,
        &actor,
        json!({"title": "Idempotent issue", "idempotencyKey": "create-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let keyed_id = issue_id(&keyed);

    let (status, keyed_replay) = send(
        &app,
        &actor,
        json!({"title": "A different title", "idempotencyKey": "create-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(issue_id(&keyed_replay), keyed_id);
    assert_eq!(keyed_replay["deduplicated"], true);
    assert_eq!(keyed_replay["deduplicationReason"], "idempotency_key");

    let stored_key: (Uuid, Uuid) = sqlx::query_as(
        "SELECT company_id, issue_id
         FROM issue_create_idempotency_keys
         WHERE company_id = $1 AND idempotency_key = 'create-1'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("idempotency row");
    assert_eq!(stored_key.0, fixture.company_id);
    assert_eq!(stored_key.1, keyed_id);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn concurrent_same_key_creates_one_issue_and_replays_it(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (left, right) = tokio::join!(
        send(
            &app,
            &actor,
            json!({"title": "Concurrent left", "idempotencyKey": "concurrent-1"}),
        ),
        send(
            &app,
            &actor,
            json!({"title": "Concurrent right", "idempotencyKey": "concurrent-1"}),
        ),
    );

    assert!(matches!(left.0, StatusCode::CREATED | StatusCode::OK));
    assert!(matches!(right.0, StatusCode::CREATED | StatusCode::OK));
    assert!(left.0 != right.0, "one request must create and one must replay");
    assert_eq!(issue_id(&left.1), issue_id(&right.1));
    let created = if left.0 == StatusCode::CREATED { &left.1 } else { &right.1 };
    let replayed = if left.0 == StatusCode::OK { &left.1 } else { &right.1 };
    assert!(created.get("deduplicated").is_none());
    assert_eq!(replayed["deduplicated"], true);
    assert_eq!(replayed["deduplicationReason"], "idempotency_key");

    let issue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues
         WHERE company_id = $1 AND title IN ('Concurrent left', 'Concurrent right')",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("issue count");
    assert_eq!(issue_count, 1);

    let key_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_create_idempotency_keys
         WHERE company_id = $1 AND idempotency_key = 'concurrent-1'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("key count");
    assert_eq!(key_count, 1);

    cleanup(&fixture).await;
}
