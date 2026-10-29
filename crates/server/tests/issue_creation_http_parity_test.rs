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


mod common;
use common::migrate;


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

async fn seed_agent(pool: &PgPool, company_id: Uuid, name: &str, role: &str, status: &str) -> Uuid {
    let agent_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, status, adapter_type)
         VALUES ($1, $2, $3, $4, $5, 'process')",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind(name)
    .bind(role)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert agent");
    agent_id
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
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

#[sqlx::test]
async fn expired_idempotency_key_is_cleaned_before_reuse(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (status, first) = send(
        &app,
        &actor,
        json!({"title": "Expired idempotency issue", "idempotencyKey": "expired-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let first_id = issue_id(&first);

    sqlx::query(
        "UPDATE issue_create_idempotency_keys
         SET created_at = NOW() - INTERVAL '8 days'
         WHERE company_id = $1 AND idempotency_key = 'expired-1'",
    )
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("age idempotency key");

    let (status, replacement) = send(
        &app,
        &actor,
        json!({"title": "Replacement after expiry", "idempotencyKey": "expired-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(replacement.get("deduplicated").is_none());
    let replacement_id = issue_id(&replacement);
    assert_ne!(replacement_id, first_id);

    let stored_issue_id: Uuid = sqlx::query_scalar(
        "SELECT issue_id
         FROM issue_create_idempotency_keys
         WHERE company_id = $1 AND idempotency_key = 'expired-1'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("replacement idempotency row");
    assert_eq!(stored_issue_id, replacement_id);

    let issue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues
         WHERE id IN ($1, $2)",
    )
    .bind(first_id)
    .bind(replacement_id)
    .fetch_one(&pool)
    .await
    .expect("replacement issue count");
    assert_eq!(issue_count, 2);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn expired_idempotency_cleanup_respects_the_500_row_batch_boundary(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);

    sqlx::query(
        "WITH stale AS (
             SELECT gen_random_uuid() AS id, generate_series(1, 501) AS ordinal
         ), inserted AS (
             INSERT INTO issues (
                 id, company_id, title, issue_number, identifier,
                 origin_fingerprint, created_at, updated_at
             )
             SELECT id, $1, format('stale issue %s', ordinal), 10000 + ordinal,
                    format('STALE-%s-%s', replace($1::text, '-', ''), ordinal),
                    format('stale-fingerprint-%s', ordinal),
                    NOW() - INTERVAL '8 days', NOW() - INTERVAL '8 days'
             FROM stale
             RETURNING id
         )
         INSERT INTO issue_create_idempotency_keys (
             company_id, idempotency_key, issue_id, created_at
         )
         SELECT $1, format('stale-key-%s', row_number() OVER ()), id,
                NOW() - INTERVAL '8 days'
         FROM inserted",
    )
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("seed stale idempotency rows");

    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));
    let (status, created) = send(
        &app,
        &actor,
        json!({"title": "After cleanup batch", "idempotencyKey": "batch-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "After cleanup batch");

    let stale_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM issue_create_idempotency_keys
         WHERE company_id = $1 AND idempotency_key LIKE 'stale-key-%'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("remaining stale keys");
    assert_eq!(stale_count, 1, "cleanup must delete at most 500 rows per request");

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn idempotency_cleanup_failure_returns_500_before_issue_creation(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    sqlx::query("DROP TABLE issue_create_idempotency_keys")
        .execute(&pool)
        .await
        .expect("drop idempotency table to simulate cleanup failure");

    let (status, _body) = send(
        &app,
        &actor,
        json!({"title": "Cleanup must fail", "idempotencyKey": "cleanup-failure-1"}),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let issue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues WHERE company_id = $1 AND title = 'Cleanup must fail'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("issue count after cleanup failure");
    assert_eq!(issue_count, 0);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn failed_issue_association_rolls_back_issue_and_idempotency_key(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (status, _body) = send(
        &app,
        &actor,
        json!({
            "title": "Should roll back",
            "idempotencyKey": "rollback-1",
            "labelIds": [Uuid::new_v4()],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let issue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues WHERE company_id = $1 AND title = 'Should roll back'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("issue rollback count");
    assert_eq!(issue_count, 0);

    let key_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_create_idempotency_keys
         WHERE company_id = $1 AND idempotency_key = 'rollback-1'",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("key rollback count");
    assert_eq!(key_count, 0);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn board_todo_without_owner_is_assigned_to_the_company_ceo_and_woken(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let ceo_id = seed_agent(&pool, fixture.company_id, "CEO", "ceo", "idle").await;
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (status, created) = send(
        &app,
        &actor,
        json!({
            "title": "Route to CEO",
            "status": "todo",
            "idempotencyKey": "route-ceo-1",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["assigneeAgentId"], json!(ceo_id));
    assert!(created["assigneeUserId"].is_null());
    assert!(matches!(
        created["status"].as_str(),
        Some("todo" | "in_progress")
    ));

    let created_issue_id = issue_id(&created);
    let wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
         WHERE company_id = $1 AND agent_id = $2 AND payload->>'issueId' = $3",
    )
    .bind(fixture.company_id)
    .bind(ceo_id)
    .bind(created_issue_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count CEO wake requests");
    assert_eq!(wake_count, 1, "the existing assignment wake path must run");

    // Idempotency replay must still return the already-created issue if the CEO
    // later becomes unavailable; it must not resolve or wake a new assignee.
    sqlx::query("UPDATE agents SET status = 'terminated' WHERE id = $1")
        .bind(ceo_id)
        .execute(&pool)
        .await
        .expect("terminate CEO after first create");
    let (replay_status, replay) = send(
        &app,
        &actor,
        json!({
            "title": "Changed retry title",
            "status": "todo",
            "idempotencyKey": "route-ceo-1",
        }),
    )
    .await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(issue_id(&replay), created_issue_id);

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn auto_assignment_rejects_missing_or_ambiguous_runnable_ceos(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let foreign_company = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    // A CEO in another tenant is not a candidate for this company.
    seed_agent(&pool, foreign_company.company_id, "Foreign CEO", "ceo", "idle").await;
    seed_agent(&pool, fixture.company_id, "Paused CEO", "ceo", "paused").await;
    seed_agent(&pool, fixture.company_id, "Pending CEO", "ceo", "pending_approval").await;
    seed_agent(&pool, fixture.company_id, "Terminated CEO", "ceo", "terminated").await;

    let (missing_status, missing_body) = send(
        &app,
        &actor,
        json!({"title": "No runnable CEO", "status": "todo"}),
    )
    .await;
    assert_eq!(missing_status, StatusCode::CONFLICT);
    assert!(missing_body["error"].as_str().unwrap().contains("no runnable CEO"));

    seed_agent(&pool, fixture.company_id, "Runnable CEO A", "ceo", "idle").await;
    seed_agent(&pool, fixture.company_id, "Runnable CEO B", "ceo", "running").await;
    let (ambiguous_status, ambiguous_body) = send(
        &app,
        &actor,
        json!({"title": "Multiple runnable CEOs", "status": "todo"}),
    )
    .await;
    assert_eq!(ambiguous_status, StatusCode::CONFLICT);
    assert!(ambiguous_body["error"]
        .as_str()
        .unwrap()
        .contains("multiple runnable CEOs"));

    let created_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues WHERE company_id = $1
         AND title IN ('No runnable CEO', 'Multiple runnable CEOs')",
    )
    .bind(fixture.company_id)
    .fetch_one(&pool)
    .await
    .expect("count conflicted issues");
    assert_eq!(created_count, 0, "conflicts must not leave unassigned todo rows");

    cleanup(&fixture).await;
    cleanup(&foreign_company).await;
}

#[sqlx::test]
async fn explicit_owners_backlog_children_and_agent_created_issues_are_unchanged(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let ceo_id = seed_agent(&pool, fixture.company_id, "CEO", "ceo", "idle").await;
    let member_id = seed_agent(&pool, fixture.company_id, "Engineer", "general", "idle").await;
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (explicit_status, explicit) = send(
        &app,
        &actor,
        json!({
            "title": "Keep explicit agent owner",
            "status": "todo",
            "assigneeAgentId": member_id,
        }),
    )
    .await;
    assert_eq!(explicit_status, StatusCode::CREATED);
    assert_eq!(explicit["assigneeAgentId"], json!(member_id));
    assert_ne!(explicit["assigneeAgentId"], json!(ceo_id));

    let user_assignee_id = Uuid::new_v4();
    let (user_assignee_status, user_assignee) = send(
        &app,
        &actor,
        json!({
            "title": "Keep explicit user assignee",
            "status": "todo",
            "assigneeUserId": user_assignee_id,
        }),
    )
    .await;
    assert_eq!(user_assignee_status, StatusCode::CREATED);
    assert_eq!(user_assignee["assigneeUserId"], json!(user_assignee_id));
    assert!(user_assignee["assigneeAgentId"].is_null());

    let responsible_user_id = Uuid::new_v4();
    let (responsible_status, responsible) = send(
        &app,
        &actor,
        json!({
            "title": "Keep explicit responsible user",
            "status": "todo",
            "responsibleUserId": responsible_user_id,
        }),
    )
    .await;
    assert_eq!(responsible_status, StatusCode::CREATED);
    assert_eq!(responsible["responsibleUserId"], json!(responsible_user_id));
    assert!(responsible["assigneeAgentId"].is_null());

    let (backlog_status, backlog) = send(
        &app,
        &actor,
        json!({"title": "Keep backlog parked", "status": "backlog"}),
    )
    .await;
    assert_eq!(backlog_status, StatusCode::CREATED);
    assert!(backlog["assigneeAgentId"].is_null());
    assert_eq!(backlog["status"], "backlog");

    let (parent_status, parent) = send(
        &app,
        &actor,
        json!({"title": "Unassigned parent", "status": "backlog"}),
    )
    .await;
    assert_eq!(parent_status, StatusCode::CREATED);
    let (child_status, child) = send(
        &app,
        &actor,
        json!({
            "title": "Board-created child",
            "status": "todo",
            "parentId": issue_id(&parent),
        }),
    )
    .await;
    assert_eq!(child_status, StatusCode::CREATED);
    assert!(child["assigneeAgentId"].is_null());

    let agent_actor = AuthorizationActor::agent(member_id, fixture.company_id, None);
    let (agent_status, agent_issue) = send(
        &app,
        &agent_actor,
        json!({"title": "Agent-created root", "status": "todo"}),
    )
    .await;
    assert_eq!(agent_status, StatusCode::CREATED);
    assert!(agent_issue["assigneeAgentId"].is_null());
    assert_eq!(agent_issue["createdByAgentId"], json!(member_id));

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn concurrent_board_todos_are_each_assigned_and_woken_once(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let ceo_id = seed_agent(&pool, fixture.company_id, "CEO", "ceo", "idle").await;
    let app = issue_routes()
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    let (left, right) = tokio::join!(
        send(
            &app,
            &actor,
            json!({"title": "Concurrent CEO task A", "status": "todo"}),
        ),
        send(
            &app,
            &actor,
            json!({"title": "Concurrent CEO task B", "status": "todo"}),
        ),
    );
    assert_eq!(left.0, StatusCode::CREATED);
    assert_eq!(right.0, StatusCode::CREATED);
    assert_eq!(left.1["assigneeAgentId"], json!(ceo_id));
    assert_eq!(right.1["assigneeAgentId"], json!(ceo_id));

    let issue_a = issue_id(&left.1);
    let issue_b = issue_id(&right.1);
    let wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
         WHERE company_id = $1 AND agent_id = $2
           AND payload->>'issueId' IN ($3, $4)",
    )
    .bind(fixture.company_id)
    .bind(ceo_id)
    .bind(issue_a.to_string())
    .bind(issue_b.to_string())
    .fetch_one(&pool)
    .await
    .expect("count concurrent CEO wake requests");
    assert_eq!(wake_count, 2);

    cleanup(&fixture).await;
}
