//! HTTP parity tests for inbox agent policy routes.
//!
//! Tests stand up a real Axum router with a real AppState and inject
//! AuthorizationActor extensions, bypassing the global auth middleware.

use api::routes::automation_misc::automation_misc_routes;
use api::routes::companies::company_routes;
use axum::Router;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use parrot_server::build_app_state;
use serde_json::{json, Value};
use services::auth::{
    ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
};
use sqlx::PgPool;
use tower::util::ServiceExt;
use uuid::Uuid;


mod common;
use common::{connect_and_migrate, delete_company};


async fn seed_company(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    let prefix = format!("IA{}", &id.simple().to_string()[..8]);
    sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
        .bind(id).bind("Inbox Agent Policy Test").bind(prefix)
        .execute(pool).await.expect("insert company");
    id
}

fn board_actor(
    company_id: Uuid,
    role: MembershipRole,
    is_instance_admin: bool,
) -> (AuthorizationActor, Uuid) {
    let uid = Uuid::new_v4();
    (
        AuthorizationActor::board_with_source(
            uid,
            company_id,
            ActorSource::Session,
            vec![CompanyMembership::new(
                company_id,
                PrincipalType::User,
                uid,
                role,
            )],
            is_instance_admin,
        ),
        uid,
    )
}

fn owner_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    board_actor(company_id, MembershipRole::Owner, false)
}

fn admin_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    board_actor(company_id, MembershipRole::Admin, false)
}

fn viewer_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    board_actor(company_id, MembershipRole::Viewer, false)
}

fn instance_admin_actor(company_id: Uuid) -> (AuthorizationActor, Uuid) {
    board_actor(company_id, MembershipRole::Viewer, true)
}

fn agent_actor(company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(Uuid::new_v4(), company_id, None)
}

fn agent_actor_for(agent_id: Uuid, company_id: Uuid) -> AuthorizationActor {
    AuthorizationActor::agent(agent_id, company_id, None)
}

async fn ensure_agent(pool: &PgPool, agent_id: Uuid, company_id: Uuid, name: &str) {
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(agent_id)
        .bind(company_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert agent");
}

async fn ensure_auth_user(pool: &PgPool, uid: Uuid) {
    sqlx::query("INSERT INTO auth_users (id, email, name) VALUES ($1, $2, 'Test User') ON CONFLICT DO NOTHING")
        .bind(uid).bind(format!("{uid}@test.example"))
        .execute(pool).await.expect("insert auth_user");
}

async fn ensure_membership(pool: &PgPool, company_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO company_memberships (company_id, principal_type, principal_id, membership_role, status) VALUES ($1, 'user', $2, $3::text::membership_role, 'active') ON CONFLICT DO NOTHING"
    )
    .bind(company_id).bind(user_id).bind(role)
    .execute(pool).await.expect("insert membership");
}

/// 写入一条真实的授权行。`granted_by_user_id` 有指向 `auth_users` 的外键，
/// 因此即便被授予主体是 agent，也必须提供一个真实用户作为授予者。
async fn ensure_grant(
    pool: &PgPool,
    company_id: Uuid,
    principal_type: &str,
    principal_id: Uuid,
    permission_key: &str,
    scope: Value,
    granted_by_user_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO principal_permission_grants \
         (id, company_id, principal_type, principal_id, permission_key, scope, granted_by_user_id) \
         VALUES ($1, $2, $3::principal_type, $4, $5, $6, $7) \
         ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(company_id)
    .bind(principal_type)
    .bind(principal_id)
    .bind(permission_key)
    .bind(scope)
    .bind(granted_by_user_id)
    .execute(pool)
    .await
    .expect("insert grant");
}

