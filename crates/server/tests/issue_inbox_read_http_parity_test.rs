//! HTTP parity tests for the issue read-state and inbox-archive endpoints.
//!
//! Covers, against Paperclip `server/src/routes/issues.ts`:
//! - POST   /issues/:id/read            (AR1–AR3)  `res.json(readState)`
//! - DELETE /issues/:id/read            (AR4–AR5)  `res.json({id, removed})`
//! - POST   /issues/:id/inbox-archive   (AR6–AR8)  `res.json(archiveState)`
//! - DELETE /issues/:id/inbox-archive   (AR9–AR11) `res.json(removed ?? {ok,userId})`
//!
//! These four routes previously answered `204 No Content`, which cannot carry
//! the `removed` flag `DELETE .../read` reports nor the archive row the frontend
//! (`parrot-web-ui/src/api/issues.ts:154-159`) parses.
//!
//! Reference lines:
//!   issues.ts:7824  POST   /issues/:id/read
//!   issues.ts:7853  DELETE /issues/:id/read
//!   issues.ts:7929  POST   /issues/:id/inbox-archive
//!   issues.ts:7960  DELETE /issues/:id/inbox-archive
//!   issues.ts:7884  resolveInboxArchiveTarget
//!   authorization.ts:1931-2066  the `inbox:manage` decision arm
//!
//! Run with a live database:
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test issue_inbox_read_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::issues::issue_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

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
    agent_id: Uuid,
}

async fn seed(pool: &PgPool) -> Fixture {
    let company_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let issue_id = Uuid::new_v4();
    let prefix = format!("AR{}", &company_id.simple().to_string()[..8]);

    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(company_id)
        .bind("InboxReadParityTest")
        .bind(&prefix)
        .execute(pool)
        .await
        .expect("insert company");

    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("ar{}@test.com", user_id))
        .bind("AR User")
        .execute(pool)
        .await
        .expect("insert auth_user");

    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, 'user', $2, 'owner', 'active')",
    )
    .bind(company_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert membership");

    // NOTE: do NOT set `reports_to` here — it is an agent self-FK
    // (`agents_reports_to_fkey` → `agents(id)`), not a user reference. The
    // agent actor below supplies its responsible user directly.
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, adapter_type, status) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent_id)
    .bind(company_id)
    .bind("AR Agent")
    .bind("http")
    .bind("running")
    .execute(pool)
    .await
    .expect("insert agent");

    sqlx::query(
        "INSERT INTO issues (id, company_id, title, status, priority) \
         VALUES ($1, $2, $3, $4::issue_status, $5::issue_priority)",
    )
    .bind(issue_id)
    .bind(company_id)
    .bind("Inbox read parity issue")
    .bind("todo")
    .bind("medium")
    .execute(pool)
    .await
    .expect("insert issue");

    Fixture {
        pool: pool.clone(),
        company_id,
        user_id,
        issue_id,
        agent_id,
    }
}

fn owner_actor(f: &Fixture) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        f.user_id,
        f.company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            f.company_id,
            PrincipalType::User,
            f.user_id,
            MembershipRole::Owner,
        )],
        true,
    )
}

/// The agent actor mirrors `auth_middleware_fn::resolve_agent_key`: the
/// responsible user is stored in both `responsible_user_id` and
/// `on_behalf_of_user_id`.
fn agent_actor(f: &Fixture) -> AuthorizationActor {
    AuthorizationActor::Agent {
        agent_id: f.agent_id,
        company_id: f.company_id,
        run_id: None,
        source: ActorSource::AgentKey,
        key_id: None,
        key_scope: None,
        responsible_user_id: Some(f.user_id),
        on_behalf_of_user_id: Some(f.user_id),
        on_behalf_of_memberships: vec![CompanyMembership::new(
            f.company_id,
            PrincipalType::User,
            f.user_id,
            MembershipRole::Owner,
        )],
    }
}

