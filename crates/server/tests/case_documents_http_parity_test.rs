//! HTTP and PostgreSQL parity coverage for the legacy Case document surface.
//!
//! Case documents share the canonical documents/revisions/annotation tables
//! with Issue documents. These tests protect the Case-specific transaction,
//! authorization, optimistic concurrency, locking, and actor-attribution
//! contract while the remaining Case routes are migrated.

use api::routes::cases::case_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor};
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
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    case_id: Uuid,
    board_user_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let case_id = Uuid::new_v4();
    let board_user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Case Documents Parity")
        .bind(format!("CD{}", &company_id.simple().to_string()[..8]))
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO cases
         (id, company_id, case_number, identifier, case_type, key, title, status)
         VALUES ($1, $2, 1, $3, 'pipeline', 'parity-case', 'Case document parity', 'draft')",
    )
    .bind(case_id)
    .bind(company_id)
    .bind(format!("CD-{}", &case_id.simple().to_string()[..8]))
    .execute(pool)
    .await
    .expect("insert case");
    Fixture {
        pool: pool.clone(),
        company_id,
        case_id,
        board_user_id,
    }
}

fn board_actor(fixture: &Fixture, user_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, fixture.company_id)
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM cases WHERE id=$1")
        .bind(fixture.case_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id=$1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

fn uuid_field(value: &Value, field: &str) -> Uuid {
    value
        .get(field)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("missing UUID field {field}: {value}"))
}

#[sqlx::test]
async fn case_document_crud_concurrency_lock_and_annotations_are_transactional(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture, fixture.board_user_id);
    let app = case_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));
    let document_uri = format!("/cases/{}/documents/plan", fixture.case_id);

    let (left, right) = tokio::join!(
        send(
            &app,
            &actor,
            "POST",
            &document_uri,
            Some(json!({"body": "case document body"})),
        ),
        send(
            &app,
            &actor,
            "POST",
            &document_uri,
            Some(json!({"body": "case document body"})),
        ),
    );
    let created = match (left.0, right.0) {
        (StatusCode::CREATED, StatusCode::CONFLICT) => left.1,
        (StatusCode::CONFLICT, StatusCode::CREATED) => right.1,
        result => panic!("concurrent create statuses: {result:?}"),
    };
    assert_eq!(created["format"], "markdown");
    let document_id = uuid_field(&created, "id");

    let (status, revisions) = send(
        &app,
        &actor,
        "GET",
        &format!("{document_uri}/revisions"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revisions.as_array().map(Vec::len), Some(1));
    let first_revision_id = uuid_field(&revisions[0], "id");

    let (left, right) = tokio::join!(
        send(
            &app,
            &actor,
            "PUT",
            &document_uri,
            Some(json!({"body": "left update", "baseRevisionId": first_revision_id})),
        ),
        send(
            &app,
            &actor,
            "PUT",
            &document_uri,
            Some(json!({"body": "right update", "baseRevisionId": first_revision_id})),
        ),
    );
    let updated = match (left.0, right.0) {
        (StatusCode::OK, StatusCode::CONFLICT) => left.1,
        (StatusCode::CONFLICT, StatusCode::OK) => right.1,
        result => panic!("concurrent update statuses: {result:?}"),
    };
    let current_revision_id = uuid_field(&updated, "revisionId");
    assert_eq!(updated["revisionNumber"], 2);

    let second_user = Uuid::new_v4();
    let second_actor = board_actor(&fixture, second_user);
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
        &second_actor,
        "POST",
        &format!("{document_uri}/unlock"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &app,
        &actor,
        "PUT",
        &document_uri,
        Some(json!({"body": "blocked while locked", "baseRevisionId": current_revision_id})),
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

    let (status, annotation) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/annotations"),
        Some(json!({
            "selectedText": "update",
            "anchorSelector": {"quote": {"exact": "update"}},
            "baseRevisionId": current_revision_id,
            "baseRevisionNumber": 2,
            "body": "Please review this case document",
        })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create annotation: {annotation}"
    );
    let thread_id = uuid_field(&annotation, "threadId");
    assert_eq!(annotation["comments"][0]["authorType"], "user");

    let (status, annotations) = send(
        &app,
        &actor,
        "GET",
        &format!("{document_uri}/annotations?status=open"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(annotations.as_array().map(Vec::len), Some(1));
    assert_eq!(annotations[0]["comments"].as_array().map(Vec::len), Some(1));

    let (status, reply) = send(
        &app,
        &actor,
        "POST",
        &format!("{document_uri}/annotations/{thread_id}/reply"),
        Some(json!({"body": "Reply from the board"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(reply["authorUserId"], fixture.board_user_id.to_string());

    let (status, resolved) = send(
        &app,
        &actor,
        "PATCH",
        &format!("{document_uri}/annotations/{thread_id}"),
        Some(json!({"status": "resolved"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resolved["status"], "resolved");

    let (status, thread) = send(
        &app,
        &actor,
        "GET",
        &format!("{document_uri}/annotations/{thread_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(thread["status"], "resolved");
    assert_eq!(thread["comments"].as_array().map(Vec::len), Some(2));

    let stored: (String, String, i64) = sqlx::query_as(
        "SELECT c.author_type, c.author_user_id, COUNT(*)
         FROM document_annotation_comments c
         WHERE c.thread_id=$1
         GROUP BY c.author_type, c.author_user_id",
    )
    .bind(thread_id)
    .fetch_one(&pool)
    .await
    .expect("annotation attribution");
    assert_eq!(stored.0, "user");
    assert_eq!(stored.1, fixture.board_user_id.to_string());
    assert_eq!(stored.2, 2);

    let document_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM case_documents WHERE case_id=$1 AND key='plan' AND document_id=$2",
    )
    .bind(fixture.case_id)
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("logical document count");
    let revision_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_revisions WHERE company_id=$1 AND document_id=$2",
    )
    .bind(fixture.company_id)
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("revision count");
    assert_eq!(document_count, 1);
    assert_eq!(revision_count, 2);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn case_document_routes_enforce_company_scope_and_key_validation(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture, fixture.board_user_id);
    let foreign_actor = AuthorizationActor::board_with_source(
        Uuid::new_v4(),
        Uuid::new_v4(),
        ActorSource::Session,
        Vec::new(),
        false,
    );
    let app = case_routes().with_state(build_app_state(pool.clone()).await.expect("app state"));
    let uri = format!("/cases/{}/documents/plan", fixture.case_id);

    let (status, _) = send(
        &app,
        &foreign_actor,
        "POST",
        &uri,
        Some(json!({"body": "must be forbidden"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/cases/{}/documents/bad$key", fixture.case_id),
        Some(json!({"body": "bad key"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    cleanup(&fixture).await;
}
