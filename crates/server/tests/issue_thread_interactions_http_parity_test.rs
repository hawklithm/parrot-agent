//! HTTP and PostgreSQL parity coverage for issue thread interactions.
//!
//! These tests exercise the route, authorization, service transaction and
//! canonical interaction schema together instead of testing the service in
//! isolation.

use api::routes::interactions::interaction_routes;
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
    let prefix = format!("IT{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("Interaction parity")
        .bind(prefix)
        .execute(pool)
        .await
        .expect("insert company");
    sqlx::query("INSERT INTO issues (id, company_id, title, status) VALUES ($1, $2, $3, 'todo')")
        .bind(issue_id)
        .bind(company_id)
        .bind("Interaction parity issue")
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

async fn app(pool: PgPool) -> Router {
    interaction_routes()
        .with_state(build_app_state(pool).await.expect("build app state"))
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

fn interaction_id(value: &Value) -> Uuid {
    value["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .expect("interaction id")
}

#[sqlx::test]
async fn create_list_get_and_idempotency_use_canonical_route_contract(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);
    let request = json!({
        "kind": "request_confirmation",
        "payload": {"version": 1, "title": "Deploy?"},
        "resolverPolicy": "anyone",
        "continuationPolicy": "none",
        "idempotencyKey": "deploy-1"
    });

    let (status, created) = send(&app, &actor, "POST", &uri, Some(request.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = interaction_id(&created);
    assert_eq!(created["resolverPolicyProvenance"], "explicit");
    assert_eq!(created["effectiveResolverPolicySource"], "requested");

    let (status, replay) = send(&app, &actor, "POST", &uri, Some(request)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(interaction_id(&replay), id);

    let conflicting = json!({
        "kind": "request_confirmation",
        "payload": {"version": 1, "title": "Different"},
        "resolverPolicy": "anyone",
        "continuationPolicy": "none",
        "idempotencyKey": "deploy-1"
    });
    let (status, _) = send(&app, &actor, "POST", &uri, Some(conflicting)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, list) = send(&app, &actor, "GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().expect("interaction list").len(), 1);

    let (status, fetched) = send(
        &app,
        &actor,
        "GET",
        &format!("{uri}/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(interaction_id(&fetched), id);
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn respond_normalizes_answers_and_rejects_invalid_payloads(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);
    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "ask_user_questions",
            "payload": {
                "version": 1,
                "questions": [{
                    "id": "scope",
                    "prompt": "Scope?",
                    "selectionMode": "single",
                    "required": true,
                    "options": [{"id": "small", "label": "Small"}, {"id": "large", "label": "Large"}]
                }]
            },
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = interaction_id(&created);
    let response_uri = format!("{uri}/{id}/respond");

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &response_uri,
        Some(json!({"answers": [{"questionId": "scope", "optionIds": ["small", "large"]}]})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, answered) = send(
        &app,
        &actor,
        "POST",
        &response_uri,
        Some(json!({
            "answers": [{"questionId": "scope", "optionIds": ["small"], "otherText": "  extra context  "}],
            "summaryMarkdown": "Selected small"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(answered["status"], "answered");
    assert_eq!(answered["result"]["answers"][0]["otherText"], "extra context");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn checkbox_accept_and_item_verdicts_follow_paperclip_values(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);

    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_checkbox_confirmation",
            "payload": {
                "version": 1,
                "options": [{"id": "one", "label": "One"}, {"id": "two", "label": "Two"}],
                "minSelected": 1,
                "maxSelected": 1
            },
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let checkbox_id = interaction_id(&created);
    let (status, accepted) = send(
        &app,
        &actor,
        "POST",
        &format!("{uri}/{checkbox_id}/accept"),
        Some(json!({"selectedOptionIds": ["two"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["interaction"]["status"], "accepted");
    assert_eq!(accepted["interaction"]["result"]["selectedOptionIds"][0], "two");

    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_item_verdicts",
            "payload": {"version": 1, "items": [{"id": "api"}, {"id": "docs"}]},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let verdict_id = interaction_id(&created);

    let (status, partial) = send(
        &app,
        &actor,
        "POST",
        &format!("{uri}/{verdict_id}/verdicts"),
        Some(json!({"verdicts": [{"itemId": "api", "verdict": "approve"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partial["status"], "pending");

    let (status, complete) = send(
        &app,
        &actor,
        "POST",
        &format!("{uri}/{verdict_id}/verdicts"),
        Some(json!({"verdicts": [{"itemId": "docs", "verdict": "reject", "reason": "Needs examples"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(complete["status"], "answered");
    assert_eq!(complete["result"]["items"][1]["verdict"], "reject");
    assert!(complete["result"]["items"][1]["resolvedAt"].is_string());
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn resolution_queues_one_assignee_continuation_wakeup_and_replay_is_quiet(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let agent_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type)
         VALUES ($1, $2, 'Continuation agent', 'idle', 'process')",
    )
    .bind(agent_id)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("insert continuation agent");
    sqlx::query(
        "UPDATE issues
         SET assignee_agent_id = $2, assignee_user_id = NULL
         WHERE id = $1",
    )
    .bind(fixture.issue_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .expect("assign continuation issue");
    // Keep the wakeup in the queued state so this test observes the durable
    // continuation request without starting a real adapter process.
    sqlx::query(
        "INSERT INTO heartbeat_runs
         (id, company_id, agent_id, status, context_snapshot)
         VALUES ($1, $2, $3, 'running', $4)",
    )
    .bind(run_id)
    .bind(fixture.company_id)
    .bind(agent_id)
    .bind(json!({ "issueId": fixture.issue_id }))
    .execute(&pool)
    .await
    .expect("insert active continuation run");

    let actor = board_actor(&fixture);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);
    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_item_verdicts",
            "payload": {"version": 1, "items": [{"id": "api"}, {"id": "docs"}]},
            "resolverPolicy": "anyone",
            "continuationPolicy": "wake_assignee"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let interaction_id = interaction_id(&created);
    let verdict_uri = format!("{uri}/{interaction_id}/verdicts");

    let (status, partial) = send(
        &app,
        &actor,
        "POST",
        &verdict_uri,
        Some(json!({"verdicts": [{"itemId": "api", "verdict": "approve"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partial["status"], "pending");

    let wake_key_prefix = format!(
        "request_item_verdicts:{}:{interaction_id}:",
        fixture.issue_id
    );
    let wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
         WHERE company_id = $1 AND agent_id = $2 AND idempotency_key LIKE $3",
    )
    .bind(fixture.company_id)
    .bind(agent_id)
    .bind(format!("{wake_key_prefix}%"))
    .fetch_one(&pool)
    .await
    .expect("count continuation wake");
    assert_eq!(wake_count, 1, "a newly resolved item must wake its assignee");

    let (status, replay) = send(
        &app,
        &actor,
        "POST",
        &verdict_uri,
        Some(json!({"verdicts": [{"itemId": "api", "verdict": "approve"}]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["status"], "pending");

    let replay_wake_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_wakeup_requests
         WHERE company_id = $1 AND agent_id = $2 AND idempotency_key LIKE $3",
    )
    .bind(fixture.company_id)
    .bind(agent_id)
    .bind(format!("{wake_key_prefix}%"))
    .fetch_one(&pool)
    .await
    .expect("count replay continuation wake");
    assert_eq!(replay_wake_count, 1, "a verdict replay must not wake again");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn board_can_cancel_questions_but_agent_cannot(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, 'idle', 'process')")
        .bind(agent_id)
        .bind(fixture.company_id)
        .bind("Interaction agent")
        .execute(&pool)
        .await
        .expect("insert agent");
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot) VALUES ($1, $2, $3, 'running', $4)")
        .bind(run_id)
        .bind(fixture.company_id)
        .bind(agent_id)
        .bind(json!({"issueId": fixture.issue_id}))
        .execute(&pool)
        .await
        .expect("insert heartbeat run");

    let board = board_actor(&fixture);
    let agent = AuthorizationActor::agent(agent_id, fixture.company_id, Some(run_id));
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);
    let (status, created) = send(
        &app,
        &board,
        "POST",
        &uri,
        Some(json!({
            "kind": "ask_user_questions",
            "payload": {"version": 1, "questions": [{"id": "q", "prompt": "Continue?", "selectionMode": "single", "options": []}]},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none",
            "addresseeAgentId": agent_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = interaction_id(&created);

    let (status, _) = send(
        &app,
        &agent,
        "POST",
        &format!("{uri}/{id}/cancel"),
        Some(json!({"reason": "agent cannot cancel"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, cancelled) = send(
        &app,
        &board,
        "POST",
        &format!("{uri}/{id}/cancel"),
        Some(json!({"reason": "No longer needed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cancelled["status"], "cancelled");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn create_rejects_addressee_with_invalid_org_chain_over_http(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let manager_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type)
         VALUES ($1, $2, 'Terminated manager', 'terminated', 'process')",
    )
    .bind(manager_id)
    .bind(fixture.company_id)
    .execute(&pool)
    .await
    .expect("insert terminated manager");
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, status, adapter_type, reports_to)
         VALUES ($1, $2, 'Child agent', 'idle', 'process', $3)",
    )
    .bind(child_id)
    .bind(fixture.company_id)
    .bind(manager_id)
    .execute(&pool)
    .await
    .expect("insert child agent");

    let app = app(pool.clone()).await;
    let (status, body) = send(
        &app,
        &board_actor(&fixture),
        "POST",
        &format!("/issues/{}/interactions", fixture.issue_id),
        Some(json!({
            "kind": "question",
            "payload": {},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none",
            "addresseeAgentId": child_id
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].as_str().unwrap_or_default().contains("manager_terminated"));
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn governed_tool_action_is_human_only_for_agent_resolution(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name, status, adapter_type) VALUES ($1, $2, $3, 'idle', 'process')")
        .bind(agent_id)
        .bind(fixture.company_id)
        .bind("Governance test agent")
        .execute(&pool)
        .await
        .expect("insert governance agent");
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO heartbeat_runs (id, company_id, agent_id, status, context_snapshot) VALUES ($1, $2, $3, 'running', $4)")
        .bind(run_id)
        .bind(fixture.company_id)
        .bind(agent_id)
        .bind(json!({ "issueId": fixture.issue_id }))
        .execute(&pool)
        .await
        .expect("insert governance run");

    let board = board_actor(&fixture);
    let agent = AuthorizationActor::agent(agent_id, fixture.company_id, Some(run_id));
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);
    let (status, created) = send(
        &app,
        &board,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_confirmation",
            "payload": {
                "version": 1,
                "title": "Run deploy",
                "toolAction": { "name": "deploy" }
            },
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["requestedResolverPolicy"], "anyone");
    assert_eq!(created["effectiveResolverPolicy"], "human_only");
    assert_eq!(created["resolverPolicyProvenance"], "explicit");
    assert_eq!(created["effectiveResolverPolicySource"], "governed_action");
    let id = interaction_id(&created);

    let (status, body) = send(
        &app,
        &agent,
        "POST",
        &format!("{uri}/{id}/accept"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["error"].as_str().unwrap_or_default().contains("human-only"));

    let (status, accepted) = send(
        &app,
        &board,
        "POST",
        &format!("{uri}/{id}/accept"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["interaction"]["status"], "accepted");
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn concurrent_accepts_have_one_winner_and_preserve_resolution_attribution(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let creator = board_actor(&fixture);
    let resolver_one = AuthorizationActor::board(Uuid::new_v4(), fixture.company_id);
    let resolver_two = AuthorizationActor::board(Uuid::new_v4(), fixture.company_id);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);

    let (status, created) = send(
        &app,
        &creator,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_confirmation",
            "payload": {"version": 1, "title": "Resolve once"},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let interaction_id = interaction_id(&created);
    let accept_uri = format!("{uri}/{interaction_id}/accept");

    let ((first_status, first_body), (second_status, second_body)) = tokio::join!(
        send(&app, &resolver_one, "POST", &accept_uri, Some(json!({}))),
        send(&app, &resolver_two, "POST", &accept_uri, Some(json!({}))),
    );
    let statuses = [first_status, second_status];
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::OK).count(), 1);
    assert_eq!(statuses.iter().filter(|status| **status == StatusCode::CONFLICT).count(), 1);
    let winner = if first_status == StatusCode::OK { first_body } else { second_body };
    assert_eq!(winner["interaction"]["status"], "accepted");

    let row: (String, String) = sqlx::query_as(
        "SELECT status::text, resolved_by_user_id FROM issue_thread_interactions WHERE id = $1",
    )
    .bind(interaction_id)
    .fetch_one(&pool)
    .await
    .expect("read resolved interaction");
    assert_eq!(row.0, "accepted");
    let resolver_one_id = match resolver_one {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => unreachable!(),
    };
    let resolver_two_id = match resolver_two {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => unreachable!(),
    };
    assert!([resolver_one_id.to_string(), resolver_two_id.to_string()].contains(&row.1));
    cleanup(&fixture).await;
}

#[sqlx::test]
async fn closed_issues_cannot_create_or_resolve_interactions(pool: PgPool) {
    migrate(&pool).await;
    let fixture = seed(&pool).await;
    let actor = board_actor(&fixture);
    let app = app(pool.clone()).await;
    let uri = format!("/issues/{}/interactions", fixture.issue_id);

    sqlx::query("UPDATE issues SET status = 'done' WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&pool)
        .await
        .expect("close issue");
    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_confirmation",
            "payload": {"version": 1, "title": "Closed"},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    sqlx::query("UPDATE issues SET status = 'todo' WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&pool)
        .await
        .expect("reopen issue");
    let (status, created) = send(
        &app,
        &actor,
        "POST",
        &uri,
        Some(json!({
            "kind": "request_confirmation",
            "payload": {"version": 1, "title": "Close before accept"},
            "resolverPolicy": "anyone",
            "continuationPolicy": "none"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let interaction_id = interaction_id(&created);
    sqlx::query("UPDATE issues SET status = 'done' WHERE id = $1")
        .bind(fixture.issue_id)
        .execute(&pool)
        .await
        .expect("close issue before accept");

    let (status, _) = send(
        &app,
        &actor,
        "POST",
        &format!("{uri}/{interaction_id}/accept"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    cleanup(&fixture).await;
}