/// A board actor from a *different* company, for cross-tenant checks.
fn foreign_actor(f: &Fixture) -> AuthorizationActor {
    let other_company = Uuid::new_v4();
    let other_user = Uuid::new_v4();
    AuthorizationActor::board_with_source(
        other_user,
        other_company,
        ActorSource::Session,
        vec![CompanyMembership::new(
            other_company,
            PrincipalType::User,
            other_user,
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

/// Send a raw (possibly malformed) body without JSON serialization.
async fn send_raw(
    app: &Router,
    actor: &AuthorizationActor,
    method: &str,
    uri: &str,
    raw: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(raw.to_string()))
        .expect("build request");
    let mut req = req;
    req.extensions_mut().insert(actor.clone());
    let resp = app.clone().oneshot(req).await.expect("dispatch request");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    (status, bytes.to_vec())
}

fn parse_json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or_else(|error| {
        panic!(
            "response body must be JSON, got {:?}: {error}",
            String::from_utf8_lossy(bytes)
        )
    })
}

async fn app_for(pool: &PgPool) -> Router {
    let state = build_app_state(pool.clone())
        .await
        .expect("build_app_state");
    issue_routes().with_state(state)
}

// ---------------------------------------------------------------------------
// AR1–AR3: POST /issues/:id/read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar1_mark_read_returns_read_state_json() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    let (status, body) = send(
        &app,
        &owner_actor(&f),
        "POST",
        &format!("/issues/{}/read", f.issue_id),
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Paperclip answers 200 + readState");
    let json = parse_json(&body);
    assert_eq!(json["issueId"], f.issue_id.to_string());
    assert_eq!(json["companyId"], f.company_id.to_string());
    assert_eq!(json["userId"], f.user_id.to_string());
    assert!(
        json["lastReadAt"].is_string(),
        "readState must carry lastReadAt: {json}"
    );

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_read_status WHERE company_id = $1 AND issue_id = $2 AND user_id = $3",
    )
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(f.user_id)
    .fetch_one(&f.pool)
    .await
    .expect("count read rows");
    assert_eq!(stored, 1, "marking read must persist exactly one row");
}

#[tokio::test]
async fn ar2_mark_read_is_idempotent_and_refreshes_timestamp() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&f);
    let uri = format!("/issues/{}/read", f.issue_id);

    let (first_status, first_body) = send(&app, &actor, "POST", &uri, Some(json!({}))).await;
    assert_eq!(first_status, StatusCode::OK);
    let first = parse_json(&first_body);

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let (second_status, second_body) = send(&app, &actor, "POST", &uri, Some(json!({}))).await;
    assert_eq!(second_status, StatusCode::OK);
    let second = parse_json(&second_body);

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issue_read_status WHERE issue_id = $1 AND user_id = $2",
    )
    .bind(f.issue_id)
    .bind(f.user_id)
    .fetch_one(&f.pool)
    .await
    .expect("count read rows");
    assert_eq!(stored, 1, "upsert must not duplicate the read row");

    let first_at = first["lastReadAt"].as_str().expect("first lastReadAt");
    let second_at = second["lastReadAt"].as_str().expect("second lastReadAt");
    assert!(
        second_at >= first_at,
        "second read must not move lastReadAt backwards: {first_at} -> {second_at}"
    );
}

/// Paperclip gates both read routes on a board actor
/// (`routes/issues.ts:7837`, `:7866`).
#[tokio::test]
async fn ar3_mark_read_rejects_agent_and_foreign_actor() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let uri = format!("/issues/{}/read", f.issue_id);

    let (agent_status, _) = send(&app, &agent_actor(&f), "POST", &uri, Some(json!({}))).await;
    assert_eq!(
        agent_status,
        StatusCode::FORBIDDEN,
        "read state is board-only in Paperclip"
    );

    let (foreign_status, _) = send(&app, &foreign_actor(&f), "POST", &uri, Some(json!({}))).await;
    assert_eq!(
        foreign_status,
        StatusCode::NOT_FOUND,
        "cross-tenant access to a by-id resource is a 404, never a 403"
    );

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_read_status WHERE issue_id = $1")
        .bind(f.issue_id)
        .fetch_one(&f.pool)
        .await
        .expect("count read rows");
    assert_eq!(rows, 0, "a rejected write must not create a read row");
}

