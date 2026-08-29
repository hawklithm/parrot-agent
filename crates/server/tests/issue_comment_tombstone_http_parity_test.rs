//! HTTP/PostgreSQL parity coverage for issue-comment tombstones.

use api::routes::issue_comments::issue_comment_routes;
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

async fn seed_company(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id)
        .bind("Issue comment tombstone parity")
        .bind(format!("CT{}", &id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    id
}

async fn seed_agent(pool: &PgPool, company_id: Uuid, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type) \
         VALUES ($1, $2, $3, 'running', 'process')",
    )
    .bind(id)
    .bind(company_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert agent");
    id
}

async fn seed_issue(pool: &PgPool, company_id: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, created_at, updated_at) \
         VALUES ($1, $2, 'Tombstone issue', 'todo', NOW(), NOW())",
    )
    .bind(id)
    .bind(company_id)
    .execute(pool)
    .await
    .expect("insert issue");
    id
}

async fn app(pool: PgPool) -> Router {
    issue_comment_routes().with_state(build_app_state(pool).await.expect("app state"))
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(body) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&body).expect("serialize body"))
        }
        None => Body::empty(),
    };
    let mut request = builder.body(body).expect("build request");
    request.extensions_mut().insert(actor.clone());
    let response = app.clone().oneshot(request).await.expect("dispatch request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    (status, value)
}

#[sqlx::test]
async fn deleting_comment_writes_redacted_tombstone_and_is_idempotent(pool: PgPool) {
    migrate(&pool).await;
    let company_id = seed_company(&pool).await;
    let author_id = seed_agent(&pool, company_id, "comment author").await;
    let issue_id = seed_issue(&pool, company_id).await;
    let actor = AuthorizationActor::agent(author_id, company_id, None);
    let app = app(pool.clone()).await;

    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{issue_id}/comments"),
        Some(json!({
            "body": "secret comment [@Nobody](agent://00000000-0000-0000-0000-000000000001)",
            "actor_type": "agent",
            "actor_id": author_id,
            "metadata": {"private": true}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create={created:?}");
    let comment_id = created["comment"]["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("created comment id");

    let (status, body) = send(
        &app,
        &actor,
        "DELETE",
        &format!("/comments/{comment_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete={body:?}");

    let row: (String, Option<chrono::DateTime<chrono::Utc>>, String, Option<Uuid>, Option<String>) =
        sqlx::query_as(
            "SELECT body, deleted_at, deleted_by_type, deleted_by_agent_id, deleted_by_user_id \
             FROM issue_comments WHERE id = $1",
        )
        .bind(comment_id)
        .fetch_one(&pool)
        .await
        .expect("tombstone row");
    assert_eq!(row.0, "");
    assert!(row.1.is_some(), "deleted_at should be set");
    assert_eq!(row.2, "agent");
    assert_eq!(row.3, Some(author_id));
    assert_eq!(row.4, None);

    let (status, fetched) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{issue_id}/comments/{comment_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get={fetched:?}");
    assert_eq!(fetched["comment"]["body"], "");
    assert!(fetched["comment"]["deletedAt"].is_string());
    assert!(fetched["comment"]["metadata"].is_null());

    let (status, body) = send(
        &app,
        &actor,
        "DELETE",
        &format!("/comments/{comment_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "repeat delete={body:?}");
}

#[sqlx::test]
async fn only_the_authenticated_comment_author_can_tombstone(pool: PgPool) {
    migrate(&pool).await;
    let company_id = seed_company(&pool).await;
    let author_id = seed_agent(&pool, company_id, "comment author").await;
    let other_id = seed_agent(&pool, company_id, "other agent").await;
    let issue_id = seed_issue(&pool, company_id).await;
    let author = AuthorizationActor::agent(author_id, company_id, None);
    let other = AuthorizationActor::agent(other_id, company_id, None);
    let app = app(pool.clone()).await;

    let (status, created) = send(
        &app,
        &author,
        "POST",
        &format!("/issues/{issue_id}/comments"),
        Some(json!({
            "body": "author-only comment",
            "actor_type": "agent",
            "actor_id": author_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create={created:?}");
    let comment_id = Uuid::parse_str(created["comment"]["id"].as_str().expect("comment id"))
        .expect("comment uuid");

    let (status, body) = send(
        &app,
        &other,
        "DELETE",
        &format!("/comments/{comment_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "delete={body:?}");

    let deleted_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deleted_at FROM issue_comments WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_one(&pool)
    .await
    .expect("comment row");
    assert!(deleted_at.is_none(), "unauthorized delete must not tombstone");
}

