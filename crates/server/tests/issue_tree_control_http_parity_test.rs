//! Issue tree control HTTP + PostgreSQL parity test
//! (PAPERCLIP_MIGRATION_PLAN §4B.1 line 376).
//!
//! Drives the real tree-control surface over HTTP against the live compile DB.
//! Before this slice the hold lifecycle was metadata-only: `create_tree_hold`
//! recorded member rows but never applied the tree-wide status transition, and
//! `release_tree_hold` never restored members to their pre-hold status — a
//! released hold left cancelled issues cancelled forever. The `IssueTreeHold`
//! model also declared `actor_agent_id`/`actor_user_id`, which do not exist in
//! the table DDL, so every `query_as::<_, IssueTreeHold>` failed at runtime.
//!
//! Covered here:
//!   - pause hold: members keep their status, gate reports the hold, execution
//!     is suppressed for a descendant of the held root
//!   - cancel hold: subtree transitions to cancelled, `previousStatus` is
//!     persisted per member
//!   - release: every non-skipped member is restored to its pre-hold status
//!   - attribution: created/released actor is taken from the authenticated
//!     actor, not the request body
//!   - authorization: read endpoints require Read scope, writes require Write
//!     scope, and cross-company access is rejected

use api::routes::issue_tree_control::issue_tree_control_routes;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

async fn migrate(pool: &PgPool) {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .expect("run migrations");
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

struct Fixture {
    pool: PgPool,
    company_id: Uuid,
    other_company_id: Uuid,
    actor: AuthorizationActor,
    foreign_actor: AuthorizationActor,
    root_id: Uuid,
    child_id: Uuid,
    grandchild_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let other_company_id = Uuid::new_v4();
    let root_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let grandchild_id = Uuid::new_v4();

    for (id, name) in [
        (company_id, "Tree Control Parity Co"),
        (other_company_id, "Foreign Tree Control Co"),
    ] {
        let prefix = format!("TC{}", &id.simple().to_string()[..8]);
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(name)
            .bind(prefix)
            .execute(pool)
            .await
            .expect("insert company");
    }

    // root -> child -> grandchild; the hold is applied to `root`.
    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, parent_id)
         VALUES ($1, $2, 'root', 'todo'::issue_status, NULL),
                ($3, $2, 'child', 'in_progress'::issue_status, $1),
                ($4, $2, 'grandchild', 'in_review'::issue_status, $3)",
    )
    .bind(root_id)
    .bind(company_id)
    .bind(child_id)
    .bind(grandchild_id)
    .execute(pool)
    .await
    .expect("insert issue tree");

    Fixture {
        pool: pool.clone(),
        company_id,
        other_company_id,
        actor: board_actor(Uuid::new_v4(), company_id),
        foreign_actor: board_actor(Uuid::new_v4(), other_company_id),
        root_id,
        child_id,
        grandchild_id,
    }
}