async fn get(app: &axum::Router, uri: &str, actor: &AuthorizationActor) -> (StatusCode, Value) {
    let mut req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn put(app: &axum::Router, uri: &str, actor: &AuthorizationActor, body_val: Value) -> (StatusCode, Value) {
    let mut req = Request::builder().method("PUT").uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body_val).unwrap())).unwrap();
    req.extensions_mut().insert(actor.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn inbox_agent_policy_get_self_returns_open_default() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200, "get self policy={body:?}");
    assert_eq!(body["mode"], "open", "default mode is open");
    assert_eq!(body["materialized"], Value::Bool(false), "not materialized");
    assert!(body["allowedAgentIds"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn inbox_agent_policy_update_and_read_self() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;

    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Policy Test Agent') ON CONFLICT DO NOTHING")
        .bind(agent_id).bind(cid)
        .execute(&pool).await.expect("insert agent");

    // Update to allowlist mode with one allowed agent
    let (status, body) = put(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor, json!({
        "mode": "allowlist",
        "allowedAgentIds": [agent_id]
    })).await;
    assert_eq!(status, 200, "update self policy={body:?}");
    assert_eq!(body["mode"], "allowlist");
    let allowed = body["allowedAgentIds"].as_array().unwrap();
    assert_eq!(allowed.len(), 1, "one allowed agent: {allowed:?}");

    // Read back
    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200);
    assert_eq!(body["mode"], "allowlist");
    assert_eq!(body["materialized"], Value::Bool(true));
    assert_eq!(body["allowedAgentIds"][0].as_str().unwrap(), agent_id.to_string());
}

/// `assertAdmin`（`routes/inbox-agent-policy.ts:19-35`）只认
/// `users:manage_permissions` 这一条 grant 行，角色本身完全不作数：
/// owner 拿到默认授权后可以通过，而**公司 Admin 不行**——Paperclip 的
/// `grantsForHumanRole`（`services/company-member-roles.ts:1-48`）给 admin
/// 的角色默认集里没有这条权限。
#[tokio::test]
async fn inbox_agent_policy_owner_with_grant_can_read_other_user() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = owner_actor(cid);
    let (_target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "owner").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;
    ensure_grant(
        &pool,
        cid,
        "user",
        aid,
        "users:manage_permissions",
        json!({}),
        aid,
    )
    .await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200, "owner with grant read other={body:?}");
    assert_eq!(body["mode"], "open");
}

/// 与上一个测试互为对照：仅凭公司 Admin 角色不构成管理授权。
/// `assertAdmin` 走 `access.canUser(...)` → `decidePrincipalGrant`
/// （`authorization.ts:648-705`），它只查 membership + grant 行，没有任何
/// 角色兜底；而 admin 的角色默认授权里不含 `users:manage_permissions`。
#[tokio::test]
async fn inbox_agent_policy_company_admin_without_grant_is_forbidden() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = admin_actor(cid);
    let (_target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "admin").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &actor).await;
    assert_eq!(
        status, 403,
        "admin without explicit grant cannot read other user's policy: {body:?}"
    );
    assert_eq!(
        body["error"],
        "Inbox agent policy administration authority required"
    );
    assert_eq!(body["code"], "inbox_agent_policy_admin_required");
}
/// `assertAdmin` 的第一条捷径是 `isInstanceAdmin`（`routes/inbox-agent-policy.ts:19-35`），
/// 它在查 grant 之前就放行，因此实例管理员即使是 Viewer 角色也能管理。
#[tokio::test]
async fn inbox_agent_policy_instance_admin_bypasses_grant_check() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = instance_admin_actor(cid);
    let (_target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "viewer").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 200, "instance admin read other={body:?}");
    assert_eq!(body["mode"], "open");
}

/// 非空 `scope` 的 grant 不能授权本能力：`scopeAllows`
/// (`authorization.ts:368-451`) 在请求方没有 scope 时直接 `deny_scope`，
/// 所以只有 `null` / `{}` 的 scope 才算通过。
#[tokio::test]
async fn inbox_agent_policy_scoped_grant_does_not_authorize() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = owner_actor(cid);
    let (_target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "owner").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;
    ensure_grant(
        &pool,
        cid,
        "user",
        aid,
        "users:manage_permissions",
        json!({ "projectIds": [Uuid::new_v4()] }),
        aid,
    )
    .await;

    let (status, _body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 403, "scoped grant is not sufficient");
}

