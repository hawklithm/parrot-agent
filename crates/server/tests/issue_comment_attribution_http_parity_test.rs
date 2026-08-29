//! Issue comment attribution HTTP + PostgreSQL parity test
//! (PAPERCLIP_MIGRATION_PLAN §4B.1 line 385).
//!
//! Paperclip distinguishes *who wrote a comment* from *who it was written
//! for*. Parrot persisted neither: `issue_comments` had no
//! `on_behalf_of_user_id` and no derived-author columns, so an agent comment
//! could not be attributed to a responsible user and a sentinel-authored
//! comment had no recoverable author.
//!
//! Covered here:
//!   - an explicit `onBehalfOfUserId` is persisted verbatim
//!   - an agent comment derives the on-behalf-of user from the creating
//!     heartbeat run's responsible user when none is supplied
//!   - a user (board) comment never receives derived on-behalf-of attribution
//!   - an unknown/stale run id yields `null` instead of failing the insert
//!   - `authorType` defaults to the actor type and can be overridden
//!   - `sourceTrust` round-trips

use api::routes::issue_comments::issue_comment_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
}

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    agent_id: Uuid,
    issue_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("CA{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Comment Attribution Co")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type)
         VALUES ($1, $2, $3, 'running', 'process')",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("Attribution agent")
    .execute(pool)
    .await
    .expect("insert agent");
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, assignee_agent_id)
         VALUES ($1, $2, $3, 'in_progress'::issue_status, $4)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind("Attribution issue")
    .bind(agent_id)
    .execute(pool)
    .await
    .expect("insert issue");

    Fixture {
        pool: pool.clone(),
        company_id,
        agent_id,
        issue_id,
    }
}

async fn cleanup(f: &Fixture) {
    let _ = sqlx::query("DELETE FROM issues WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM agents WHERE id = $1")
        .bind(f.agent_id)
        .execute(&f.pool)
        .await;
    let _ = sqlx::query("DELETE FROM companies WHERE id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await;
}

/// Insert a heartbeat run carrying a responsible user.
async fn seed_run(f: &Fixture, responsible_user_id: Option<&str>) -> Uuid {
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO heartbeat_runs (id, company_id, agent_id, status, responsible_user_id, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4, $5)",
    )
    .bind(run_id)
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(responsible_user_id)
    .bind(json!({"issueId": f.issue_id.to_string()}))
    .execute(&f.pool)
    .await
    .expect("insert heartbeat run");
    run_id
}

async fn send(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(ref value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(value).expect("serialize body"))
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).expect("build request");
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

fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Owner,
        )],
        false,
    )
}

async fn read_attribution(f: &Fixture, comment_id: Uuid) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT on_behalf_of_user_id, author_type
           FROM issue_comments WHERE id = $1",
    )
    .bind(comment_id)
    .fetch_one(&f.pool)
    .await
    .expect("read comment attribution")
}

