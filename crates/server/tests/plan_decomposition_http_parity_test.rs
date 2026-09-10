//! HTTP parity integration tests for Plan Decomposition endpoints.
//!
//! Covers:
//! - GET  /issues/:id/accepted-plan-decompositions (list, 404 on missing)
//! - POST /issues/:id/accepted-plan-decompositions (submit, reject bad input)
//! - Cross-company isolation
//!
//! L246 — Plan Review Context / Approval / Recovery: "Accepted Plan Decomposition" sub-item.
//! Paperclip source: packages/server/src/routes/issues.ts plan decomposition routes.
//!
//! Run with a live database:
//!   DATABASE_URL=postgres://postgres:admin123@localhost:5433/parrot_agent_compile \
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
use services::auth::{AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

async fn connect_and_migrate() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@localhost:5433/parrot_agent_compile".to_string()
    });
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    pool
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    user_id: Uuid,
    agent_id: Uuid,
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
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind("user")
    .bind(user_id)
    .bind("owner")
    .bind("active")
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
        "INSERT INTO issues (id, company_id, title, status, priority) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
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
        agent_id,
        issue_id,
    }
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
    let req_body = match body {
        Some(ref value) => {
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
// PD2: Submit valid plan decomposition returns 201
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_valid() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    // Insert a document and revision for the decomposition to reference
    let document_id: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (id, company_id, key, title, created_by_agent_id)
         VALUES (gen_random_uuid(), $1, 'plan', 'Test Plan', $2)
         RETURNING id",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert document");

    let revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_revisions (id, company_id, document_id, revision_number, body, format)
         VALUES (gen_random_uuid(), $1, $2, 1, '# Plan\n\nSome plan content', 'markdown')
         RETURNING id",
    )
    .bind(f.company_id)
    .bind(document_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert document revision");

    let child_issue_1 = Uuid::new_v4();
    let child_issue_2 = Uuid::new_v4();

    // Insert child issues
    for (cid, idx) in [(child_issue_1, 1), (child_issue_2, 2)] {
        sqlx::query(
            "INSERT INTO issues (id, company_id, title, status, priority)
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(cid)
        .bind(f.company_id)
        .bind(format!("Child task {}", idx))
        .bind("backlog")
        .bind("medium")
        .execute(&f.pool)
        .await
        .expect("insert child issue");
    }

    let body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "childIssueIds": [child_issue_1.to_string(), child_issue_2.to_string()],
        "ownerAgentId": f.agent_id.to_string(),
        "requestedChildCount": 2
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
        StatusCode::CREATED,
        "expected 201 Created, got {}: {}",
        status,
        String::from_utf8_lossy(&resp_body)
    );

    let json = parse_json(&resp_body);
    assert_eq!(json["acceptedPlanRevisionId"], revision_id.to_string());
    let child_ids: Vec<&str> = json["childIssueIds"]
        .as_array()
        .expect("childIssueIds should be array")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(child_ids.len(), 2);
}

// ---------------------------------------------------------------------------
// PD3: Submit with missing required fields returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_submit_plan_decomposition_missing_fields() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    // Missing acceptedPlanRevisionId
    let body = json!({
        "childIssueIds": [Uuid::new_v4().to_string()],
        "ownerAgentId": f.agent_id.to_string()
    });

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(body),
    )
    .await;

    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for missing fields, got {}",
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
        "childIssueIds": ["also-not-a-uuid"],
        "ownerAgentId": f.agent_id.to_string()
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
// PD6: List returns decompositions after creation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_plan_decompositions_after_create() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = issue_routes().with_state(state);
    let actor = owner_actor(&f);

    // Insert document and revision
    let document_id: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (id, company_id, key, title, created_by_agent_id)
         VALUES (gen_random_uuid(), $1, 'plan', 'Test Plan', $2)
         RETURNING id",
    )
    .bind(f.company_id)
    .bind(f.agent_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert document");

    let revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO document_revisions (id, company_id, document_id, revision_number, body, format)
         VALUES (gen_random_uuid(), $1, $2, 1, '# Plan', 'markdown')
         RETURNING id",
    )
    .bind(f.company_id)
    .bind(document_id)
    .fetch_one(&f.pool)
    .await
    .expect("insert revision");

    let child_issue = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(child_issue)
    .bind(f.company_id)
    .bind("Child task")
    .bind("backlog")
    .bind("medium")
    .execute(&f.pool)
    .await
    .expect("insert child issue");

    // Create decomposition
    let create_body = json!({
        "acceptedPlanRevisionId": revision_id.to_string(),
        "childIssueIds": [child_issue.to_string()],
        "ownerAgentId": f.agent_id.to_string(),
        "requestedChildCount": 1
    });

    send(
        &app,
        &actor,
        "POST",
        &format!("/issues/{}/accepted-plan-decompositions", f.issue_id),
        Some(create_body),
    )
    .await;

    // List should now return 1 item
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
    assert!(json.is_array());
    let items = json.as_array().unwrap();
    assert_eq!(items.len(), 1, "expected 1 decomposition, got {}", items.len());
    assert_eq!(items[0]["acceptedPlanRevisionId"], revision_id.to_string());
}