/// agent 分支要求调用者身份绑定到具体 agent 且该 agent 持有
/// `users:manage_permissions` grant。测试用的 `AuthorizationActor::agent`
/// 没有 on-behalf-of 用户，因此 `assertAdmin` 之前的 self-user 解析
/// （`routes/inbox-agent-policy.ts:14-17`）就以 401 拒绝。
#[tokio::test]
async fn inbox_agent_policy_agent_without_user_context_is_unauthorized() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let agent = agent_actor(cid);

    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &agent).await;
    assert_eq!(status, 401, "agent has no board user context: {body:?}");
    assert_eq!(body["error"], "Board user context required");
}

/// agent 持 grant 但仍无 board 用户上下文 → 同样 401，证明 agent 路径
/// 并不因为持有 grant 就跳过 `selfUserId` 检查。
#[tokio::test]
async fn inbox_agent_policy_agent_with_grant_still_requires_user_context() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (_grantor, gid) = owner_actor(cid);
    ensure_auth_user(&pool, gid).await;
    ensure_membership(&pool, cid, gid, "owner").await;

    let agent_id = Uuid::new_v4();
    ensure_agent(&pool, agent_id, cid, "Policy Agent").await;
    ensure_grant(
        &pool,
        cid,
        "agent",
        agent_id,
        "users:manage_permissions",
        json!({}),
        gid,
    )
    .await;

    let agent = agent_actor_for(agent_id, cid);
    let (status, body) = get(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &agent).await;
    assert_eq!(status, 401, "agent with grant still needs user context: {body:?}");
}

/// `assertActiveUserMembership`（`routes/inbox-agent-policy.ts:37-42`）在
/// 管理读取时校验目标用户存在**且** membership 为 active。目标用户不存在
/// 时是 404，不是 403——授权先通过、存在性后校验。
#[tokio::test]
async fn inbox_agent_policy_admin_gate_precedes_target_existence() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = owner_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "owner").await;
    ensure_grant(
        &pool,
        cid,
        "user",
        aid,
        "users:manage_permissions",
        json!({}),
        aid,
    )
    .await;

    // 目标用户不存在
    let missing = Uuid::new_v4();
    let (status, _body) = get(&app, &format!("/companies/{cid}/users/{missing}/inbox-agent-policy"), &actor).await;
    assert_eq!(status, 404, "granted actor + missing target user → 404");
}

/// 公司 Admin 只改自己的策略走 `PUT /me`，该路由**没有** `assertAdmin`
/// （`routes/inbox-agent-policy.ts:79-88`），因此 Admin 可以维护自己的
/// inbox 策略——这正是 403 只作用于 `/:userId` 的原因。
#[tokio::test]
async fn inbox_agent_policy_company_admin_can_write_own_policy() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, aid) = admin_actor(cid);
    ensure_auth_user(&pool, aid).await;
    ensure_membership(&pool, cid, aid, "admin").await;

    let (status, body) = put(&app, &format!("/companies/{cid}/users/me/inbox-agent-policy"), &actor, json!({
        "mode": "disabled",
        "allowedAgentIds": []
    })).await;
    assert_eq!(status, 200, "admin writes own policy: {body:?}");
    assert_eq!(body["mode"], "disabled");
}

/// 与上一条对照：Viewer 既没有 `users:manage_permissions`，也没有别的
/// 角色兜底 → 403。
#[tokio::test]
async fn inbox_agent_policy_viewer_cannot_read_other_user() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (viewer, vid) = viewer_actor(cid);
    let (_target, tid) = owner_actor(cid);
    ensure_auth_user(&pool, vid).await;
    ensure_membership(&pool, cid, vid, "viewer").await;
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "owner").await;

    let (status, body) = get(&app, &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"), &viewer).await;
    assert_eq!(status, 403, "viewer cannot read other user's policy");
    assert_eq!(body["code"], "inbox_agent_policy_admin_required");
}



/// `inboxAgentPolicyModeSchema = z.enum(["open","allowlist","disabled"])`
/// (`packages/shared/src/validators/inbox-agent-policy.ts:3`). `disabled` is
/// what makes the `inbox_management_disabled` denial reachable in
/// `resolveInboxArchiveTarget`, so it must round-trip.
#[tokio::test]
async fn inbox_agent_policy_accepts_disabled_mode() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;

    let uri = format!("/companies/{cid}/users/me/inbox-agent-policy");
    let (status, body) = put(&app, &uri, &actor, json!({ "mode": "disabled" })).await;
    assert_eq!(status, 200, "disabled must be accepted: {body:?}");
    assert_eq!(body["mode"], "disabled");

    let (status, body) = get(&app, &uri, &actor).await;
    assert_eq!(status, 200);
    assert_eq!(body["mode"], "disabled", "disabled must round-trip");
    assert_eq!(body["materialized"], Value::Bool(true));
}