// ---------------------------------------------------------------------------
// AR4–AR5: DELETE /issues/:id/read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar4_unmark_read_reports_removed_boolean() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&f);
    let uri = format!("/issues/{}/read", f.issue_id);

    send(&app, &actor, "POST", &uri, Some(json!({}))).await;

    let (status, body) = send(&app, &actor, "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "Paperclip answers 200, not 204");
    let json = parse_json(&body);
    assert_eq!(json["id"], f.issue_id.to_string());
    assert_eq!(
        json["removed"], true,
        "removing an existing read state reports removed=true"
    );

    // Second delete: nothing left to remove.
    let (again_status, again_body) = send(&app, &actor, "DELETE", &uri, None).await;
    assert_eq!(again_status, StatusCode::OK);
    let again = parse_json(&again_body);
    assert_eq!(
        again["removed"], false,
        "a no-op delete must report removed=false, which 204 could not express"
    );
}

#[tokio::test]
async fn ar5_unmark_read_is_cross_tenant_safe() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let uri = format!("/issues/{}/read", f.issue_id);

    // Seed a read state as the owner, then try to delete it from another tenant.
    send(&app, &owner_actor(&f), "POST", &uri, Some(json!({}))).await;

    let (status, _) = send(&app, &foreign_actor(&f), "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "existence must not leak");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issue_read_status WHERE issue_id = $1")
        .bind(f.issue_id)
        .fetch_one(&f.pool)
        .await
        .expect("count read rows");
    assert_eq!(rows, 1, "a forbidden delete must not remove the row");
}

// ---------------------------------------------------------------------------
// AR6–AR8: POST /issues/:id/inbox-archive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar6_archive_returns_row_and_persists_attribution() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    let (status, body) = send(
        &app,
        &owner_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Paperclip answers 200 + archiveState");
    let json = parse_json(&body);
    assert_eq!(json["id"], f.issue_id.to_string());
    assert_eq!(json["userId"], f.user_id.to_string());
    assert!(
        json["archivedAt"].is_string(),
        "archiveState must carry archivedAt: {json}"
    );

    let (actor_type, agent_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT archived_by_actor_type, archived_by_agent_id FROM issue_inbox_archives \
         WHERE company_id = $1 AND issue_id = $2 AND user_id = $3",
    )
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(f.user_id)
    .fetch_one(&f.pool)
    .await
    .expect("load archive row");
    assert_eq!(actor_type, "user");
    assert_eq!(agent_id, None, "a board archive carries no agent attribution");
}

#[tokio::test]
async fn ar7_archive_by_agent_records_agent_attribution() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    // The agent acts for its responsible user, which the default-open policy
    // permits (`authorization.ts:2058`).
    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "responsible-user archive must be allowed: {}",
        String::from_utf8_lossy(&body)
    );

    let json = parse_json(&body);
    assert_eq!(
        json["userId"], f.user_id.to_string(),
        "the target is the responsible user, not the agent"
    );

    let (actor_type, agent_id): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT archived_by_actor_type, archived_by_agent_id FROM issue_inbox_archives \
         WHERE company_id = $1 AND issue_id = $2 AND user_id = $3",
    )
    .bind(f.company_id)
    .bind(f.issue_id)
    .bind(f.user_id)
    .fetch_one(&f.pool)
    .await
    .expect("load archive row");
    assert_eq!(actor_type, "agent");
    assert_eq!(agent_id, Some(f.agent_id));
}

/// `inbox_management_disabled` — the target user turned agent inbox management
/// off, so the archive must be refused with that code.
#[tokio::test]
async fn ar8_archive_by_agent_is_denied_when_policy_is_disabled() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    sqlx::query(
        "INSERT INTO user_inbox_agent_policies (company_id, user_id, mode, allowed_agent_ids) \
         VALUES ($1, $2, 'disabled', '{}')",
    )
    .bind(f.company_id)
    .bind(f.user_id)
    .execute(&f.pool)
    .await
    .expect("insert disabled policy");

    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let json = parse_json(&body);
    assert_eq!(
        json["code"], "inbox_management_disabled",
        "clients branch on the denial code: {json}"
    );

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_inbox_archives WHERE issue_id = $1")
            .bind(f.issue_id)
            .fetch_one(&f.pool)
            .await
            .expect("count archive rows");
    assert_eq!(rows, 0, "a denied archive must not write a row");
}

