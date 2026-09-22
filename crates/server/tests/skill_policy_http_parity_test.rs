//! HTTP parity integration tests for Paperclip's Company Skill Policy (#125).
//!
//! Ports `server/src/__tests__/company-skill-policy-routes.test.ts` over the real
//! router + database:
//! - the open default and the stable authentication / company-boundary errors,
//! - policy administration authority, revision CAS, rule evaluation, activity log,
//! - 422 for unknown actions and secret-bearing policy locators.
//!
//! Run with a live database, e.g.:
//!   DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/parrot_agent_compile \
//!     cargo test -p parrot-server --test skill_policy_http_parity_test

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;

use api::routes::skill_policy::skill_policy_routes;
use parrot_server::build_app_state;
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};

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

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("response body must be JSON")
}

/// Paperclip's `x-test-actor: board` — `source: "local_implicit"`,
/// `isInstanceAdmin: true`. Takes the `local_implicit || isInstanceAdmin`
/// fast path in `assertCanAdministerPolicy`.
fn board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board(user_id, company_id)
}

/// A board actor that is neither `local_implicit` nor an instance admin and holds
/// no `users:manage_permissions` grant — must be refused with 403.
fn session_board_actor(user_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::board_with_source(
        user_id,
        company_id,
        ActorSource::Session,
        vec![CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        )],
        false,
    )
}

struct Fixture {
    pool: PgPool,
    company_a: Uuid,
    company_b: Uuid,
    agent_id: Uuid,
}

async fn seed_fixture(pool: &PgPool) -> Fixture {
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    for (id, prefix) in [(company_a, "SPA"), (company_b, "SPB")] {
        sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(format!("Skill Policy Parity {prefix}"))
            .bind(format!("{prefix}{}", &id.simple().to_string()[..8]))
            .execute(pool)
            .await
            .expect("insert company");
    }
    // Paperclip's fixture uses `role: "engineer"`; Parrot's `valid_role` CHECK
    // only admits ceo/vp/manager/researcher/general.
    sqlx::query(
        "INSERT INTO agents (id, company_id, name, role, adapter_type, adapter_config, status) \
         VALUES ($1, $2, 'Builder', 'general', 'process', '{}'::jsonb, 'idle')",
    )
    .bind(agent_id)
    .bind(company_a)
    .execute(pool)
    .await
    .expect("insert agent");
    Fixture {
        pool: pool.clone(),
        company_a,
        company_b,
        agent_id,
    }
}

mod common;
use common::{connect_and_migrate, delete_company};

