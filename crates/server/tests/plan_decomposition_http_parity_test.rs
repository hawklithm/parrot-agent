//! HTTP parity integration tests for Accepted Plan Decomposition endpoints.
//!
//! Covers:
//! - GET  /issues/:id/accepted-plan-decompositions (list, 404 on missing)
//! - POST /issues/:id/accepted-plan-decompositions (submit, reject bad input)
//! - Idempotent replay, conflicting payload, cross-company isolation
//!
//! L246 — Plan Review Context / Approval / Recovery: "Accepted Plan Decomposition" sub-item.
//! Paperclip source: server/src/routes/issues.ts plan decomposition routes.
//!
//! Run with a live database:
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test plan_decomposition_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::issues::issue_routes;
use parrot_server::build_app_state;
use services::auth::{AuthorizationActor, MembershipRole, PrincipalType};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------


mod common;
use common::connect_and_migrate;


struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    issue_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("PD{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("PlanDecompositionTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(format!("pd{}@test.com", user_id))
    .bind("PD User")
    .execute(pool)
    .await
    .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active') ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");

    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("PD Agent")
    .bind("http")
    .bind("idle")
    .execute(pool)
    .await
    .expect("insert agent");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority) VALUES ($1, $2, $3, $4::issue_status, $5::issue_priority) ON CONFLICT DO NOTHING",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind("Plan decomposition test issue")
    .bind("in_progress")
    .bind("medium")
    .execute(pool)
    .await
    .expect("insert issue");

    Fixture {
        pool: pool.clone(),
        company_id,
        user_id,
        issue_id,
    }
}

/// The live `documents` table keys documents by id and carries the body
/// inline; the `plan` key lives on `issue_documents`, and the immutable
/// history lives in `document_revisions`.
async fn seed_plan_revision(f: &Fixture, revision_number: i32) -> Uuid {
    let document_id: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (id, company_id, title, content, content_type)
         VALUES (gen_random_uuid(), $1, 'Plan', '# Plan', 'markdown')
         RETURNING id",
    )
    .bind(f.company_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert document");

    sqlx::query(
        "INSERT INTO issue_documents (company_id, issue_id, document_id, key)
         VALUES ($1, $2, $3, 'plan')",
    )
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(document_id)
    .execute(&f.pool)
    .await
    .expect("link issue document");

    sqlx::query_scalar(
        "INSERT INTO document_revisions
             (id, company_id, document_id, revision_number, content, created_by_type, created_by_id)
         VALUES (gen_random_uuid(), $1, $2, $3, '# Plan', 'user', $4)
         RETURNING id",
    )
    .bind(f.company_id)
    .bind(document_id)
    .bind(revision_number)
    .bind(f.user_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert document revision")
}

/// An accepted `request_confirmation` bound to the plan document revision is
/// what makes a revision "accepted" and therefore decomposable.
async fn seed_accepted_confirmation(f: &Fixture, revision_id: Uuid) -> Uuid {
    let interaction_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issue_thread_interactions
             (id, company_id, issue_id, kind, status, title, summary, payload, resolved_at)
         VALUES ($1, $2, $3, 'request_confirmation', 'accepted', 'Accept plan', 'Accept plan', $4, NOW())",
    )
    .bind(interaction_id)
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(json!({
        "target": {
            "type": "issue_document",
            "issueId": f.issue_id.to_string(),
            "key": "plan",
            "revisionId": revision_id.to_string(),
        }
    }))
    .execute(&f.pool)
    .await
    .expect("insert accepted confirmation");
    interaction_id
}

/// A fully decomposable issue: plan document, accepted revision, confirmation.
async fn seed_decomposable(f: &Fixture) -> Uuid {
    let revision_id = seed_plan_revision(f, 1).await;
    seed_accepted_confirmation(f, revision_id).await;
    revision_id
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        fixture.user_id,
        fixture.company_id,
        services::auth::ActorSource::Session,
        vec![services::auth::CompanyMembership::new(
            fixture.company_id,
            PrincipalType::User,
            fixture.user_id,
            MembershipRole::Owner,
        )],
        true,
    )
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    let req_body = match &body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize request body"))
        }
        None => Body::empty(),
    };
    let mut req = builder.body(req_body).expect("build request");
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

fn ids_of(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("expected a JSON array")
        .iter()
        .map(|item| item.as_str().expect("array items must be strings").to_string())
        .collect()
}

