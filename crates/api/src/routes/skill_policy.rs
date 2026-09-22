//! Company Skill Policy routes.
//!
//! 逐行对齐 paperclip `server/src/routes/company-skill-policy.ts`：
//! `GET/PUT/DELETE /companies/:company_id/skill-policy` 与
//! `POST /companies/:company_id/skill-policy/evaluate`。早期的
//! `mode`/`allowRules`/`denyRules` 模型与 `POST .../simulate` 已被文档模型
//! （`schemaVersion`/`defaultEffect`/`rules`）取代，无兼容别名。
//!
//! 错误体复刻 `middleware/error-handler.ts:107-119` 的 `HttpError` 分支：
//! `{ error, code?, reason?, remediation?, details? }`。因为 `AppError` 无法携带
//! `code`，本模块的 handler 直接返回 `(StatusCode, Json<Value>)`（同
//! `automation_misc.rs` 的既有约定）。
//!
//! 注意两个 Paperclip 细节：
//! - `validatePolicyBody` 是**中间件**，先于 handler 执行 —— 校验失败优先于
//!   鉴权失败（未认证的非法 body 得到 422 而非 401）。
//! - `assertCompanyAccess` 按 HTTP 方法区分读写：POST/PUT/DELETE 属写语义，
//!   viewer 成员一律 403。

use crate::app_state::AppState;
use crate::routes::{require_company_access, AccessMode};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use services::auth::{ActorSource, AuthorizationActor, PermissionKey};
use services::skill_policy_service::{
    normalize_skill_policy_source_locator, normalize_skill_policy_source_type,
    parse_evaluate_skill_policy_request, parse_replace_skill_policy_request, SkillPolicyAction,
    SkillPolicyActivity, SkillPolicyError, SkillPolicyEvaluateInput,
    SkillPolicyEvaluationResource, SkillPolicyPrincipal, SkillPolicyPrincipalType,
    SkillPolicyReplaceInput, SkillPolicyResetInput,
};
use uuid::Uuid;

pub fn skill_policy_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/companies/:company_id/skill-policy",
            get(get_skill_policy)
                .put(replace_skill_policy)
                .delete(reset_skill_policy),
        )
        .route(
            "/companies/:company_id/skill-policy/evaluate",
            post(evaluate_skill_policy),
        )
}

type PolicyError = (StatusCode, Json<Value>);

// ---------------------------------------------------------------------------
// 错误体
// ---------------------------------------------------------------------------

/// Paperclip `HttpError` → `error-handler.ts:107-119` 的响应体。
///
/// `details` 仅在 `code === "skill_policy_denied"` 时被省略（脱敏）；
/// `remediation` 只有是字符串时才出现。
fn http_error(
    status: StatusCode,
    error: &str,
    code: &str,
    remediation: Option<&str>,
    details: Value,
) -> PolicyError {
    let mut body = json!({ "error": error, "code": code });
    if let Some(remediation) = remediation {
        body["remediation"] = Value::from(remediation);
    }
    if code != "skill_policy_denied" {
        body["details"] = details;
    }
    (status, Json(body))
}

/// Paperclip `HttpError(401, "Authentication required", { code })`。
fn authentication_required() -> PolicyError {
    http_error(
        StatusCode::UNAUTHORIZED,
        "Authentication required",
        "skill_authentication_required",
        None,
        json!({ "code": "skill_authentication_required" }),
    )
}

/// Paperclip `forbidden("Agent key cannot access another company", { code })`。
fn company_boundary_denied() -> PolicyError {
    http_error(
        StatusCode::FORBIDDEN,
        "Agent key cannot access another company",
        "skill_company_boundary_denied",
        None,
        json!({ "code": "skill_company_boundary_denied" }),
    )
}

/// Paperclip `forbidden("Skill policy administration authority required", {...})`。
fn admin_required() -> PolicyError {
    const REMEDIATION: &str = "Ask a company administrator to manage the skill policy.";
    http_error(
        StatusCode::FORBIDDEN,
        "Skill policy administration authority required",
        "skill_policy_admin_required",
        Some(REMEDIATION),
        json!({
            "code": "skill_policy_admin_required",
            "remediation": REMEDIATION,
        }),
    )
}

/// Paperclip `unprocessable("Invalid skill policy document", { code, issues })`。
fn validation_failed(issues: Vec<Value>) -> PolicyError {
    http_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Invalid skill policy document",
        "skill_policy_validation_failed",
        None,
        json!({
            "code": "skill_policy_validation_failed",
            "issues": issues,
        }),
    )
}