/// The `.strict()` + `superRefine` shape rules all fail as route-level Zod
/// validation, i.e. **400** — not 422.
#[tokio::test]
async fn inbox_agent_policy_route_schema_failures_are_bad_request() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;
    let uri = format!("/companies/{cid}/users/me/inbox-agent-policy");
    let agent_id = Uuid::new_v4().to_string();

    // Unknown mode.
    let (status, _) = put(&app, &uri, &actor, json!({ "mode": "sometimes" })).await;
    assert_eq!(status, 400, "unknown mode fails the enum");

    // Missing `mode` entirely.
    let (status, _) = put(&app, &uri, &actor, json!({})).await;
    assert_eq!(status, 400, "mode is required");

    // `allowedAgentIds` non-empty while mode is not allowlist (superRefine).
    let (status, _) = put(
        &app,
        &uri,
        &actor,
        json!({ "mode": "open", "allowedAgentIds": [agent_id] }),
    )
    .await;
    assert_eq!(
        status, 400,
        "allowedAgentIds must be empty when mode is not allowlist"
    );
    let (status, _) = put(
        &app,
        &uri,
        &actor,
        json!({ "mode": "disabled", "allowedAgentIds": [agent_id] }),
    )
    .await;
    assert_eq!(status, 400, "same rule applies to disabled");

    // `.strict()` — unknown top-level key.
    let (status, _) = put(
        &app,
        &uri,
        &actor,
        json!({ "mode": "open", "extra": true }),
    )
    .await;
    assert_eq!(status, 400, "strict schema rejects unknown keys");

    // `allowedAgentIds` must be uuids.
    let (status, _) = put(
        &app,
        &uri,
        &actor,
        json!({ "mode": "allowlist", "allowedAgentIds": ["not-a-uuid"] }),
    )
    .await;
    assert_eq!(status, 400, "allowedAgentIds entries must be uuids");
}

/// Only `allowlist` persists ids; other modes store an empty list
/// (`services/inbox-agent-policy.ts:34`).
#[tokio::test]
async fn inbox_agent_policy_clears_allowed_agents_for_non_allowlist_modes() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;

    let agent_id = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Switch Agent')")
        .bind(agent_id)
        .bind(cid)
        .execute(&pool)
        .await
        .expect("insert agent");

    let uri = format!("/companies/{cid}/users/me/inbox-agent-policy");
    let (status, _) = put(
        &app,
        &uri,
        &actor,
        json!({ "mode": "allowlist", "allowedAgentIds": [agent_id] }),
    )
    .await;
    assert_eq!(status, 200);

    // Switching to `disabled` must not leave stale ids behind.
    let (status, body) = put(&app, &uri, &actor, json!({ "mode": "disabled" })).await;
    assert_eq!(status, 200);
    assert!(
        body["allowedAgentIds"].as_array().unwrap().is_empty(),
        "leaving allowlist clears the list: {body:?}"
    );
    let stored: Vec<Uuid> = sqlx::query_scalar(
        "SELECT allowed_agent_ids FROM user_inbox_agent_policies WHERE company_id = $1 AND user_id = $2",
    )
    .bind(cid)
    .bind(uid)
    .fetch_one(&pool)
    .await
    .expect("load policy row");
    assert!(stored.is_empty(), "db row must be cleared too: {stored:?}");
}