async fn child_count(f: &Fixture, parent_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE parent_id = $1")
        .bind(parent_id)
        .fetch_one(&f.pool)
        .await
        .expect("count child issues")
}

// ---------------------------------------------------------------------------
// PD1: Empty list returns 200 with empty array
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_plan_decompositions_empty() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "expected 200 OK, got {}", status);
    let json = parse_json(&body);
    assert!(json.is_array(), "response should be a JSON array");
    assert_eq!(json.as_array().unwrap().len(), 0, "expected empty array");
}

// ---------------------------------------------------------------------------
// PD2: Submitting an accepted plan revision creates its children and returns
//      Paperclip's `{decomposition, childIssueIds, newlyCreatedChildIssueIds}`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_valid() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;

    let body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [
            { "title": "Child task 1", "priority": "high" },
            { "title": "Child task 2" }
        ]
    });

    let (status, resp_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    let decomposition = &json["decomposition"];
    assert_eq!(
        decomposition["acceptedPlanRevisionId"],
        revision_id.to_string()
    );
    assert_eq!(decomposition["status"], "completed");
    assert_eq!(decomposition["requestedChildCount"], 2);
    assert!(
        decomposition["completedAt"].is_string(),
        "completedAt must be populated: {decomposition}"
    );
    // Paperclip deliberately omits `requestedChildren` from the API shape.
    assert!(
        decomposition.get("requestedChildren").is_none(),
        "requestedChildren must not be exposed"
    );

    let child_ids = ids_of(&json["childIssueIds"]);
    assert_eq!(child_ids.len(), 2, "expected 2 children: {json}");
    assert_eq!(
        ids_of(&json["newlyCreatedChildIssueIds"]).len(),
        2,
        "first attempt must report both children as new: {json}"
    );
    assert_eq!(child_count(&f, f.issue_id).await, 2, "children persisted");

    // Every returned id must be a live child issue of the source issue.
    for child_id in &child_ids {
        let parent: Option<Uuid> =
            sqlx::query_scalar("SELECT parent_id FROM issues WHERE id = $1::uuid")
                .bind(child_id)
                .fetch_one(&f.pool)
                .await
                .expect("load child");
        assert_eq!(parent, Some(f.issue_id), "child parent linkage");
    }
}

// ---------------------------------------------------------------------------
// PD3: Missing `children` is a bad request
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_missing_fields() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;

    // `children` absent entirely.
    let body = json!({ "acceptedPlanRevisionId": revision_id.to_string() });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for missing children, got {}",
        status
    );

    // `children` present but empty violates `.min(1)`.
    let body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": []
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for empty children, got {}",
        status
    );
}

// ---------------------------------------------------------------------------
// PD4: Submit with invalid UUID format returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_invalid_uuid() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    let body = json!({
        "acceptedPlanRevisionId": "not-a-uuid",
        "children": [{ "title": "Child" }]
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "expected 400 for invalid UUID, got {}",
        status
    );
}

// ---------------------------------------------------------------------------
// PD5: GET for non-existent issue returns 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_plan_decompositions_nonexistent_issue() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    let fake_issue_id = Uuid::new_v4();

    let (status, _) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", fake_issue_id),
        None,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404 for non-existent issue, got {}",
        status
    );
}

// ---------------------------------------------------------------------------
// PD6: A revision outside the issue's plan document is unprocessable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_revision_not_in_plan_document() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    // A revision that exists but belongs to no document of this issue.
    let orphan_document_id: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (id, company_id, title, content, content_type)
         VALUES (gen_random_uuid(), $1, 'Unrelated', 'x', 'markdown') RETURNING id",
    )
    .bind(f.company_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert unrelated document");

    let orphan_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_revisions
             (id, company_id, document_id, revision_number, content)
         VALUES (gen_random_uuid(), $1, $2, 1, 'x') RETURNING id",
    )
    .bind(f.company_id)
    .bind(orphan_document_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert orphan revision");

    let body = json!({
        "acceptedPlanRevisionId": orphan_revision_id.to_string(),
        "children": [{ "title": "Child" }]
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "expected 422 for a revision outside the plan document, got {}",
        status
    );
    assert_eq!(child_count(&f, f.issue_id).await, 0, "no child created");
}

// ---------------------------------------------------------------------------
// PD7: A plan revision with no accepted confirmation is unprocessable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_without_accepted_confirmation() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    // In the plan document, but nothing ever accepted it.
    let revision_id = seed_plan_revision(&f, 1).await;

    let body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child" }]
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "expected 422 without an accepted plan confirmation, got {}",
        status
    );
    assert_eq!(child_count(&f, f.issue_id).await, 0, "no child created");
}

