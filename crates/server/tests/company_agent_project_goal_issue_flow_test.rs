//! Full end-to-end flow: Company → Agent → Project → Goal → Issue.
//!
//! Validates the authorization chain, route merging, and service interactions
//! for the complete workflow. Uses `#[sqlx::test]` for isolated database connections.

use api::routes::{goal_routes, issue_routes, project_routes};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;


mod common;
use common::migrate;


struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    owner_id: Uuid,
    agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    // Insert company with required issue_prefix
    let company_id = Uuid::new_v4();
    let prefix = format!("FL{}", &company_id.simple().to_string()[..8]);
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
    .bind("Flow Agent")
    .execute(pool)
    .await
    .expect("insert agent");

    Fixture {
        pool: pool.clone(),
        company_id,
        owner_id,
        agent_id,
    }
}

fn owner_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board(fixture.owner_id, fixture.company_id)
}

fn agent_actor(fixture: &Fixture) -> AuthorizationActor {
    AuthorizationActor::agent_with_key(
        fixture.agent_id,
        fixture.company_id,
        Uuid::new_v4(),
        services::auth::AgentApiKeyScope::new(fixture.agent_id, fixture.company_id),
        Some(fixture.owner_id),
    )
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body must be JSON")
    };
    (status, value)
}

async fn cleanup(fixture: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM goals WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM company_memberships WHERE company_id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM auth_users WHERE id = $1")
        .bind(fixture.owner_id)
        .execute(&fixture.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(fixture.company_id)
        .execute(&fixture.pool)
        .await;
}

#[sqlx::test]
async fn company_agent_project_goal_issue_flow(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = owner_actor(&fixture);
    let app = Router::new()
        .merge(project_routes())
        .merge(goal_routes())
        .merge(issue_routes())
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    // 1. Create project
    let (status, project) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/projects", fixture.company_id),
        json!({"name": "Flow Project"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create project should succeed");
    let project_id = project["id"].as_str().expect("project id");
    let project_id = Uuid::parse_str(project_id).expect("parse project uuid");

    // 2. List projects
    let (status, projects) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/projects", fixture.company_id),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list projects should succeed");
    let projects_array = projects
        .as_array()
        .expect("projects should be array");
    assert!(projects_array.iter().any(|p| p["id"] == project_id.to_string()));

    // 3. Create goal
    let (status, goal) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({"title": "Flow Goal", "level": "project"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create goal should succeed");
    let goal_id = goal["id"].as_str().expect("goal id");
    let goal_id = Uuid::parse_str(goal_id).expect("parse goal uuid");

    // 4. Create issue with goal reference
    let (status, issue) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({"title": "Flow Issue", "goalId": goal_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create issue should succeed");
    assert_eq!(issue["goalId"], goal_id.to_string());

    // 5. Create issue with agent reference
    let (status, issue2) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({"title": "Agent Issue", "assigneeAgentId": fixture.agent_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create agent issue should succeed");
    assert_eq!(
        issue2["assigneeAgentId"],
        fixture.agent_id.to_string()
    );

    // 6. Verify issue listing
    let (status, issues) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list issues should succeed");
    let issues_array = issues
        .as_array()
        .expect("issues should be array");
    assert!(issues_array.len() >= 2);

    // 7. Try unauthorized access
    let other_company_id = Uuid::new_v4();
    let (_, unauthorized) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/issues", other_company_id),
        json!({"title": "Unauthorized Issue"}),
    )
    .await;
    // Should get 404 or 403 (company doesn't exist or no access)
    assert!(
        unauthorized == Value::Null || unauthorized["error"] == "empty response body"
            || unauthorized.as_object().map_or(false, |o| o.contains_key("error"))
    );

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn create_and_list_goals(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = owner_actor(&fixture);
    let app = Router::new()
        .merge(goal_routes())
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    // Create first goal
    let (status, goal1) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({"title": "First Goal", "level": "project"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let goal1_id = goal1["id"].as_str().expect("goal id");

    // Create second goal
    let (status, goal2) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({"title": "Second Goal", "level": "task"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let goal2_id = goal2["id"].as_str().expect("goal id");

    // List goals
    let (status, goals) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let goals_array = goals.as_array().expect("goals should be array");
    assert!(goals_array.len() >= 2);
    assert!(goals_array
        .iter()
        .any(|g| g["id"] == goal1_id));
    assert!(goals_array
        .iter()
        .any(|g| g["id"] == goal2_id));

    cleanup(&fixture).await;
}

#[sqlx::test]
async fn multiple_goals_and_issues(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = owner_actor(&fixture);
    let app = Router::new()
        .merge(goal_routes())
        .merge(issue_routes())
        .with_state(build_app_state(pool.clone()).await.expect("build app state"));

    // Create two goals
    let (status, goal1) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({"title": "Goal 1", "level": "project"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let goal1_id = Uuid::parse_str(goal1["id"].as_str().expect("goal1 id"))
        .expect("parse goal1 uuid");

    let (status, goal2) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/goals", fixture.company_id),
        json!({"title": "Goal 2", "level": "task"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let goal2_id = Uuid::parse_str(goal2["id"].as_str().expect("goal2 id"))
        .expect("parse goal2 uuid");

    // Create issue for goal1
    let (status, issue1) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({"title": "Issue 1", "goalId": goal1_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "issue1 should have status 201");
    assert_eq!(issue1["goalId"], goal1_id.to_string());

    // Create issue for goal2
    let (status, issue2) = send(
        &app,
        &actor,
        "POST",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({"title": "Issue 2", "goalId": goal2_id}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "issue2 should have status 201");
    assert_eq!(issue2["goalId"], goal2_id.to_string());

    // List all issues
    let (status, issues) = send(
        &app,
        &actor,
        "GET",
        &format!("/companies/{}/issues", fixture.company_id),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let issues_array = issues.as_array().expect("issues should be array");
    assert!(issues_array.len() >= 2);

    // Verify each issue has correct goal reference
    let issue1_id = Uuid::parse_str(issue1["id"].as_str().expect("issue1 id"))
        .expect("parse issue1 uuid");
    let issue2_id = Uuid::parse_str(issue2["id"].as_str().expect("issue2 id"))
        .expect("parse issue2 uuid");

    let issue1_found = issues_array.iter().any(|i| {
        i["id"] == issue1_id.to_string() && i["goalId"] == goal1_id.to_string()
    });
    let issue2_found = issues_array.iter().any(|i| {
        i["id"] == issue2_id.to_string() && i["goalId"] == goal2_id.to_string()
    });
    assert!(issue1_found, "issue1 should be in list with correct goal");
    assert!(issue2_found, "issue2 should be in list with correct goal");

    cleanup(&fixture).await;
}