/// `assertCompanyAccess` 的拒绝体（`routes/authz.ts:75-121`）：匿名主体先经
/// `assertAuthenticated` 拿到 401，其余情况是 403。与 `automation_misc.rs`
/// 的同名私有函数逐字一致。
fn company_access_denied(actor: &AuthorizationActor) -> PolicyError {
    if actor.is_anonymous() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Unauthorized" })),
        );
    }
    let message = match actor {
        AuthorizationActor::Agent { .. } => "Agent key cannot access another company",
        _ => "User does not have access to this company",
    };
    (StatusCode::FORBIDDEN, Json(json!({ "error": message })))
}

/// 错误中间件未识别错误时的兜底 500（`error-handler.ts:150-153`）。
fn internal_server_error() -> PolicyError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Internal server error" })),
    )
}

fn serialize<T: Serialize>(value: T) -> Result<Json<Value>, PolicyError> {
    serde_json::to_value(value).map(Json).map_err(|error| {
        tracing::error!(error = %error, "failed to serialize skill policy response");
        internal_server_error()
    })
}

/// 把服务层错误映射到 Paperclip 的响应体。
fn map_skill_policy_error(error: SkillPolicyError) -> PolicyError {
    match error {
        SkillPolicyError::RevisionConflict {
            expected_revision,
            current_revision,
        } => http_error(
            StatusCode::CONFLICT,
            "Skill policy revision is stale",
            "skill_policy_revision_conflict",
            None,
            json!({
                "code": "skill_policy_revision_conflict",
                "expectedRevision": expected_revision,
                "currentRevision": current_revision,
            }),
        ),
        // Paperclip `notFound("Agent not found")`：无 code、无 details。
        SkillPolicyError::AgentNotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Agent not found" })),
        ),
        SkillPolicyError::CompanyBoundaryDenied => company_boundary_denied(),
        // 库里存量行解析失败：Paperclip 侧是 `ZodError`，被错误中间件渲染为
        // 400 `{ error: "Validation error", details: issues }`。
        SkillPolicyError::CorruptPolicy(issues) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Validation error", "details": issues })),
        ),
        SkillPolicyError::Database(error) => {
            tracing::error!(error = %error, "skill policy database error");
            internal_server_error()
        }
    }
}

// ---------------------------------------------------------------------------
// 授权
// ---------------------------------------------------------------------------

/// Paperclip `access.hasPermission(...)` 最终落到
/// `authorization.decidePrincipalGrant`（`services/authorization.ts:648-705`）：
/// 放行条件是「active membership + 该权限的 grant 行」，**角色本身不构成授权**。
/// 其中 `scopeAllows`（`:368-451`）在 grant 带非空 scope 而请求未带 scope 时
/// 一律 `deny_scope`，因此 `users:manage_permissions` 只认空 scope 的 grant。
///
/// 与 `automation_misc.rs` 的同名私有函数逐字一致（Paperclip 的
/// `access.canUser` / `access.hasPermission` 是同一个 `scopeAllows` 谓词）。
async fn has_unscoped_grant(
    state: &AppState,
    company_id: Uuid,
    principal_type: &str,
    principal_id: Uuid,
    permission_key: &str,
) -> Result<bool, PolicyError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM principal_permission_grants \
         WHERE company_id = $1 AND principal_type = $2::principal_type \
         AND principal_id = $3 AND permission_key = $4 AND scope = '{}'::jsonb)",
    )
    .bind(company_id)
    .bind(principal_type)
    .bind(principal_id)
    .bind(permission_key)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(error = %error, "failed to query principal permission grants");
        internal_server_error()
    })
}

/// Paperclip `assertSkillPolicyCompanyAccess`（`company-skill-policy.ts:28-37`）：
/// 匿名 401 → agent 跨公司 403 → `assertCompanyAccess`。
///
/// `mode` 对应 Paperclip 里 `assertCompanyAccess` 依据 `req.method` 推出的
/// 读/写语义：GET 为读，POST/PUT/DELETE 为写（viewer 成员写操作 403）。
fn assert_skill_policy_company_access(
    actor: &AuthorizationActor,
    company_id: Uuid,
    mode: AccessMode,
) -> Result<(), PolicyError> {
    if actor.is_anonymous() {
        return Err(authentication_required());
    }
    if let AuthorizationActor::Agent {
        company_id: actor_company,
        ..
    } = actor
    {
        if *actor_company != company_id {
            return Err(company_boundary_denied());
        }
    }
    require_company_access(actor, company_id, mode).map_err(|_| company_access_denied(actor))
}