// ---------------------------------------------------------------------------
// PD8: List returns the accepted decomposition with its child summaries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_plan_decompositions_after_create() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;

    let create_body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child task" }]
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "seed decomposition");

    let (status, body) = send(
        &app,
        &actor,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    let items = json.as_array().expect("array response");
    assert_eq!(items.len(), 1, "expected 1 decomposition, got {}", items.len());

    let summary = &items[0];
    assert_eq!(summary["acceptedPlanRevisionId"], revision_id.to_string());
    assert_eq!(summary["acceptedPlanRevisionNumber"], 1);
    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["requestedChildCount"], 1);

    // The UI renders these; they must be live rows, not stale snapshots.
    let children = summary["childIssues"].as_array().expect("childIssues array");
    assert_eq!(children.len(), 1, "expected 1 child summary: {summary}");
    assert_eq!(children[0]["title"], "Child task");
    assert_eq!(children[0]["status"], "backlog");
    assert_eq!(children[0]["priority"], "medium");
    assert_eq!(
        summary["childIssueIds"][0],
        children[0]["id"],
        "childIssueIds and childIssues must agree"
    );
}

// ---------------------------------------------------------------------------
// PD9: Replaying the identical request is idempotent — no duplicate children,
//      and nothing reported as newly created the second time.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_replay_is_idempotent() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;

    let body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child A" }, { "title": "Child B" }]
    });

    let (first_status, first_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body.clone()),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK, "first submit");
    let first = parse_json(&first_body);

    let (second_status, second_body) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;
    assert_eq!(
        second_status,
        StatusCode::OK,
        "replay must succeed: {}",
        String::from_utf8_lossy(&second_body)
    );
    let second = parse_json(&second_body);

    // Same children, nothing new, and crucially no duplicated rows.
    assert_eq!(
        ids_of(&second["childIssueIds"]),
        ids_of(&first["childIssueIds"]),
        "replay must return the original children"
    );
    assert!(
        ids_of(&second["newlyCreatedChildIssueIds"]).is_empty(),
        "replay must not create anything: {second}"
    );
    assert_eq!(
        second["decomposition"]["id"], first["decomposition"]["id"],
        "replay must reuse the original claim"
    );
    assert_eq!(
        second["decomposition"]["completedAt"], first["decomposition"]["completedAt"],
        "replay must report the original completion time"
    );
    assert_eq!(
        child_count(&f, f.issue_id).await,
        2,
        "replay must not duplicate children"
    );
}

// ---------------------------------------------------------------------------
// PD10: Same revision with a different payload conflicts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_conflicting_payload() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;

    let first_body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child A" }]
    });
    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(first_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first submit");

    // Same accepted revision, materially different children.
    let conflicting_body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child A" }, { "title": "Child B" }]
    });
    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(conflicting_body),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "expected 409 for a conflicting payload, got {}",
        status
    );
    assert_eq!(
        child_count(&f, f.issue_id).await,
        1,
        "conflicting request must not add children"
    );
}

// ---------------------------------------------------------------------------
// PD11: Cross-company isolation — company B sees no decompositions from company A
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_plan_decomposition_company_isolation() {
    let pool = connect_and_migrate().await;

    // --- Company A: create a decomposition on its own issue. ---
    let f = seed(&pool).await;
    let actor_a = owner_actor(&f);
    let revision_id = seed_decomposable(&f).await;
    let app_a = issue_routes().with_state(build_app_state(pool.clone()).await.unwrap());

    let create_body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "children": [{ "title": "Child A" }]
    });
    let (status, _) = send(
        &app_a,
        &actor_a,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(create_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "company A decomposition");

    // --- Company B: its own issue, its own actor. ---
    let g = seed(&pool).await;
    let actor_b = owner_actor(&g);
    let app_b = issue_routes().with_state(build_app_state(pool.clone()).await.unwrap());

    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", g.issue_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert!(json.is_array());
    assert_eq!(
        json.as_array().unwrap().len(),
        0,
        "company B should see 0 decompositions"
    );

    // Company B also cannot read company A's issue by id.
    let (status, _) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-company issue must not be visible"
    );
}
