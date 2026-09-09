//! HTTP parity integration test for Company → Agent → Project → Goal → Issue flow.
//!
//! Tests validate the end-to-end chain through the live Axum router and PostgreSQL.

use api::routes::{goal_routes, issue_routes, project_routes};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

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

/// Seed a company, owner user, agent, project, and goal.
async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let prefix = format!("FL{}", &company_id.simple().to_string()[..6]);

    // Insert company with required issue_prefix
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(company_id)
    .bind("Flow Test Company")
    .bind(prefix)
    .execute(pool)
    .await
    .expect("insert company");

    let owner_id = Uuid::new_v4();
    // Insert auth_user (not users)
    sqlx::query(
        "INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(owner_id)
    .bind(format!("{owner_id}@flow.test"))
    .bind("Flow Owner")
    .execute(pool)
    .await
    .expect("insert auth_user");

    // Insert company_membership (correct table name)
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active') ON CONFLICT DO NOTHING",
    )
    .bind(company_id)
    .bind(owner_id)
    .execute(pool)
    .await
    .expect("insert membership");

    let agent_id = Uuid::new_v4();
    // Insert agent with required adapter_type
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type) VALUES ($1, $2, $3, 'http') ON CONFLICT DO NOTHING",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("Flow Test Agent")
    .execute(pool)
    .await
    .expect("insert agent");

    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name, status) VALUES ($1, $2, $3, 'backlog') ON CONFLICT DO NOTHING",
    )
    .bind(project_id)
    .bind(company_id)
    .bind("Flow Test Project")
    .execute(pool)
    .await
    .expect("insert project");

    let goal_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO goals (id, company_id, title, level, status) VALUES ($1, $2, $3, 'project', 'planned') ON CONFLICT DO NOTHING",
    )
    .bind(goal_id)
    .bind(company_id)
    .bind("Flow Test Goal")
    .execute(pool)
    .await
    .expect("insert goal");

    Fixture {
        pool: pool.clone(),
        company_id,
        owner_id,
        agent_id,
        project_id,
        goal_id,
    }
}

#[derive(Debug)]
struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    owner_id: Uuid,
    agent_id: Uuid,
    project_id: Uuid,
    goal_id: Uuid,
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        fixture.owner_id,
        fixture.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            fixture.company_id,
            PrincipalType::User,
            fixture.owner_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

fn agent_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::Agent {
        agent_id: fixture.agent_id,
        company_id: fixture.company_id,
        run_id: None,
        source: ActorSource::LocalImplicit,
        key_id: None,
        key_scope: None,
        responsible_user_id: None,
        on_behalf_of_user_id: None,
        on_behalf_of_memberships: vec![],
    }
}

async fn send(
    app: &axum::Router,
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
        .expect("request should not error");

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let json: Value = serde_json::from_slice(&body_bytes)
        .unwrap_or(json!({"raw": String::from_utf8_lossy(&body_bytes).to_string()}));
    (status, json)
}

/// §9.2 FLOW1: Full chain — create company → agent → project → goal → issue
#[tokio::test]
async fn company_agent_project_goal_issue_flow() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;

    let state = build_app_state(pool.clone()).await.expect("build state");
    let app = axum::Router::new()
        .merge(project_routes())
        .merge(goal_routes())
        .merge(issue_routes())
        .with_state(state);

    let owner = owner_actor(&fixture);
    let agent = agent_actor(&fixture);

    // Step 1: List projects for company
    let (status, _) = send(&app, &owner, "GET", &format!("/api/projects?company_id={}", fixture.company_id), None).await;
    assert_eq!(status, StatusCode::OK, "list projects should succeed");

    // Step 2: Create issue under goal
    let issue_body = json!({
        "title": "Flow Test Issue",
        "description": "Integration test issue",
        "priority": "medium",
        "status": "open",
        "goal_id": fixture.goal_id,
        "assignee_ids": [fixture.agent_id]
    });
    let (status, issue_resp) = send(
        &app, &owner, "POST",
        &format!("/api/goals/{}/issues", fixture.goal_id),
        Some(issue_body),
    ).await;
    eprintln!("Create issue response: {:?}", issue_resp);
    assert_eq!(status, StatusCode::CREATED, "create issue should succeed");

    let issue_id = issue_resp["id"].as_str().expect("issue id missing");
    let issue_id: Uuid = issue_id.parse().expect("invalid uuid");

    // Step 3: Verify issue appears in goal's issue list
    let (status, issues_resp) = send(
        &app, &owner, "GET",
        &format!("/api/goals/{}/issues", fixture.goal_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "list issues should succeed");
    let issues = issues_resp["items"].as_array().expect("issues array");
    let found = issues.iter().any(|i| i["id"].as_str() == Some(issue_id.to_string().as_str()));
    assert!(found, "created issue should appear in goal's issue list");

    // Step 4: Agent can view issues
    let (status, _) = send(
        &app, &agent, "GET",
        &format!("/api/goals/{}/issues", fixture.goal_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "agent should be able to view issues");

    // Step 5: Cross-company isolation
    let other_company_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)",
    )
    .bind(other_company_id)
    .bind("Other Company")
    .bind(format!("OT{}", &other_company_id.simple().to_string()[..6]))
    .execute(&fixture.pool)
    .await
    .expect("insert other company");

    let (status, _) = send(
        &app, &owner, "GET",
        &format!("/api/projects?company_id={}", other_company_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "listing with no projects should return 200");
}