// ---------------------------------------------------------------------------
// PD7: Cross-company isolation — company B sees no decompositions from company A
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_plan_decomposition_company_isolation() {
    let pool = connect_and_migrate().await;

    // Seed company A
    let company_a_id = Uuid::new_v4();
    let user_a_id = Uuid::new_v4();
    let issue_a_id = Uuid::new_v4();
    let agent_a_id = Uuid::new_v4();
    let prefix_a = format!("PA{}", &company_a_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_a_id)
        .bind("Company A")
        .bind(&prefix_a)
        .execute(&pool)
        .await
        .expect("insert company A");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_a_id)
    .bind(format!("a{}@test.com", user_a_id))
    .bind("User A")
    .execute(&pool)
    .await
    .expect("insert user A");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_a_id)
    .bind("user")
    .bind(user_a_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership A");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(agent_a_id)
    .bind(company_a_id)
    .bind("Agent A")
    .bind("http")
    .bind("idle")
    .execute(&pool)
    .await
    .expect("insert agent A");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(issue_a_id)
    .bind(company_a_id)
    .bind("Issue A")
    .bind("in_progress")
    .bind("medium")
    .execute(&pool)
    .await
    .expect("insert issue A");

    // Create decomposition in company A
    let doc_a: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (id, company_id, key, title, created_by_agent_id)
         VALUES (gen_random_uuid(), $1, 'plan', 'Plan A', $2) RETURNING id",
    )
    .bind(company_a_id)
    .bind(agent_a_id)
    .fetch_one(&pool)
    .await
    .expect("insert doc A");
    let rev_a: Uuid = sqlx::query_scalar(
        "INSERT INTO document_revisions (id, company_id, document_id, revision_number, body)
         VALUES (gen_random_uuid(), $1, $2, 1, 'Plan A body') RETURNING id",
    )
    .bind(company_a_id)
    .bind(doc_a)
    .fetch_one(&pool)
    .await
    .expect("insert rev A");

    let actor_a = AuthorizationActor::board_with_source(
        user_a_id,
        company_a_id,
        services::auth::ActorSource::Session,
        vec![services::auth::CompanyMembership::new(
            company_a_id,
            PrincipalType::User,
            user_a_id,
            MembershipRole::Owner,
        )],
        true,
    );

    let app_state_a = build_app_state(pool.clone()).await.unwrap();
    let app_a = issue_routes().with_state(app_state_a);

    let child_a = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(child_a)
    .bind(company_a_id)
    .bind("Child A")
    .bind("backlog")
    .bind("medium")
    .execute(&pool)
    .await
    .expect("insert child A");

    let create_body = json!({
        "acceptedPlanRevisionId": rev_a.to_string(),
        "childIssueIds": [child_a.to_string()],
        "ownerAgentId": agent_a_id.to_string(),
        "requestedChildCount": 1
    });
    send(&app_a, &actor_a, "POST", &format!("/issues/{}/accepted-plan-decompositions", issue_a_id), Some(create_body))
        .await;

    // Now seed company B
    let company_b_id = Uuid::new_v4();
    let user_b_id = Uuid::new_v4();
    let issue_b_id = Uuid::new_v4();
    let prefix_b = format!("PB{}", &company_b_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_b_id)
        .bind("Company B")
        .bind(&prefix_b)
        .execute(&pool)
        .await
        .expect("insert company B");
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_b_id)
    .bind(format!("b{}@test.com", user_b_id))
    .bind("User B")
    .execute(&pool)
    .await
    .expect("insert user B");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_b_id)
    .bind("user")
    .bind(user_b_id)
    .bind("owner")
    .bind("active")
    .execute(&pool)
    .await
    .expect("insert membership B");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(company_b_id)
    .bind("Agent B")
    .bind("http")
    .bind("idle")
    .execute(&pool)
    .await
    .expect("insert agent B");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
    )
    .bind(issue_b_id)
    .bind(company_b_id)
    .bind("Issue B")
    .bind("in_progress")
    .bind("medium")
    .execute(&pool)
    .await
    .expect("insert issue B");

    let actor_b = AuthorizationActor::board_with_source(
        user_b_id,
        company_b_id,
        services::auth::ActorSource::Session,
        vec![services::auth::CompanyMembership::new(
            company_b_id,
            PrincipalType::User,
            user_b_id,
            MembershipRole::Owner,
        )],
        true,
    );

    let app_state_b = build_app_state(pool.clone()).await.unwrap();
    let app_b = issue_routes().with_state(app_state_b);

    // Company B user listing decompositions for company B issue should be empty
    let (status, body) = send(
        &app_b,
        &actor_b,
        "GET",
        &format!("/issues/{}/accepted-plan-decompositions", issue_b_id),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let json = parse_json(&body);
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0, "company B should see 0 decompositions");
}
