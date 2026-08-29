//! HTTP and PostgreSQL parity coverage for issue documents.
//!
//! The regressions here target the canonical issue_documents /
//! document_revisions path: logical-key first-create serialization,
//! restore revision allocation, optimistic annotation anchors, and actor
//! attribution for annotation comments.
//!
//! The migration runner is invoked explicitly because this suite exercises
//! schema additions that must be present in an already-created test database.

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

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request_body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut request = builder.body(request_body).expect("build request");
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
    (status, value)
}

async fn migrate(pool: &PgPool) {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::ERROR)
        .try_init();
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    issue_id: Uuid,
    board_user_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Issue Documents Parity")
        .bind(format!("ID{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status) VALUES ($1, $2, $3, 'todo')",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind("Document parity issue")
    .execute(pool)
    .await
    .expect("insert issue");
    Fixture {
        pool: pool.clone(),
        company_id,
        issue_id,
        board_user_id,
    }
}

fn board_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.board_user_id, fixture.company_id)
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

fn revision_id(revision: &Value) -> Uuid {
    revision
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("revision id")
}

#[sqlx::test]
async fn document_restore_and_annotation_paths_are_transactional(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app =
        issue_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));
    let document_uri = format!("/issues/{}/documents/plan", fixture.issue_id);

    let (status, _) = send(
        &app,
        &actor,
        "PUT",
        &document_uri,
        Some(json!({"body": "one", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revisions) = send(
        &app,
        &actor,
        "GET",
        &format!("{document_uri}/revisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revisions = revisions.as_array().expect("revision list");
    assert_eq!(revisions.len(), 1);
    let first_revision_id = revision_id(&revisions[0]);

    let (status, _) = send(
        &app,
        &actor,
        "PUT",
        &document_uri,
        Some(json!({
            "body": "two",
            "format": "markdown",
            "baseRevisionId": first_revision_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, revisions) = send(
        &app,
        &actor,
        "GET",
        &format!("{document_uri}/revisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revisions = revisions.as_array().expect("revision list");
    assert_eq!(revisions.len(), 2);
    let current_revision_id = revision_id(&revisions[0]);
    let current_revision_number = revisions[0]
        .get("revisionNumber")
        .and_then(Value::as_i64)
        .expect("current revision number");

    let agent = AuthorizationActor::agent(Uuid::new_v4(), fixture.company_id, None);
    let (status, _) = send(
        &app,
        &agent,
        "POST",
        &format!("{document_uri}/lock"),
        Some(json!({"actorType": "agent", "actorId": Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, locked) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/lock"),
        Some(json!({"actorType": "agent", "actorId": Uuid::new_v4()})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(locked["lockedByType"], "user");
    assert_eq!(locked["lockedById"], fixture.board_user_id.to_string());

    let (status, _) = send(
        &app,
        &actor,
        "PUT",
        &document_uri,
        Some(json!({"body": "blocked while locked"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/revisions/{first_revision_id}/restore"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/unlock"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/unlock"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, annotation) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/annotations"),
        Some(json!({
            "selectedText": "two",
            "anchorSelector": {"quote": {"exact": "two"}},
            "baseRevisionId": current_revision_id,
            "baseRevisionNumber": current_revision_number,
            "body": "Please review this text",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let thread_id = annotation
        .get("threadId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .expect("annotation thread id");
    assert_eq!(annotation["comments"][0]["authorType"], "user");

    let (status, annotations) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/documents/PLAN/annotations?status=all", fixture.issue_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(annotations.as_array().expect("annotations").len(), 1);

    let restore_uri = format!("{document_uri}/revisions/{first_revision_id}/restore");
    let (left, right) = tokio::join!(
        send(&app, &actor, "POST", &restore_uri, Some(json!({}))),
        send(&app, &actor, "POST", &restore_uri, Some(json!({}))),
    );
    assert_eq!(left.0, StatusCode::OK);
    assert_eq!(right.0, StatusCode::OK);

    let numbers: Vec<i32> = sqlx::query_scalar(
        "SELECT revision_number FROM document_revisions
         WHERE document_id = (SELECT document_id FROM issue_documents
                              WHERE issue_id = $1 AND key = 'plan')
         ORDER BY revision_number",
    )
    .bind(fixture.issue_id)
    .fetch_all(&pool)
    .await
    .expect("revision numbers");
    assert_eq!(numbers, vec![1, 2, 3, 4]);

    let annotation_comment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_annotation_comments WHERE thread_id = $1",
    )
    .bind(thread_id)
    .fetch_one(&pool)
    .await
    .expect("annotation comment count");
    assert_eq!(annotation_comment_count, 1);
    let attribution: (String, String) = sqlx::query_as(
        "SELECT author_type, author_user_id
         FROM document_annotation_comments WHERE thread_id = $1",
    )
    .bind(thread_id)
    .fetch_one(&pool)
    .await
    .expect("annotation attribution");
    assert_eq!(attribution.0, "user");
    assert_eq!(attribution.1, fixture.board_user_id.to_string());

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn concurrent_first_document_puts_create_one_logical_document(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app =
        issue_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));
    let uri = format!("/issues/{}/documents/plan", fixture.issue_id);

    let (left, right) = tokio::join!(
        send(&app, &actor, "PUT", &uri, Some(json!({"body": "left"}))),
        send(&app, &actor, "PUT", &uri, Some(json!({"body": "right"}))),
    );
    assert_eq!(left.0, StatusCode::OK);
    assert_eq!(right.0, StatusCode::OK);

    let document_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_documents WHERE issue_id = $1 AND key = 'plan'",
    )
    .bind(fixture.issue_id)
    .fetch_one(&pool)
    .await
    .expect("document count");
    let revision_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_revisions
         WHERE document_id = (SELECT document_id FROM issue_documents
                              WHERE issue_id = $1 AND key = 'plan')",
    )
    .bind(fixture.issue_id)
    .fetch_one(&pool)
    .await
    .expect("revision count");
    assert_eq!(document_count, 1);
    assert_eq!(revision_count, 2);

    cleanup(&fixture).await;
}