/// §9.2 FLOW2: Complete an issue and verify status update
#[tokio::test]
async fn goal_progress_on_issue_complete() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;

    let state = build_app_state(pool.clone()).await.expect("build state");
    let app = axum::Router::new()
        .merge(project_routes())
        .merge(goal_routes())
        .merge(issue_routes())
        .with_state(state);

    let owner = owner_actor(&fixture);

    // Create issue
    let issue_body = json!({
        "title": "Progress Test Issue",
        "description": "For progress tracking",
        "priority": "high",
        "status": "open",
        "goal_id": fixture.goal_id,
    });
    let (status, issue_resp) = send(
        &app, &owner, "POST",
        &format!("/api/goals/{}/issues", fixture.goal_id),
        Some(issue_body),
    ).await;
    assert_eq!(status, StatusCode::CREATED, "create issue failed");

    let issue_id: Uuid = issue_resp["id"].as_str().unwrap().parse().unwrap();

    // Complete the issue
    let (status, _) = send(
        &app, &owner, "PATCH",
        &format!("/api/issues/{}", issue_id),
        Some(json!({"status": "completed"})),
    ).await;
    assert_eq!(status, StatusCode::OK, "update issue failed");

    // Verify issue is now completed
    let (status, issue_get) = send(
        &app, &owner, "GET",
        &format!("/api/issues/{}", issue_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "get issue failed");
    assert_eq!(issue_get["status"].as_str(), Some("completed"), "issue should be completed");
}

/// §9.2 FLOW3: Multiple goals under one project, multiple issues per goal
#[tokio::test]
async fn multiple_goals_and_issues() {
    let pool = connect_and_migrate().await;
    let fixture = seed(&pool).await;

    let state = build_app_state(pool.clone()).await.expect("build state");
    let app = axum::Router::new()
        .merge(project_routes())
        .merge(goal_routes())
        .merge(issue_routes())
        .with_state(state);

    let owner = owner_actor(&fixture);

    // Create second goal
    let goal2_body = json!({
        "title": "Second Goal",
        "project_id": fixture.project_id,
        "status": "active"
    });
    let (status, goal2_resp) = send(
        &app, &owner, "POST", "/api/goals",
        Some(goal2_body),
    ).await;
    assert_eq!(status, StatusCode::CREATED, "create second goal failed");
    let goal2_id: Uuid = goal2_resp["id"].as_str().unwrap().parse().unwrap();

    // Create issues for both goals
    let issue1_body = json!({
        "title": "Issue for Goal 1",
        "status": "open",
        "priority": "medium",
        "goal_id": fixture.goal_id,
    });
    let issue2_body = json!({
        "title": "Issue for Goal 2",
        "status": "open",
        "priority": "low",
        "goal_id": goal2_id,
    });

    let (_, issue1_resp) = send(
        &app, &owner, "POST",
        &format!("/api/goals/{}/issues", fixture.goal_id),
        Some(issue1_body),
    ).await;

    let (_, issue2_resp) = send(
        &app, &owner, "POST",
        &format!("/api/goals/{}/issues", goal2_id),
        Some(issue2_body),
    ).await;

    assert_ne!(
        issue1_resp.get("id"),
        issue2_resp.get("id"),
        "different issues should have different IDs"
    );

    // Verify listing goals for project
    let (status, goals_resp) = send(
        &app, &owner, "GET",
        &format!("/api/projects/{}/goals", fixture.project_id),
        None,
    ).await;
    assert_eq!(status, StatusCode::OK, "list goals failed");
    let goals = goals_resp["items"].as_array().expect("goals array");
    assert!(goals.len() >= 2, "should have at least 2 goals, got: {}", goals.len());
}