/// Paperclip `assertCanAdministerPolicy`（`company-skill-policy.ts:39-56`）。
///
/// 公司边界之后，board 侧只认 `local_implicit`、实例管理员，或该用户自己的
/// `users:manage_permissions` grant 行；agent 侧只认 agent 主体的 grant 行。
/// **公司 Admin 与 Owner 的角色本身都不构成授权**（Paperclip 的
/// `grantsForHumanRole` 只给 owner 预置该权限、admin 明确不给）。
async fn assert_can_administer_policy(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    mode: AccessMode,
) -> Result<(), PolicyError> {
    assert_skill_policy_company_access(actor, company_id, mode)?;

    let permitted = match actor {
        AuthorizationActor::Board {
            user_id,
            source,
            is_instance_admin,
            ..
        } => {
            matches!(source, ActorSource::LocalImplicit)
                || *is_instance_admin
                || has_unscoped_grant(
                    state,
                    company_id,
                    "user",
                    *user_id,
                    PermissionKey::USERS_MANAGE_PERMISSIONS,
                )
                .await?
        }
        AuthorizationActor::Agent { agent_id, .. } => {
            has_unscoped_grant(
                state,
                company_id,
                "agent",
                *agent_id,
                PermissionKey::USERS_MANAGE_PERMISSIONS,
            )
            .await?
        }
        AuthorizationActor::None => false,
    };

    if permitted {
        Ok(())
    } else {
        Err(admin_required())
    }
}