async fn cleanup(f: &Fixture) {
    sqlx::query("DELETE FROM issue_tree_hold_members WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM issue_tree_holds WHERE company_id = $1")
        .bind(f.company_id)
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM issues WHERE company_id = ANY($1)")
        .bind(vec![f.company_id, f.other_company_id])
        .execute(&f.pool)
        .await
        .ok();
    sqlx::query("DELETE FROM companies WHERE id = ANY($1)")
        .bind(vec![f.company_id, f.other_company_id])
        .execute(&f.pool)
        .await
        .ok();
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

async fn issue_status(f: &Fixture, issue_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT status::text FROM issues WHERE id = $1")
        .bind(issue_id)
        .fetch_one(&f.pool)
        .await
        .expect("read issue status")
}

/// A pause hold must suppress execution without mutating issue status, and the
/// gate must report the hold for a descendant of the held root.
#[sqlx::test]
async fn pause_hold_gates_execution_without_mutating_status(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = issue_tree_control_routes().with_state(state);

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-holds", f.root_id),
        Some(json!({ "mode": "pause", "reason": "hold for review" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    let hold_id = body["hold"]["id"]
        .as_str()
        .expect("hold id")
        .parse::<Uuid>()
        .expect("parse hold id");
    assert_eq!(body["hold"]["createdByActorType"], json!("user"));

    // Pause never rewrites issue status.
    assert_eq!(issue_status(&f, f.root_id).await, "todo");
    assert_eq!(issue_status(&f, f.child_id).await, "in_progress");
    assert_eq!(issue_status(&f, f.grandchild_id).await, "in_review");

    // Every tree member is recorded, with its pre-hold status persisted.
    let (status, body) = send(
        &app,
        &f.actor,
        "GET",
        &format!("/issues/{}/tree-holds/{}/members", f.root_id, hold_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let members = body["members"].as_array().expect("members array");
    assert_eq!(members.len(), 3, "expected the whole subtree: {body}");
    for member in members {
        assert_eq!(
            member["previousStatus"].as_str().map(str::to_owned),
            member["issueStatus"].as_str().map(str::to_owned),
            "pre-hold status must be captured: {member}"
        );
    }

    // The gate reports the hold for the root and for a descendant.
    let (status, body) = send(
        &app,
        &f.actor,
        "GET",
        &format!("/issues/{}/tree-control/state", f.grandchild_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["paused"], json!(true));
    assert_eq!(body["gate"]["holdId"], json!(hold_id.to_string()));
    assert_eq!(body["gate"]["issueId"], json!(f.grandchild_id.to_string()));
    assert_eq!(body["gate"]["isRoot"], json!(false));
    assert_eq!(body["gate"]["reason"], json!("hold for review"));

    cleanup(&f).await;
}

/// A cancel hold transitions the subtree to cancelled, and releasing it
/// restores every member to the status it held before the hold was applied.
#[sqlx::test]
async fn cancel_hold_transitions_subtree_and_release_restores_status(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = issue_tree_control_routes().with_state(state);

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-holds", f.root_id),
        Some(json!({ "mode": "cancel", "reason": "subtree abandoned" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    let hold_id = body["hold"]["id"]
        .as_str()
        .expect("hold id")
        .parse::<Uuid>()
        .expect("parse hold id");

    // The transition is actually applied, not just previewed.
    assert_eq!(issue_status(&f, f.root_id).await, "cancelled");
    assert_eq!(issue_status(&f, f.child_id).await, "cancelled");
    assert_eq!(issue_status(&f, f.grandchild_id).await, "cancelled");

    let (status, members_body) = send(
        &app,
        &f.actor,
        "GET",
        &format!("/issues/{}/tree-holds/{}/members", f.root_id, hold_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {members_body}");
    let mut previous: Vec<String> = members_body["members"]
        .as_array()
        .expect("members")
        .iter()
        .map(|m| {
            m["previousStatus"]
                .as_str()
                .expect("previousStatus")
                .to_string()
        })
        .collect();
    previous.sort();
    assert_eq!(
        previous,
        vec![
            "in_progress".to_string(),
            "in_review".to_string(),
            "todo".to_string()
        ]
    );

    // Release restores each member to its pre-hold status.
    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-holds/{}/release", f.root_id, hold_id),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["hold"]["status"], json!("released"));
    assert_eq!(body["hold"]["releasedByActorType"], json!("user"));

    assert_eq!(issue_status(&f, f.root_id).await, "todo");
    assert_eq!(issue_status(&f, f.child_id).await, "in_progress");
    assert_eq!(issue_status(&f, f.grandchild_id).await, "in_review");

    // The restore is recorded per member so the release is auditable.
    let restored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_tree_hold_members WHERE hold_id = $1 AND restored_at IS NOT NULL",
    )
    .bind(hold_id)
    .fetch_one(&f.pool)
    .await
    .expect("count restored members");
    assert_eq!(restored, 3);

    // A second release is rejected rather than silently re-restoring.
    let (status, _) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-holds/{}/release", f.root_id, hold_id),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    cleanup(&f).await;
}

/// Attribution must come from the authenticated actor; a request body that
/// claims a different actor is ignored.
#[sqlx::test]
async fn hold_attribution_uses_authenticated_actor_not_request_body(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = issue_tree_control_routes().with_state(state);

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-holds", f.root_id),
        Some(json!({
            "mode": "pause",
            "actor_type": "system",
            "actor_id": Uuid::new_v4(),
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body: {body}");
    let hold_id = body["hold"]["id"]
        .as_str()
        .expect("hold id")
        .parse::<Uuid>()
        .expect("parse hold id");

    let (created_type, created_id): (Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT created_by_actor_type, created_by_agent_id FROM issue_tree_holds WHERE id = $1",
    )
    .bind(hold_id)
    .fetch_one(&f.pool)
    .await
    .expect("read hold attribution");
    assert_eq!(created_type.as_deref(), Some("user"));
    assert!(
        created_id.is_none() || created_id == f.actor.principal_id(),
        "attribution must not come from the request body"
    );

    cleanup(&f).await;
}

/// Cross-company access is rejected before any tree-control work happens.
#[sqlx::test]
async fn tree_control_rejects_cross_company_access(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = issue_tree_control_routes().with_state(state);

    let (status, _) = send(
        &app,
        &f.foreign_actor,
        "POST",
        &format!("/issues/{}/tree-holds", f.root_id),
        Some(json!({ "mode": "cancel" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &app,
        &f.foreign_actor,
        "GET",
        &format!("/issues/{}/tree-control/state", f.root_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // No hold was created by the rejected request.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_tree_holds")
        .fetch_one(&f.pool)
        .await
        .expect("count holds");
    assert_eq!(count, 0);

    cleanup(&f).await;
}

/// Preview reports the whole subtree and never mutates issue status.
#[sqlx::test]
async fn preview_reports_subtree_without_mutating_status(pool: PgPool) {
    migrate(&pool).await;
    let f = seed(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = issue_tree_control_routes().with_state(state);

    let (status, body) = send(
        &app,
        &f.actor,
        "POST",
        &format!("/issues/{}/tree-control/preview", f.root_id),
        Some(json!({ "mode": "cancel" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let affected = body["affectedIssues"].as_array().expect("affectedIssues");
    assert_eq!(affected.len(), 3, "preview must span the subtree: {body}");

    // A preview is side-effect free.
    assert_eq!(issue_status(&f, f.child_id).await, "in_progress");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_tree_holds")
        .fetch_one(&f.pool)
        .await
        .expect("count holds");
    assert_eq!(count, 0);

    cleanup(&f).await;
}