/// An agent naming a user it is neither responsible for nor granted is refused
/// with `inbox_cross_user_grant_required`, not a silent success.
#[tokio::test]
async fn ar8b_archive_by_agent_for_ungranted_other_user_is_denied() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    // A second active user in the same company, with no policy row.
    let other_user = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(other_user)
        .bind(format!("other{}@test.com", other_user))
        .bind("Other User")
        .execute(&f.pool)
        .await
        .expect("insert other user");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, 'user', $2, 'operator', 'active')",
    )
    .bind(f.company_id)
    .bind(other_user)
    .execute(&f.pool)
    .await
    .expect("insert other membership");

    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({ "userId": other_user.to_string() })),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let json = parse_json(&body);
    assert_eq!(
        json["code"], "inbox_cross_user_grant_required",
        "an absent policy row is not a company-wide cross-user grant: {json}"
    );
}

/// A scoped `inbox:manage` grant is an administrative override, and it outranks
/// a `disabled` target policy (`authorization.ts:1970-1996`).
#[tokio::test]
async fn ar8c_archive_by_agent_honours_scoped_grant_over_disabled_policy() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    let other_user = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(other_user)
        .bind(format!("granted{}@test.com", other_user))
        .bind("Granted User")
        .execute(&f.pool)
        .await
        .expect("insert granted user");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, 'user', $2, 'operator', 'active')",
    )
    .bind(f.company_id)
    .bind(other_user)
    .execute(&f.pool)
    .await
    .expect("insert granted membership");
    sqlx::query(
        "INSERT INTO user_inbox_agent_policies (company_id, user_id, mode, allowed_agent_ids) \
         VALUES ($1, $2, 'disabled', '{}')",
    )
    .bind(f.company_id)
    .bind(other_user)
    .execute(&f.pool)
    .await
    .expect("insert disabled policy");

    sqlx::query(
        "INSERT INTO principal_permission_grants \
            (id, company_id, principal_type, principal_id, permission_key, scope, granted_by_user_id) \
         VALUES ($1, $2, 'agent', $3, 'inbox:manage', $4::jsonb, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(json!({ "userId": other_user.to_string() }).to_string())
    .bind(f.user_id)
    .execute(&f.pool)
    .await
    .expect("insert inbox:manage grant");

    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({ "userId": other_user.to_string() })),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a scoped grant overrides the target policy: {}",
        String::from_utf8_lossy(&body)
    );
    let json = parse_json(&body);
    assert_eq!(json["userId"], other_user.to_string());
}

/// A grant that does not list the requested user must not authorize it.
#[tokio::test]
async fn ar8d_archive_by_agent_rejects_out_of_scope_grant() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    let other_user = Uuid::new_v4();
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, $3)")
        .bind(other_user)
        .bind(format!("scoped{}@test.com", other_user))
        .bind("Scoped User")
        .execute(&f.pool)
        .await
        .expect("insert scoped user");
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, \
         membership_role, status) VALUES ($1, 'user', $2, 'operator', 'active')",
    )
    .bind(f.company_id)
    .bind(other_user)
    .execute(&f.pool)
    .await
    .expect("insert scoped membership");

    // Grant scoped to a *different* user id.
    sqlx::query(
        "INSERT INTO principal_permission_grants \
            (id, company_id, principal_type, principal_id, permission_key, scope, granted_by_user_id) \
         VALUES ($1, $2, 'agent', $3, 'inbox:manage', $4::jsonb, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(f.company_id)
    .bind(f.agent_id)
    .bind(json!({ "userId": Uuid::new_v4().to_string() }).to_string())
    .bind(f.user_id)
    .execute(&f.pool)
    .await
    .expect("insert out-of-scope grant");

    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({ "userId": other_user.to_string() })),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let json = parse_json(&body);
    assert_eq!(json["code"], "inbox_cross_user_grant_required");
    assert_eq!(json["reason"], "deny_scope");
}