/// Paperclip `currentPrincipal`（`company-skill-policy.ts:58-67`）。
async fn current_principal(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<SkillPolicyPrincipal, PolicyError> {
    match actor {
        AuthorizationActor::Agent { agent_id, .. } => state
            .skill_policy_service
            .resolve_agent_principal(company_id, *agent_id)
            .await
            .map_err(map_skill_policy_error),
        AuthorizationActor::Board { user_id, .. } => Ok(SkillPolicyPrincipal {
            principal_type: SkillPolicyPrincipalType::Board,
            id: user_id.to_string(),
            role: Some("board".to_string()),
        }),
        AuthorizationActor::None => Err(http_error(
            StatusCode::FORBIDDEN,
            "Authenticated company actor required",
            "skill_authentication_required",
            None,
            json!({ "code": "skill_authentication_required" }),
        )),
    }
}

/// Paperclip `getActorInfo(req)`（`routes/authz.ts:197-241`）的审计字段子集。
fn activity_of(actor: &AuthorizationActor) -> SkillPolicyActivity {
    let (agent_id, run_id) = match actor {
        AuthorizationActor::Agent {
            agent_id, run_id, ..
        } => (Some(*agent_id), *run_id),
        _ => (None, None),
    };
    SkillPolicyActivity {
        actor_type: actor.actor_type().to_string(),
        actor_id: actor.principal_id().unwrap_or_else(Uuid::nil),
        agent_id,
        run_id,
    }
}

// ---------------------------------------------------------------------------
// skill 变更类操作的策略网关
// ---------------------------------------------------------------------------

/// Paperclip `assertCanMutateCompanySkills`（`company-skills.ts:194-232`）的策略层。
///
/// 公司边界与平台不变式由各 handler 自己的 `require_company_access` 覆盖；
/// 这里补齐匿名/跨公司 agent 的稳定错误码，然后按主体评估公司策略。
/// 拒绝体是 `toSkillPolicyDenialResponse`（`company-skills.ts:134-141`）：
/// `{ code: "skill_policy_denied", reason, remediation? }`。
pub(crate) async fn enforce_skill_policy(
    state: &AppState,
    actor: &AuthorizationActor,
    company_id: Uuid,
    action: SkillPolicyAction,
    resource: SkillPolicyEvaluationResource,
) -> Result<(), PolicyError> {
    assert_skill_policy_company_access(actor, company_id, AccessMode::Write)?;
    let principal = current_principal(state, actor, company_id).await?;
    let decision = state
        .skill_policy_service
        .evaluate(SkillPolicyEvaluateInput {
            company_id,
            principal,
            action,
            resource,
        })
        .await
        .map_err(map_skill_policy_error)?;

    if decision.allowed {
        return Ok(());
    }
    let mut body = json!({
        "code": "skill_policy_denied",
        "reason": decision.reason,
    });
    if let Some(remediation) = decision.remediation {
        body["remediation"] = Value::from(remediation);
    }
    Err((StatusCode::FORBIDDEN, Json(body)))
}

/// Convert this module's fully-rendered error into `AppError::Rendered`.
///
/// `skills.rs` handlers use `AppError`; the policy gateway here needs the
/// `code`/`remediation` body fields that `AppError`'s plain string variants
/// drop. Round-tripping through `Rendered` keeps one response shape on both
/// paths instead of restating Paperclip's error bodies a second time.
impl From<PolicyError> for crate::errors::AppError {
    fn from((status, Json(body)): PolicyError) -> Self {
        crate::errors::AppError::Rendered { status, body }
    }
}

/// Paperclip `skillPolicyResource`（`company-skills.ts:164-181`）：按 `skill_id`
/// 读存量行，用请求携带的值覆盖 `sourceLocator`/`key`/`sourceType`。
///
/// 与 Paperclip 的唯一差异：`getById` 未命中时它返回 `null` 并退化为「只有
/// 请求值」的资源；这里的 `get_skill_by_id` 把未命中报成 `NotFound`。因此
/// 未命中一律容忍为 `None`——handler 自己的 404 才是权威的未命中响应，
/// 策略网关不该抢先返回一个不同的错误。
pub(crate) async fn skill_policy_resource(
    state: &AppState,
    company_id: Uuid,
    skill_id: Uuid,
    skill_key: Option<&str>,
    source_type: Option<&str>,
    source_locator: Option<&str>,
) -> SkillPolicyEvaluationResource {
    let stored = state
        .skill_registry_service
        .get_skill_by_id(company_id, skill_id)
        .await
        .ok();
    let stored_str = |key: &str| {
        stored
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(Value::as_str)
    };

    let skill_key = skill_key
        .and_then(as_non_empty)
        .or_else(|| stored_str("key"));
    let source_type = source_type
        .and_then(as_non_empty)
        .or_else(|| stored_str("sourceType"));
    let source_locator = source_locator
        .and_then(as_non_empty)
        .or_else(|| stored_str("sourceLocator"));

    SkillPolicyEvaluationResource {
        skill_id: Some(skill_id),
        skill_key: skill_key.map(str::to_string),
        source_type: Some(normalize_skill_policy_source_type(source_type)),
        source_locator: source_locator.map(normalize_skill_policy_source_locator),
    }
}

/// Paperclip `asString`：只接受非空字符串（会先 trim 判空）。
fn as_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /companies/:company_id/skill-policy`
async fn get_skill_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, PolicyError> {
    assert_skill_policy_company_access(&actor, company_id, AccessMode::Read)?;
    let policy = state
        .skill_policy_service
        .get(company_id)
        .await
        .map_err(map_skill_policy_error)?;
    serialize(policy)
}

/// `PUT /companies/:company_id/skill-policy`
async fn replace_skill_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, PolicyError> {
    let (policy, expected_revision) =
        parse_replace_skill_policy_request(&body).map_err(validation_failed)?;
    assert_can_administer_policy(&state, &actor, company_id, AccessMode::Write).await?;
    let result = state
        .skill_policy_service
        .replace(SkillPolicyReplaceInput {
            company_id,
            expected_revision,
            policy,
            activity: activity_of(&actor),
        })
        .await
        .map_err(map_skill_policy_error)?;
    serialize(result)
}

/// `DELETE /companies/:company_id/skill-policy` —— 200 带响应体（非 204）。
async fn reset_skill_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, PolicyError> {
    assert_can_administer_policy(&state, &actor, company_id, AccessMode::Write).await?;
    let result = state
        .skill_policy_service
        .reset(SkillPolicyResetInput {
            company_id,
            activity: activity_of(&actor),
        })
        .await
        .map_err(map_skill_policy_error)?;
    serialize(result)
}

/// `POST /companies/:company_id/skill-policy/evaluate`
async fn evaluate_skill_policy(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, PolicyError> {
    let request = parse_evaluate_skill_policy_request(&body).map_err(validation_failed)?;
    assert_skill_policy_company_access(&actor, company_id, AccessMode::Write)?;

    let principal = match request.principal_agent_id {
        // 以他人身份评估属管理动作：先要管理权，再解析被评估的 agent。
        Some(agent_id) => {
            assert_can_administer_policy(&state, &actor, company_id, AccessMode::Write).await?;
            state
                .skill_policy_service
                .resolve_agent_principal(company_id, agent_id)
                .await
                .map_err(map_skill_policy_error)?
        }
        None => current_principal(&state, &actor, company_id).await?,
    };

    let decision = state
        .skill_policy_service
        .evaluate(SkillPolicyEvaluateInput {
            company_id,
            principal,
            action: request.action,
            resource: request.resource,
        })
        .await
        .map_err(map_skill_policy_error)?;
    serialize(decision)
}