/// An allowlist naming an agent from another company is a **service-layer**
/// `unprocessable`, i.e. 422 — distinct from the 400 shape failures above.
#[tokio::test]
async fn inbox_agent_policy_foreign_agent_is_unprocessable() {
    let pool = connect_and_migrate().await;
    let cid = seed_company(&pool).await;
    let other_cid = seed_company(&pool).await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let app = automation_misc_routes().with_state(state);
    let (actor, uid) = owner_actor(cid);
    ensure_auth_user(&pool, uid).await;
    ensure_membership(&pool, cid, uid, "owner").await;

    let foreign_agent = Uuid::new_v4();
    sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Foreign Agent')")
        .bind(foreign_agent)
        .bind(other_cid)
        .execute(&pool)
        .await
        .expect("insert foreign agent");

    let (status, _) = put(
        &app,
        &format!("/companies/{cid}/users/me/inbox-agent-policy"),
        &actor,
        json!({ "mode": "allowlist", "allowedAgentIds": [foreign_agent] }),
    )
    .await;
    assert_eq!(
        status, 422,
        "agents outside the company are a service-layer unprocessable"
    );
}

/// 端到端：走真实的 `POST /companies` 建公司，owner 随后必须能管理
/// inbox agent 策略。
///
/// 这是 Paperclip `routes/companies.ts:1110-1140` 的接线验证——建公司时
/// `ensureMembership(owner)` 之后紧接着 `ensureRoleDefaultGrants(owner)`，
/// 其中包含 `users:manage_permissions`。若这一步缺失，公司主人反而会被
/// 自己的 `assertAdmin` 以 403 拒之门外：Paperclip 的 `assertAdmin` 不看
/// 角色、只认 grant 行（`routes/inbox-agent-policy.ts:19-35`）。
#[tokio::test]
async fn company_creation_grants_owner_inbox_agent_policy_authority() {
    let pool = connect_and_migrate().await;
    let state = build_app_state(pool.clone()).await.unwrap();
    let owner_id = Uuid::new_v4();
    ensure_auth_user(&pool, owner_id).await;

    let app = Router::new()
        .merge(company_routes())
        .merge(automation_misc_routes())
        .with_state(state);

    // 以实例管理员身份建公司：Paperclip 的 `POST /companies` 要求
    // `local_implicit` 或实例管理员（`routes/companies.ts:1110-1114`）。
    let creator = AuthorizationActor::board_with_source(
        owner_id,
        Uuid::new_v4(),
        ActorSource::LocalImplicit,
        vec![],
        true,
    );
    let mut req = Request::builder()
        .method("POST")
        .uri("/companies")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({ "name": "E2E Grant Co" })).unwrap(),
        ))
        .unwrap();
    req.extensions_mut().insert(creator);
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    assert_eq!(status, StatusCode::CREATED, "create company={created:?}");
    let cid = Uuid::parse_str(created["id"].as_str().expect("company id")).unwrap();

    // 公司创建流程必须落下一行 owner 的 `users:manage_permissions` 授权。
    let granted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM principal_permission_grants \
         WHERE company_id = $1 AND principal_type = 'user' AND principal_id = $2 \
         AND permission_key = 'users:manage_permissions' AND scope = '{}'::jsonb)",
    )
    .bind(cid)
    .bind(owner_id)
    .fetch_one(&pool)
    .await
    .expect("query grant");
    assert!(
        granted,
        "company creation must seed owner users:manage_permissions"
    );

    // 同一 owner（Session 来源、真实 membership）管理任意公司成员策略 → 200。
    let (_target, tid) = viewer_actor(cid);
    ensure_auth_user(&pool, tid).await;
    ensure_membership(&pool, cid, tid, "viewer").await;
    let owner = AuthorizationActor::board_with_source(
        owner_id,
        cid,
        ActorSource::Session,
        vec![CompanyMembership::new(
            cid,
            PrincipalType::User,
            owner_id,
            MembershipRole::Owner,
        )],
        false,
    );
    let (status, body) = get(
        &app,
        &format!("/companies/{cid}/users/{tid}/inbox-agent-policy"),
        &owner,
    )
    .await;
    assert_eq!(status, 200, "owner manages other user's policy: {body:?}");

    // Creating a company also provisions built-in agents, owner grants and the
    // two memberships above; the company row itself is not cascade-deleted, so
    // removing it here keeps the shared test database free of `E2E Grant Co`
    // debris that would otherwise exhaust the issue-prefix ladder.
    delete_company(&pool, cid).await;
}