/// #125 company-skill-policy acceptance.
#[tokio::test]
async fn skill_policy_crud_evaluate_and_authz_match_paperclip() {
    let pool = connect_and_migrate().await;
    let f = seed_fixture(&pool).await;
    let state = build_app_state(pool.clone()).await.expect("build_app_state");
    let app = skill_policy_routes().with_state(state);

    let agent = AuthorizationActor::agent(f.agent_id, f.company_a, None);
    let board = board_actor(Uuid::new_v4(), f.company_a);
    let base = format!("/companies/{}/skill-policy", f.company_a);

    // --- 1. Open default and the stable authentication / boundary errors ------

    let (status, body) = send(&app, &agent, "GET", &base, None).await;
    assert_eq!(status, StatusCode::OK, "get default policy → 200");
    let default_policy = parse(&body);
    assert_eq!(default_policy["revision"], 0);
    assert_eq!(default_policy["materialized"], false);
    assert_eq!(default_policy["defaultEffect"], "allow");

    let (status, body) = send(&app, &AuthorizationActor::none(), "GET", &base, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "anonymous get → 401");
    assert_eq!(parse(&body)["code"], "skill_authentication_required");

    let cross_company = AuthorizationActor::agent(f.agent_id, f.company_b, None);
    let (status, body) = send(&app, &cross_company, "GET", &base, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-company agent get → 403");
    assert_eq!(parse(&body)["code"], "skill_company_boundary_denied");

    // --- 2. Administration authority, revisions, evaluation, audit ------------

    let document = json!({
        "schemaVersion": 1,
        "expectedRevision": 0,
        "defaultEffect": "allow",
        "rules": [{
            "id": "deny-remove",
            "priority": 1,
            "effect": "deny",
            "subject": { "type": "all_agents" },
            "actions": ["skills.remove"],
        }],
    });

    // Same-company agent without the `users:manage_permissions` grant.
    let (status, body) = send(&app, &agent, "PUT", &base, Some(document.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "agent put → 403");
    assert_eq!(parse(&body)["code"], "skill_policy_admin_required");

    // A session board actor is not `local_implicit` and holds no grant either.
    let session_board = session_board_actor(Uuid::new_v4(), f.company_a);
    let (status, body) = send(&app, &session_board, "PUT", &base, Some(document.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "session board put → 403");
    assert_eq!(parse(&body)["code"], "skill_policy_admin_required");

    let (status, body) = send(&app, &board, "PUT", &base, Some(document.clone())).await;
    assert_eq!(status, StatusCode::OK, "implicit board put → 200");
    let replaced = parse(&body);
    assert_eq!(replaced["revision"], 1);
    assert_eq!(replaced["materialized"], true);

    let (status, body) = send(&app, &board, "PUT", &base, Some(document)).await;
    assert_eq!(status, StatusCode::CONFLICT, "stale expectedRevision → 409");
    let conflict = parse(&body);
    assert_eq!(conflict["code"], "skill_policy_revision_conflict");
    assert_eq!(conflict["details"]["expectedRevision"], 0);
    assert_eq!(conflict["details"]["currentRevision"], 1);

    let evaluate = format!("{base}/evaluate");
    let (status, body) = send(
        &app,
        &agent,
        "POST",
        &evaluate,
        Some(json!({ "action": "skills.remove", "resource": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "evaluate denial → 200");
    let decision = parse(&body);
    assert_eq!(decision["allowed"], false);
    assert_eq!(decision["reason"], "explicit_rule");
    assert_eq!(decision["matchedRuleId"], "deny-remove");

    // Impersonating another principal is an administration action.
    let (status, body) = send(
        &app,
        &agent,
        "POST",
        &evaluate,
        Some(json!({
            "action": "skills.remove",
            "resource": {},
            "principal": { "agentId": f.agent_id },
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "impersonating evaluate → 403");
    assert_eq!(parse(&body)["code"], "skill_policy_admin_required");

    let (status, body) = send(&app, &board, "DELETE", &base, None).await;
    assert_eq!(status, StatusCode::OK, "delete policy → 200");
    let reset = parse(&body);
    assert_eq!(reset["revision"], 0);
    assert_eq!(reset["materialized"], false);

    let (replaced_rows, reset_rows): (i64, i64) = sqlx::query_as(
        "SELECT \
             COUNT(*) FILTER (WHERE event_type = 'company.skill_policy_replaced'), \
             COUNT(*) FILTER (WHERE event_type = 'company.skill_policy_reset') \
         FROM activity_logs \
         WHERE company_id = $1 AND resource_type = 'company_skill_policy' AND resource_id = $1",
    )
    .bind(f.company_a)
    .fetch_one(&pool)
    .await
    .expect("read activity log");
    assert_eq!(replaced_rows, 1, "one company.skill_policy_replaced row");
    assert_eq!(reset_rows, 1, "one company.skill_policy_reset row");

    // --- 3. Unknown actions and secret-bearing locators → 422 -----------------

    let (status, body) = send(
        &app,
        &agent,
        "POST",
        &evaluate,
        Some(json!({ "action": "skills.publish", "resource": {} })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "unknown action → 422");
    assert_eq!(parse(&body)["code"], "skill_policy_validation_failed");

    let (status, body) = send(
        &app,
        &board,
        "PUT",
        &base,
        Some(json!({
            "schemaVersion": 1,
            "expectedRevision": 0,
            "defaultEffect": "allow",
            "rules": [{
                "id": "secret-locator",
                "priority": 1,
                "effect": "deny",
                "subject": { "type": "all_agents" },
                "actions": ["skills.import"],
                "resources": { "sourceLocators": ["https://example.com/skill?token=do-not-store"] },
            }],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "secret locator → 422");
    assert_eq!(parse(&body)["code"], "skill_policy_validation_failed");

    delete_company(&pool, f.company_a).await;
    delete_company(&pool, f.company_b).await;
}