/// `inboxArchiveBodySchema` is `.strict()`: unknown keys are rejected.
#[tokio::test]
async fn ar8e_archive_rejects_unknown_body_field() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let uri = format!("/issues/{}/inbox-archive", f.issue_id);

    let (status, _) = send(
        &app,
        &owner_actor(&f),
        "POST",
        &uri,
        Some(json!({ "unknownField": true })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unknown key fails Zod's .strict() parse"
    );

    let (malformed_status, _) = send_raw(&app, &owner_actor(&f), "POST", &uri, "{not json").await;
    assert_eq!(
        malformed_status,
        StatusCode::BAD_REQUEST,
        "malformed JSON must be rejected, not read as an empty body"
    );
}

// ---------------------------------------------------------------------------
// AR9–AR11: DELETE /issues/:id/inbox-archive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar9_unarchive_returns_removed_then_ok_fallback() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&f);
    let uri = format!("/issues/{}/inbox-archive", f.issue_id);

    send(&app, &actor, "POST", &uri, Some(json!({}))).await;

    let (status, body) = send(&app, &actor, "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::OK, "Paperclip answers 200, not 204");
    let json = parse_json(&body);
    assert_eq!(
        json["removed"], true,
        "an existing archive is reported as removed: {json}"
    );

    // Paperclip's fallback keeps the response JSON-shaped when no row existed.
    let (again_status, again_body) = send(&app, &actor, "DELETE", &uri, None).await;
    assert_eq!(again_status, StatusCode::OK);
    let again = parse_json(&again_body);
    assert_eq!(again["ok"], true);
    assert_eq!(again["userId"], f.user_id.to_string());
    assert!(
        again["removed"].is_null(),
        "the fallback shape omits removed: {again}"
    );
}

#[tokio::test]
async fn ar10_archive_and_unarchive_round_trip_clears_row() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let actor = owner_actor(&f);
    let uri = format!("/issues/{}/inbox-archive", f.issue_id);

    send(&app, &actor, "POST", &uri, Some(json!({}))).await;
    let after_archive: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_inbox_archives WHERE issue_id = $1")
            .bind(f.issue_id)
            .fetch_one(&f.pool)
            .await
            .expect("count archive rows");
    assert_eq!(after_archive, 1);

    send(&app, &actor, "DELETE", &uri, None).await;
    let after_unarchive: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_inbox_archives WHERE issue_id = $1")
            .bind(f.issue_id)
            .fetch_one(&f.pool)
            .await
            .expect("count archive rows");
    assert_eq!(after_unarchive, 0, "unarchive must delete the row");

    // Re-archiving after an unarchive is the normal inbox flow and must succeed.
    let (restatus, _) = send(&app, &actor, "POST", &uri, Some(json!({}))).await;
    assert_eq!(restatus, StatusCode::OK);
}

#[tokio::test]
async fn ar11_unarchive_is_cross_tenant_safe() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;
    let uri = format!("/issues/{}/inbox-archive", f.issue_id);

    send(&app, &owner_actor(&f), "POST", &uri, Some(json!({}))).await;

    let (status, _) = send(&app, &foreign_actor(&f), "DELETE", &uri, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM issue_inbox_archives WHERE issue_id = $1")
            .bind(f.issue_id)
            .fetch_one(&f.pool)
            .await
            .expect("count archive rows");
    assert_eq!(rows, 1, "a cross-tenant delete must not remove the row");
}

/// An agent may only unarchive for a target it is allowed to manage.
#[tokio::test]
async fn ar12_agent_target_user_must_be_an_active_member() {
    let pool = connect_and_migrate().await;
    let f = seed(&pool).await;
    let app = app_for(&pool).await;

    // A user id that exists nowhere: `getResponsibleUserSnapshot` denies it.
    let unknown_user = Uuid::new_v4();
    let (status, body) = send(
        &app,
        &agent_actor(&f),
        "POST",
        &format!("/issues/{}/inbox-archive", f.issue_id),
        Some(json!({ "userId": unknown_user.to_string() })),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let json = parse_json(&body);
    assert_eq!(json["code"], "inbox_target_user_unresolved");
    assert_eq!(json["reason"], "deny_missing_membership");
}
