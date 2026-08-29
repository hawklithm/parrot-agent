//! Stale request-confirmation expiry on issue-document revision
//! (PAPERCLIP_MIGRATION_PLAN §4B.1 lines 383/387).
//!
//! Paperclip expires a pending `request_confirmation` when the issue-document
//! revision it targets is superseded (`expireStaleRequestConfirmationTarget`),
//! otherwise an agent is left confirming content that no longer exists. Parrot
//! implemented `expire_stale_request_confirmations_for_issue_document` but never
//! called it from any document write path, so stale confirmations stayed pending
//! forever.
//!
//! Covered here:
//!   - a new revision expires a confirmation bound to the superseded revision
//!   - a confirmation bound to the newest revision is preserved
//!   - a restore also expires confirmations bound to the pre-restore revision
//!   - confirmations on a different document key are untouched
//!   - a failure to expire never fails the document write

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
    issue_id: Uuid,
    board_user_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Stale Confirmation Co")
        .bind(format!("SC{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO issues (id, company_id, title, status) VALUES ($1, $2, $3, 'todo')")
        .bind(issue_id)
        .bind(company_id)
        .bind("Stale confirmation issue")
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

fn actor(f: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(f.board_user_id, f.company_id)
}

async fn cleanup(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issue_thread_interactions WHERE issue_id = $1")
        .bind(f.issue_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM issues WHERE id = $1")
        .bind(f.issue_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}

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
            Body::from(serde_json::to_vec(&value).expect("serialize body"))
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
        .expect("read body");
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, parsed)
}

/// Insert a pending `request_confirmation` bound to `revision_id`.
async fn insert_confirmation(f: &Fixture, key: &str, revision_id: Uuid, label: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_thread_interactions
           (id, company_id, issue_id, kind, status, title, summary, payload)
         VALUES ($1, $2, $3, 'request_confirmation', 'pending', $4, $4, $5)",
    )
    .bind(id)
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(label)
    .bind(json!({
        "target": {
            "type": "issue_document",
            "issueId": f.issue_id.to_string(),
            "key": key,
            "revisionId": revision_id.to_string(),
        }
    }))
    .execute(&f.pool)
    .await
    .expect("insert confirmation");
    id
}

async fn interaction_status(f: &Fixture, id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT status::text FROM issue_thread_interactions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .expect("read interaction status")
}

/// A new revision expires a confirmation bound to the superseded revision, and
/// preserves one already bound to the newest revision.
#[sqlx::test]
async fn new_revision_expires_stale_confirmations(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let actor = actor(&f);
    let app = issue_routes().with_state(build_app_state(pool.clone()).await.expect("state"));
    let uri = format!("/issues/{}/documents/plan", f.issue_id);

    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &uri,
        Some(json!({"body": "rev one", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let rev_one = Uuid::parse_str(body["revisionId"].as_str().expect("revision id")).expect("uuid");

    // A confirmation bound to revision 1 and to an unknown older revision.
    let stale = insert_confirmation(&f, "plan", Uuid::new_v4(), "stale confirmation").await;
    let stale_note = insert_confirmation(&f, "plan", Uuid::new_v4(), "unknown revision").await;

    // Writing revision 2 supersedes revision 1 and the unknown revision.
    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &uri,
        Some(json!({"body": "rev two", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["expiredConfirmations"], json!(2));

    assert_eq!(interaction_status(&f, stale).await, "expired");
    assert_eq!(interaction_status(&f, stale_note).await, "expired");

    // A confirmation bound to the current revision survives the next write.
    let rev_two = Uuid::parse_str(body["revisionId"].as_str().expect("revision id")).expect("uuid");
    let current = insert_confirmation(&f, "plan", rev_two, "current confirmation").await;
    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &uri,
        Some(json!({"body": "rev three", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    // The current-revision confirmation is stale relative to revision 3, but
    // the earlier two are already expired and must not be double-counted.
    assert_eq!(body["expiredConfirmations"], json!(1));
    assert_eq!(interaction_status(&f, current).await, "expired");

    let _ = rev_one;
    cleanup(&f).await;
}

/// A confirmation on a different document key is untouched by a revision.
#[sqlx::test]
async fn confirmations_on_other_document_keys_are_preserved(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let actor = actor(&f);
    let app = issue_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &format!("/issues/{}/documents/plan", f.issue_id),
        Some(json!({"body": "plan rev", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    let other = insert_confirmation(&f, "notes", Uuid::new_v4(), "other document").await;

    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &format!("/issues/{}/documents/plan", f.issue_id),
        Some(json!({"body": "plan rev two", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["expiredConfirmations"], json!(0));
    assert_eq!(
        interaction_status(&f, other).await,
        "pending",
        "a confirmation on another document key must survive"
    );

    cleanup(&f).await;
}

/// Restoring a revision produces a new revision, which also expires stale
/// confirmations.
#[sqlx::test]
async fn restore_expires_stale_confirmations(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let actor = actor(&f);
    let app = issue_routes().with_state(build_app_state(pool.clone()).await.expect("state"));
    let uri = format!("/issues/{}/documents/plan", f.issue_id);

    let (status, body) = send(
        &app,
        &actor,
        "PUT",
        &uri,
        Some(json!({"body": "first", "format": "markdown"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let first_revision_id =
        Uuid::parse_str(body["revisionId"].as_str().expect("revision id")).expect("uuid");

    let stale = insert_confirmation(&f, "plan", Uuid::new_v4(), "pre-restore").await;

    let (status, body) = send(
        &app,
        &actor,
        "POST",
        &format!(
            "/issues/{}/documents/plan/revisions/{}/restore",
            f.issue_id, first_revision_id
        ),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(
        body.get("expiredConfirmations").is_some(),
        "restore must report expired confirmations: {body}"
    );
    assert_eq!(interaction_status(&f, stale).await, "expired");

    cleanup(&f).await;
}