/// An explicit `onBehalfOfUserId` is persisted verbatim.
#[sqlx::test]
async fn explicit_on_behalf_of_user_is_persisted(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_comment_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    let on_behalf_of = Uuid::new_v4().to_string();
    let (status, body) = send(
        &app,
        &AuthorizationActor::agent(f.agent_id, f.company_id, None),
        "POST",
        &format!("/issues/{}/comments", f.issue_id),
        Some(json!({
            "body": "Acting on behalf of a user",
            "actor_type": "agent",
            "onBehalfOfUserId": on_behalf_of,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    let comment_id = Uuid::parse_str(body["comment"]["id"].as_str().expect("comment id")).expect("uuid");
    let (stored, author_type) = read_attribution(&f, comment_id).await;
    assert_eq!(stored, Some(on_behalf_of));
    assert_eq!(author_type.as_deref(), Some("agent"));

    cleanup(&f).await;
}

/// An agent comment derives the on-behalf-of user from the creating run.
#[sqlx::test]
async fn agent_comment_derives_on_behalf_of_from_run(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_comment_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    let responsible_user_id = Uuid::new_v4().to_string();
    let run_id = seed_run(&f, Some(&responsible_user_id)).await;

    let (status, body) = send(
        &app,
        &AuthorizationActor::agent(f.agent_id, f.company_id, Some(run_id)),
        "POST",
        &format!("/issues/{}/comments", f.issue_id),
        Some(json!({
            "body": "Derived attribution",
            "actor_type": "agent",
            "actorRunId": run_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    let comment_id = Uuid::parse_str(body["comment"]["id"].as_str().expect("comment id")).expect("uuid");
    let (stored, _) = read_attribution(&f, comment_id).await;
    assert_eq!(
        stored,
        Some(responsible_user_id),
        "an agent comment must derive the responsible user from its run"
    );

    cleanup(&f).await;
}

/// A board comment never receives derived on-behalf-of attribution.
#[sqlx::test]
async fn board_comment_has_no_derived_on_behalf_of(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_comment_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    // An agent run with a responsible user exists, so a board comment that
    // ignored the "agents only" rule would pick up this user.
    let responsible_user_id = Uuid::new_v4().to_string();
    let _run_id = seed_run(&f, Some(&responsible_user_id)).await;
    let board_user_id = Uuid::new_v4();

    let (status, body) = send(
        &app,
        &board_actor(board_user_id, f.company_id),
        "POST",
        &format!("/issues/{}/comments", f.issue_id),
        Some(json!({
            "body": "Human comment",
            "actorType": "user",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    let comment_id = Uuid::parse_str(body["comment"]["id"].as_str().expect("comment id")).expect("uuid");
    let (stored, author_type) = read_attribution(&f, comment_id).await;
    assert_eq!(
        stored, None,
        "a human comment is not implicitly on behalf of the run's user"
    );
    assert_eq!(author_type.as_deref(), Some("user"));

    cleanup(&f).await;
}

/// A run without a responsible user yields no attribution rather than failing
/// the insert.
#[sqlx::test]
async fn run_without_responsible_user_yields_no_attribution(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_comment_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    let run_id = seed_run(&f, None).await;

    let (status, body) = send(
        &app,
        &AuthorizationActor::agent(f.agent_id, f.company_id, Some(run_id)),
        "POST",
        &format!("/issues/{}/comments", f.issue_id),
        Some(json!({
            "body": "Run with no responsible user",
            "actor_type": "agent",
            "actorRunId": run_id,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    let comment_id = Uuid::parse_str(body["comment"]["id"].as_str().expect("comment id")).expect("uuid");
    let (stored, _) = read_attribution(&f, comment_id).await;
    assert_eq!(stored, None);

    cleanup(&f).await;
}

/// `authorType` can override the actor type, and `sourceTrust` round-trips.
#[sqlx::test]
async fn author_type_override_and_source_trust_round_trip(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let app = issue_comment_routes().with_state(build_app_state(pool.clone()).await.expect("state"));

    let (status, body) = send(
        &app,
        &AuthorizationActor::agent(f.agent_id, f.company_id, None),
        "POST",
        &format!("/issues/{}/comments", f.issue_id),
        Some(json!({
            "body": "Attributed comment",
            "actor_type": "agent",
            "authorType": "user",
            "sourceTrust": { "level": "trusted", "source": "operator" },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");

    let comment_id = Uuid::parse_str(body["comment"]["id"].as_str().expect("comment id")).expect("uuid");
    let (_, author_type) = read_attribution(&f, comment_id).await;
    assert_eq!(
        author_type.as_deref(),
        Some("user"),
        "an explicit authorType must win over the actor type"
    );

    let source_trust: Option<Value> =
        sqlx::query_scalar("SELECT source_trust FROM issue_comments WHERE id = $1")
            .bind(comment_id)
            .fetch_one(&f.pool)
            .await
            .expect("read source trust");
    assert_eq!(
        source_trust,
        Some(json!({ "level": "trusted", "source": "operator" }))
    );

    cleanup(&f).await;
}
