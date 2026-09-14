//! 访问控制路由（对应任务拆解 §9 阶段一 + 阶段二 + 阶段三）。
//!
//! 提供 `/api` 下的访问控制路由组：
//! - 阶段一：Board 认领（board-claim + bootstrap）
//! - 阶段二：CLI 认证、Board API Key 管理、邀请、加入请求、成员管理
//! - 阶段三：实例管理员管理（promote / demote；用户列表见 user_directory）
//!
//! 路由组挂载 `AuthMiddleware` 层（`auth_middleware_fn`）将 `AuthorizationActor`
//! 注入 request extensions；handler 通过 `Extension<AuthorizationActor>` 读取，
//! 并通过 `assert_company_access` / `assert_instance_admin` 守卫进行授权检查。
//!
//! 注意：本路由组已被 `app_state::create_router` 通过 `.nest("/api", ..)` 挂载，
//! 因此此处的路径**不得**再带 `/api` 前缀，否则会变成 `/api/api/...` 死路径。

use std::{collections::HashMap, sync::Arc};

use axum::{
    async_trait,
    extract::{Extension, FromRequestParts, Path, Query, State},
    http::{request::Parts, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, delete, patch},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use services::auth::{
    auth_middleware_fn, middleware_from_env, AuthError, AuthorizationAction, AuthorizationActor,
    MembershipRole, PermissionKey,
};
use services::auth::authorization_service::assert_instance_admin;
use services::auth::board_claim::{
    claim_board_ownership, claim_first_instance_admin, inspect_board_claim_challenge,
    BoardClaimOutcome, ClaimStatus, FirstAdminClaimOutcome,
};
use services::auth::cli_auth::{
    approve_cli_auth_challenge, cancel_cli_auth_challenge, create_cli_auth_challenge,
    describe_cli_auth_challenge,
};
use services::auth::decision_engine::decide_access;

use crate::app_state::AppState;

/// 公司 ID 路径参数提取器（访问控制路由组通用，供阶段二/三成员与邀请端点复用）。
#[allow(dead_code)]
pub struct CompanyId(pub Uuid);

#[allow(dead_code)]
#[async_trait]
impl<S> FromRequestParts<S> for CompanyId
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .uri
            .path()
            .rsplit('/')
            .next()
            .ok_or_else(|| AuthError::bad_request("Missing company id".to_string()))?;
        let id = Uuid::parse_str(value)
            .map_err(|_| AuthError::bad_request("Invalid company id".to_string()))?;
        Ok(CompanyId(id))
    }
}

/// 成员 ID 路径参数提取器。
#[allow(dead_code)]
pub struct MemberId(pub Uuid);

#[allow(dead_code)]
#[async_trait]
impl<S> FromRequestParts<S> for MemberId
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .uri
            .path()
            .rsplit('/')
            .next()
            .ok_or_else(|| AuthError::bad_request("Missing member id".to_string()))?;
        let id = Uuid::parse_str(value)
            .map_err(|_| AuthError::bad_request("Invalid member id".to_string()))?;
        Ok(MemberId(id))
    }
}

/// 通用 token 路径参数提取器（用于 board-claim / invite 等挑战 token）。
#[allow(dead_code)]
pub struct Token(pub String);

#[allow(dead_code)]
#[async_trait]
impl<S> FromRequestParts<S> for Token
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let value = parts
            .uri
            .path()
            .rsplit('/')
            .next()
            .ok_or_else(|| AuthError::bad_request("Missing token".to_string()))?;
        if value.is_empty() {
            return Err(AuthError::bad_request("Missing token".to_string()));
        }
        Ok(Token(value.to_string()))
    }
}

/// 构建访问控制路由组，并挂载认证中间件层。
pub fn access_control_routes(state: AppState) -> Router<AppState> {
    let mw = Arc::new(middleware_from_env(Arc::new(state.pool.clone())));

    Router::new()
        // 阶段一：Board 认领
        .route(
            "/board-claim/:token",
            get(inspect_board_claim),
        )
        .route("/board-claim/:token/claim", post(claim_board))
        .route("/bootstrap/claim", post(bootstrap_claim))
        // 阶段二：CLI 认证
        .route("/cli-auth/challenges", post(create_cli_challenge_handler))
        .route(
            "/cli-auth/challenges/:id",
            get(get_cli_challenge_handler),
        )
        .route(
            "/cli-auth/challenges/:id/approve",
            post(approve_cli_challenge_handler),
        )
        .route(
            "/cli-auth/challenges/:id/cancel",
            post(cancel_cli_challenge_handler),
        )
        .route("/cli-auth/me", get(get_cli_auth_me))
        .route("/cli-auth/revoke-current", post(revoke_current_cli_key))
        // 阶段二：Board API Key 管理
        .route("/board-api-keys", get(list_board_api_keys).post(create_board_api_key))
        .route("/board-api-keys/:key_id", delete(revoke_board_api_key))
        // 阶段二：邀请管理
        .route("/companies/:company_id/invites", get(list_company_invites).post(create_invite))
        .route("/invites/:token", get(get_invite))
        .route("/invites/:token/accept", post(accept_invite))
        .route("/invites/:invite_id/revoke", post(revoke_invite))
        .route("/invites/:token/test-resolution", get(get_invite_test_resolution))
        // 阶段二：加入请求
        .route("/companies/:company_id/join-requests", get(list_join_requests))
        .route(
            "/companies/:company_id/join-requests/:request_id/approve",
            post(approve_join_request),
        )
        .route(
            "/companies/:company_id/join-requests/:request_id/reject",
            post(reject_join_request),
        )
        // 阶段二：成员管理
        .route("/companies/:company_id/members", get(list_members))
        .route(
            "/companies/:company_id/members/:member_id",
            patch(update_member),
        )
        .route(
            "/companies/:company_id/members/:member_id/role-and-grants",
            patch(update_member_role_and_grants),
        )
        .route(
            "/companies/:company_id/members/:member_id/permissions",
            patch(update_member_permissions),
        )
        .route(
            "/companies/:company_id/members/:member_id/archive",
            post(archive_member),
        )
        .layer(axum::middleware::from_fn_with_state(mw, auth_middleware_fn))
        .with_state(state)
}

/// GET /board-claim/:token?code=...
///
/// 查看 Board 认领挑战详情。对齐 Paperclip `access.ts:2671-2680`：
/// 状态为 `invalid` 时返回 404，其余原样返回。
async fn inspect_board_claim(
    Query(params): Query<BoardClaimQuery>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AuthError::not_found("Board claim challenge not found"));
    }
    let code = params
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let inspection = inspect_board_claim_challenge(&token, code);
    if inspection.status == ClaimStatus::Invalid {
        return Err(AuthError::not_found("Board claim challenge not found"));
    }
    Ok(Json(json!(inspection)))
}

#[derive(Debug, Clone, Deserialize)]
struct BoardClaimQuery {
    code: Option<String>,
}

/// POST /board-claim/:token/claim
///
/// 认领 Board 所有权：把当前会话用户提升为实例管理员，移除本地 board 主体的
/// 实例管理员角色，并让其在所有公司成为 owner。
/// 对齐 Paperclip `access.ts:2682-2717` 的状态码语义。
async fn claim_board(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(token): Path<String>,
    Json(payload): Json<BoardClaimRequest>,
) -> Result<impl IntoResponse, AuthError> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AuthError::not_found("Board claim challenge not found"));
    }
    let code = payload
        .code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if code.is_none() {
        return Err(AuthError::bad_request("Claim code is required"));
    }
    let user_id = session_user_id(&actor)?;

    match claim_board_ownership(&state.pool, &token, code, user_id).await? {
        BoardClaimOutcome::Claimed { claimed_by_user_id } => Ok((
            StatusCode::OK,
            Json(json!({ "claimed": true, "userId": claimed_by_user_id })),
        )),
        BoardClaimOutcome::NotAvailable(ClaimStatus::Invalid) => {
            Err(AuthError::not_found("Board claim challenge not found"))
        }
        BoardClaimOutcome::NotAvailable(ClaimStatus::Expired) => Err(AuthError::conflict(
            "Board claim challenge expired. Restart server to generate a new one.",
        )),
        BoardClaimOutcome::NotAvailable(ClaimStatus::Claimed) => {
            Err(AuthError::conflict("Board claim challenge is no longer available"))
        }
        BoardClaimOutcome::NotAvailable(ClaimStatus::Available) => Err(AuthError::conflict(
            "Board claim challenge is no longer available",
        )),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BoardClaimRequest {
    code: Option<String>,
}

/// POST /bootstrap/claim
///
/// 首次管理员认领：实例尚无实例管理员时把当前会话用户提升为管理员。
/// 仅在 `authenticated` + `private` 部署下可用（Paperclip `access.ts:2719-2742`）。
async fn bootstrap_claim(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<impl IntoResponse, AuthError> {
    if !browser_first_admin_claim_available() {
        return Err(AuthError::not_found(
            "Browser first-admin claim is not available",
        ));
    }
    let user_id = session_user_id(&actor)?;

    match claim_first_instance_admin(&state.pool, user_id).await? {
        FirstAdminClaimOutcome::AlreadyClaimed { .. } => Err(AuthError::conflict(
            "Someone else has already claimed this instance",
        )),
        FirstAdminClaimOutcome::Claimed { user_id } => {
            Ok((StatusCode::OK, Json(json!({ "claimed": true, "userId": user_id }))))
        }
    }
}

/// `POST /bootstrap/claim` 可用性：需 `DEPLOYMENT_MODE=authenticated` 且
/// `DEPLOYMENT_EXPOSURE=private`（未设置时按 private 处理）。
fn browser_first_admin_claim_available() -> bool {
    let mode = std::env::var("DEPLOYMENT_MODE").unwrap_or_else(|_| "local_trusted".to_string());
    let exposure = std::env::var("DEPLOYMENT_EXPOSURE").unwrap_or_else(|_| "private".to_string());
    mode == "authenticated" && exposure == "private"
}

/// 从认证 actor 提取 Board 用户 ID（非 Board 用户返回 401）。
fn actor_user_id(actor: &AuthorizationActor) -> Result<Uuid, AuthError> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        _ => Err(AuthError::unauthenticated("Board user authentication required")),
    }
}

/// 提取浏览器会话的 Board 用户 ID。
///
/// Board 认领必须由已登录的浏览器会话发起（Paperclip 要求
/// `actor.source === "session"`）：本地隐式身份或 API Key 都不具备"某个真实
/// 用户接管实例"的语义。
fn session_user_id(actor: &AuthorizationActor) -> Result<Uuid, AuthError> {
    match actor {
        AuthorizationActor::Board { user_id, source, .. }
            if *source == services::auth::ActorSource::Session =>
        {
            Ok(*user_id)
        }
        _ => Err(AuthError::unauthenticated(
            "Sign in from a browser session before claiming first admin",
        )),
    }
}

/// actor 是否为已登录的浏览器会话用户（Paperclip `isSignedInBoardUser`：
/// `source === "session" || isLocalImplicit`）。
fn is_signed_in_board_user(actor: &AuthorizationActor) -> bool {
    match actor {
        AuthorizationActor::Board { source, .. } => matches!(
            source,
            services::auth::ActorSource::Session | services::auth::ActorSource::LocalImplicit
        ),
        _ => false,
    }
}

/// actor 是否为本机隐式身份（Paperclip `isLocalImplicit`），可绕过实例管理员要求。
fn is_local_implicit(actor: &AuthorizationActor) -> bool {
    matches!(
        actor,
        AuthorizationActor::Board {
            source: services::auth::ActorSource::LocalImplicit,
            ..
        }
    )
}

// ============================================================================
// 阶段二：CLI 认证端点
// ============================================================================

/// POST /cli-auth/challenges
///
/// 创建 CLI 设备授权挑战。对齐 Paperclip `access.ts:2744-2765`：
/// 201 + 明文 `token`（挑战密钥）与 `boardApiToken`（待批准时落库的 key 明文），
/// 二者仅在本次响应中出现一次。
async fn create_cli_challenge_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateCliChallengeRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AuthError> {
    let requested_access = payload
        .requested_access
        .unwrap_or_else(|| "board".to_string());
    let created = create_cli_auth_challenge(
        &state.pool,
        payload.command,
        payload.client_name,
        requested_access,
        payload.requested_company_id,
    )
    .await?;

    let approval_path = format!(
        "/cli-auth/{}?token={}",
        created.id,
        urlencode_component(&created.token)
    );
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": created.id,
            "token": created.token,
            "boardApiToken": created.board_api_token,
            "approvalPath": approval_path,
            "approvalUrl": serde_json::Value::Null,
            "pollPath": format!("/cli-auth/challenges/{}", created.id),
            "expiresAt": created.expires_at,
            "suggestedPollIntervalMs": 1000,
        })),
    ))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCliChallengeRequest {
    command: String,
    client_name: Option<String>,
    requested_access: Option<String>,
    requested_company_id: Option<Uuid>,
}

/// 按 `application/x-www-form-urlencoded` 规则转义查询参数值，与 Paperclip 的
/// `encodeURIComponent` 对齐（用于构造 `approvalPath` 中的挑战密钥）。
fn urlencode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~'
            | b'*' | b'\'' | b'(' | b')' => encoded.push(byte as char),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// GET /cli-auth/challenges/:id?token=...
///
/// 查询挑战状态（供 CLI 轮询与 Board 审批页使用）。
/// 对齐 Paperclip `access.ts:2767-2791`：id 或 token 缺失、或 token 与
/// `secret_hash` 不匹配时返回 404。
async fn get_cli_challenge_handler(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Query(params): Query<CliChallengeQuery>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let token = params
        .token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AuthError::not_found("CLI auth challenge not found"))?;

    let view = describe_cli_auth_challenge(&state.pool, id, token).await?;

    let signed_in = is_signed_in_board_user(&actor);
    let can_approve = signed_in
        && (view.requested_access != "instance_admin_required"
            || is_local_implicit(&actor)
            || actor.is_instance_admin());

    Ok(Json(json!({
        "id": view.id,
        "status": view.status,
        "command": view.command,
        "clientName": view.client_name,
        "requestedAccess": view.requested_access,
        "requestedCompanyId": view.requested_company_id,
        "requestedCompanyName": view.requested_company_name,
        "approvedAt": view.approved_at,
        "cancelledAt": view.cancelled_at,
        "expiresAt": view.expires_at,
        "approvedByUser": view.approved_by_user.map(|u| json!({
            "id": u.id,
            "name": u.name,
            "email": u.email,
        })),
        "requiresSignIn": !signed_in,
        "canApprove": can_approve,
        "currentUserId": match &actor {
            AuthorizationActor::Board { user_id, .. } => json!(user_id),
            _ => serde_json::Value::Null,
        },
    })))
}

#[derive(Debug, Clone, Deserialize)]
struct CliChallengeQuery {
    token: Option<String>,
}

/// POST /cli-auth/challenges/:id/approve
///
/// 批准挑战：用挑战里预先落库的 `pending_key_hash` 创建 Board API Key，
/// 并把挑战标记为 approved。对齐 Paperclip `access.ts:2793-2844`。
async fn approve_cli_challenge_handler(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CliChallengeTokenRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    if !is_signed_in_board_user(&actor) {
        return Err(AuthError::unauthenticated(
            "Sign in before approving CLI access",
        ));
    }
    let user_id = actor_user_id(&actor)?;

    let outcome = approve_cli_auth_challenge(
        &state.pool,
        id,
        &payload.token,
        user_id,
        actor.is_instance_admin(),
    )
    .await?;

    Ok(Json(json!({
        "approved": outcome.approved,
        "status": outcome.status,
        "userId": outcome.user_id,
        "keyId": outcome.key_id,
        "expiresAt": outcome.expires_at,
    })))
}

#[derive(Debug, Clone, Deserialize)]
struct CliChallengeTokenRequest {
    token: String,
}

/// POST /cli-auth/challenges/:id/cancel
///
/// 取消挑战。对齐 Paperclip `access.ts:2846-2857`。
async fn cancel_cli_challenge_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CliChallengeTokenRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let outcome = cancel_cli_auth_challenge(&state.pool, id, &payload.token).await?;
    Ok(Json(json!({
        "status": outcome.status,
        "cancelled": outcome.cancelled,
    })))
}

/// GET /cli-auth/me
///
/// 返回当前 Board 身份及其公司成员关系。对齐 Paperclip `access.ts:2859-2873`。
async fn get_cli_auth_me(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let (user, memberships, is_admin) =
        services::auth::board_access::resolve_board_access(&state.pool, user_id).await?;

    Ok(Json(json!({
        "user": {
            "id": user.id,
            "email": user.email,
            "name": user.name,
            "image": user.avatar_url,
        },
        "userId": user_id,
        "isInstanceAdmin": is_admin,
        "companyIds": memberships.iter().map(|m| m.company_id).collect::<Vec<_>>(),
        "memberships": memberships.iter().map(|m| json!({
            "companyId": m.company_id,
            "membershipRole": m.role,
            "status": m.status,
        })).collect::<Vec<_>>(),
        "source": match &actor {
            AuthorizationActor::Board { source, .. } => json!(source),
            _ => json!("none"),
        },
        // 仅 board_key 来源携带 keyId（Paperclip `access.ts:2869`）
        "keyId": actor.key_id(),
    })))
}

/// POST /cli-auth/revoke-current
///
/// 撤销**当前**这把 Board API Key（CLI 登出）。对齐 Paperclip
/// `access.ts:2964-2992`：仅当 actor 由 Board API Key 认证时可用，
/// 响应为 `{revoked:true, keyId}`。
async fn revoke_current_cli_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AuthError> {
    if !matches!(
        &actor,
        AuthorizationActor::Board {
            source: services::auth::ActorSource::BoardKey,
            ..
        }
    ) {
        return Err(AuthError::bad_request(
            "Current board API key context is required",
        ));
    }
    let user_id = actor_user_id(&actor)?;
    let key_id = actor
        .key_id()
        .ok_or_else(|| AuthError::internal("Board key actor is missing its key id"))?;

    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(
        state.pool.clone(),
    );
    repo.revoke(key_id, user_id).await.map_err(|e| {
        AuthError::internal(format!("Failed to revoke API key: {}", e))
    })?;

    crate::routes::log_activity(
        &state.pool,
        uuid::Uuid::nil(),
        "auth.board_api_key_revoked",
        &actor,
        "board_api_key",
        key_id,
        json!({ "revokedVia": "cli_auth_logout" }),
    )
    .await;

    Ok(Json(json!({ "revoked": true, "keyId": key_id })))
}

// ============================================================================
// 阶段二：Board API Key 管理端点
// ============================================================================

/// GET /api/board-api-keys
///
/// 列出当前用户签发的 Board API Keys。对齐 Paperclip `access.ts:2874-2882`：
/// 默认只返回仍有效的 Key，`?includeInactive=true` 时连同已撤销/已过期的一并返回。
/// 响应不含密钥哈希，但**包含** `revokedAt`（前端据此区分状态）。
async fn list_board_api_keys(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Query(query): Query<ListBoardApiKeysQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    let keys = repo
        .list_by_user(user_id, query.include_inactive.unwrap_or(false))
        .await
        .map_err(|e| AuthError::internal(format!("Failed to list API keys: {}", e)))?;

    Ok(Json(keys.into_iter().map(board_api_key_json).collect()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListBoardApiKeysQuery {
    include_inactive: Option<bool>,
}

/// Board API Key 的对外 JSON 视图（对齐 Paperclip `listBoardApiKeys` 的投影）。
///
/// Paperclip 只投影 `{id,name,createdAt,lastUsedAt,revokedAt,expiresAt}`；
/// Parrot 多出的 `key_prefix` 展示列**不**出现在响应里（前端零消费）。
fn board_api_key_json(key: repositories::models::auth_keys::BoardApiKey) -> serde_json::Value {
    json!({
        "id": key.id,
        "name": key.name,
        "lastUsedAt": key.last_used_at,
        "revokedAt": key.revoked_at,
        "expiresAt": key.expires_at,
        "createdAt": key.created_at,
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBoardApiKeyRequest {
    // Paperclip `createBoardApiKeySchema`：name 默认 `paperclipai cli`，1..=120 字符。
    #[serde(default = "default_board_api_key_name")]
    name: String,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<Utc>>,
    #[serde(default)]
    requested_company_id: Option<Uuid>,
}

fn default_board_api_key_name() -> String {
    services::auth::cli_auth::DEFAULT_KEY_LABEL.to_string()
}

/// POST /api/board-api-keys
///
/// 手工签发一把 Board API Key。对齐 Paperclip `access.ts:2884-2929`：
/// `name` 可省略（默认 `paperclipai cli`），`expiresAt` 省略时给 30 天，
/// 成功返回 **201** 与仅此一次的明文 `token`。
async fn create_board_api_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Json(payload): Json<CreateBoardApiKeyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AuthError> {
    let user_id = actor_user_id(&actor)?;

    let name = payload.name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(AuthError::bad_request(
            "Key name must be between 1 and 120 characters",
        ));
    }
    if let Some(company_id) = payload.requested_company_id {
        crate::routes::assert_company_access(&actor, company_id, true)
            .map_err(|_| AuthError::forbidden("Insufficient permissions"))?;
    }

    let key = issue_board_api_key(
        &state.pool,
        user_id,
        name,
        payload.expires_at,
    )
    .await?;

    log_board_api_key_activity(
        &state.pool,
        &actor,
        user_id,
        "board_api_key.created",
        json!({
            "boardApiKeyId": key.id,
            "name": key.name,
            "requestedCompanyId": payload.requested_company_id,
            "expiresAt": key.expires_at,
        }),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": key.id,
            "name": key.name,
            "token": key.token,
            "lastUsedAt": key.last_used_at,
            "revokedAt": key.revoked_at,
            "expiresAt": key.expires_at,
            "createdAt": key.created_at,
        })),
    ))
}

/// DELETE /api/board-api-keys/:key_id
///
/// 撤销一把 Board API Key。对齐 Paperclip `access.ts:2930-2960`：
/// 非本人名下的 Key 与不存在的 Key 一律 404，成功返回 `{ok:true, keyId}`。
async fn revoke_board_api_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    repo.revoke(key_id, user_id).await.map_err(|e| match e {
        repositories::board_api_key_repository::RepositoryError::NotFound(_) => {
            AuthError::not_found("Board API key not found")
        }
        other => AuthError::internal(format!("Failed to revoke API key: {}", other)),
    })?;

    log_board_api_key_activity(
        &state.pool,
        &actor,
        user_id,
        "board_api_key.revoked",
        json!({ "boardApiKeyId": key_id, "revokedVia": "board_api_key_lifecycle" }),
    )
    .await;

    Ok(Json(json!({ "ok": true, "keyId": key_id })))
}

/// 新签发的 Key（含仅此一次的明文 `token`）。
///
/// 与 Paperclip `createNamedBoardApiKey` 的返回一致：不含 `key_prefix`
/// （Parrot 独有的展示列，对外投影里已一律剔除）。
struct IssuedBoardApiKey {
    id: Uuid,
    name: String,
    token: String,
    last_used_at: Option<chrono::DateTime<Utc>>,
    revoked_at: Option<chrono::DateTime<Utc>>,
    expires_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
}

/// 生成并落库一把 Board API Key。
///
/// token 明文形如 `pcp_board_<48 hex>`（对齐 Paperclip `createBoardApiToken`），
/// 服务端只保存 SHA-256 哈希；`expires_at` 省略时按 30 天计算。
async fn issue_board_api_key(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    name: &str,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<IssuedBoardApiKey, AuthError> {
    use repositories::board_api_key_repository::{
        create_board_api_token, hash_api_key, BoardApiKeyRepository, PgBoardApiKeyRepository,
        KEY_PREFIX_DISPLAY_LEN,
    };
    use services::auth::cli_auth::BOARD_API_KEY_TTL_DAYS;

    let token = create_board_api_token();
    let key_hash = hash_api_key(&token);
    let key_prefix: String = token.chars().take(KEY_PREFIX_DISPLAY_LEN).collect();
    let expires_at = Some(
        expires_at.unwrap_or_else(|| Utc::now() + chrono::Duration::days(BOARD_API_KEY_TTL_DAYS)),
    );

    let repo = PgBoardApiKeyRepository::new(pool.clone());
    let key = repo
        .create(user_id, name.to_string(), key_hash, key_prefix, expires_at)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to create API key: {}", e)))?;

    Ok(IssuedBoardApiKey {
        id: key.id,
        name: key.name,
        token,
        last_used_at: key.last_used_at,
        revoked_at: key.revoked_at,
        expires_at: key.expires_at,
        created_at: key.created_at,
    })
}

/// 写一条 Board API Key 生命周期 activity（对齐 Paperclip `resolveBoardActivityCompanyIds`）。
///
/// Paperclip 会按「用户活跃成员公司 → requestedCompanyId → 挑战公司 → 全部公司」的
/// 顺序解析活动归属公司并**逐公司**写入；`activity_logs.company_id` 为 NOT NULL，
/// 用户暂无成员关系时退化为 nil UUID（全局活动）。
async fn log_board_api_key_activity(
    pool: &sqlx::PgPool,
    actor: &AuthorizationActor,
    user_id: Uuid,
    event_type: &str,
    details: serde_json::Value,
) {
    let company_ids = match services::auth::board_access::resolve_board_access(pool, user_id).await {
        Ok((_, memberships, is_instance_admin)) => {
            let active: Vec<Uuid> = memberships
                .iter()
                .filter(|m| m.status.is_active())
                .map(|m| m.company_id)
                .collect();
            if !active.is_empty() {
                active
            } else if is_instance_admin {
                sqlx::query_scalar("SELECT id FROM companies")
                    .fetch_all(pool)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    if company_ids.is_empty() {
        crate::routes::log_activity(
            pool,
            Uuid::nil(),
            event_type,
            actor,
            "user",
            user_id,
            details,
        )
        .await;
        return;
    }

    for company_id in company_ids {
        crate::routes::log_activity(
            pool,
            company_id,
            event_type,
            actor,
            "user",
            user_id,
            details.clone(),
        )
        .await;
    }
}

// ============================================================================
// 阶段二：邀请管理端点
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInviteRequest {
    #[serde(default = "default_invite_type")]
    invite_type: String,
    email: Option<String>,
    allowed_join_types: Option<String>,
    ttl_hours: Option<i64>,
    human_role: Option<String>,
    defaults_payload: Option<serde_json::Value>,
    #[serde(alias = "agentMessage", alias = "invite_message")]
    invite_message: Option<String>,
}

fn default_invite_type() -> String {
    "company_join".to_string()
}

/// Build a public URL when the deployment advertises one, otherwise keep the
/// relative path usable by local development and reverse proxies.
fn public_url(path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let base = std::env::var("PAPERCLIP_PUBLIC_URL")
        .ok()
        .or_else(|| std::env::var("PARROT_PUBLIC_URL").ok())
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty());
    base.map(|value| format!("{value}{path}")).unwrap_or(path)
}

fn invite_resource_urls(token: &str) -> serde_json::Value {
    let logo_path = format!("/api/invites/{token}/logo");
    let onboarding_path = format!("/api/invites/{token}/onboarding");
    let onboarding_text_path = format!("/api/invites/{token}/onboarding.txt");
    let skill_index_path = format!("/api/invites/{token}/skills/index");
    json!({
        "companyLogoUrl": public_url(&logo_path),
        "onboardingPath": onboarding_path,
        "onboardingUrl": public_url(&onboarding_path),
        "onboardingTextPath": onboarding_text_path,
        "onboardingTextUrl": public_url(&onboarding_text_path),
        "skillIndexPath": skill_index_path,
        "skillIndexUrl": public_url(&skill_index_path),
    })
}

/// GET /api/companies/:company_id/invites
/// 列出公司的邀请（对齐 Paperclip `listCompanyInvitesQuerySchema`）。
#[derive(Debug, Deserialize)]
struct ListCompanyInvitesQuery {
    /// active | accepted | expired | revoked
    #[serde(rename = "state")]
    state: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// GET /api/companies/:company_id/invites
/// 列出邀请；需 `users:invite` 权限（对齐 Paperclip assertCompanyPermission）。
async fn list_company_invites(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(query): Query<ListCompanyInvitesQuery>,
) -> Result<Json<serde_json::Value>, AuthError> {
    actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("users:invite") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    // 与 Paperclip `inviteStateWhereClause` 保持一致的四个状态；
    // `expires_at` 为 NOT NULL（migrations/00_init_schema_unified.sql:1074），
    // 因此不再需要 IS NOT NULL 兜底。
    let (state_sql, has_state) = match query.state.as_deref() {
        Some("active") => (
            "(revoked_at IS NULL AND accepted = false AND expires_at > NOW())".to_string(),
            true,
        ),
        Some("accepted") => ("accepted = true".to_string(), true),
        Some("expired") => (
            "(revoked_at IS NULL AND accepted = false AND expires_at <= NOW())".to_string(),
            true,
        ),
        Some("revoked") => ("revoked_at IS NOT NULL".to_string(), true),
        Some(other) => return Err(AuthError::bad_request(format!("Invalid invite state: {}", other))),
        None => (String::new(), false),
    };

    use sqlx::Row;
    let sql = format!(
        "SELECT i.id, i.company_id, c.name AS company_name,
                i.invite_type::text AS invite_type, i.invited_email,
                i.invited_by_user_id, i.allowed_join_types::text AS allowed_join_types,
                i.human_role::text AS human_role, i.defaults_payload,
                i.invite_message, i.accepted, i.accepted_at, i.revoked_at,
                i.expires_at, i.created_at, i.updated_at,
                CASE WHEN i.revoked_at IS NOT NULL THEN 'revoked'
                     WHEN i.accepted = true THEN 'accepted'
                     WHEN i.expires_at <= NOW() THEN 'expired'
                     ELSE 'active' END AS state,
                (SELECT j.id FROM join_requests j
                 WHERE j.invite_id = i.id
                 ORDER BY j.created_at DESC LIMIT 1) AS related_join_request_id,
                u.id AS invited_by_id, u.email AS invited_by_email,
                u.name AS invited_by_name, u.avatar_url AS invited_by_image
         FROM invites i
         JOIN companies c ON c.id = i.company_id
         LEFT JOIN auth_users u ON u.id = i.invited_by_user_id
         WHERE i.company_id = $1 {} ORDER BY i.created_at DESC LIMIT $2 OFFSET $3",
        if has_state { format!("AND {}", state_sql.replace("revoked_at", "i.revoked_at").replace("accepted", "i.accepted").replace("expires_at", "i.expires_at")) } else { String::new() }
    );
    let rows = sqlx::query(&sql)
        .bind(company_id)
        .bind(limit + 1)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to list invites: {}", e)))?;

    let has_more = rows.len() as i64 > limit;
    let visible = if has_more { &rows[..limit as usize] } else { &rows[..] };
    let items: Vec<serde_json::Value> = visible
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Uuid, _>("id"),
                "companyId": company_id,
                "companyName": r.get::<String, _>("company_name"),
                "inviteType": r.get::<String, _>("invite_type"),
                "allowedJoinTypes": r.get::<String, _>("allowed_join_types"),
                "humanRole": r.get::<Option<String>, _>("human_role"),
                "defaultsPayload": r.get::<Option<serde_json::Value>, _>("defaults_payload"),
                "inviteMessage": r.get::<Option<String>, _>("invite_message"),
                "invitedByUserId": r.get::<Option<Uuid>, _>("invited_by_user_id"),
                "invitedByUser": access_user_value(
                    r,
                    "invited_by_id",
                    "invited_by_email",
                    "invited_by_name",
                    "invited_by_image",
                ),
                "expiresAt": r.get::<chrono::DateTime<Utc>, _>("expires_at"),
                "revokedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("revoked_at"),
                "acceptedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("accepted_at"),
                "createdAt": r.get::<chrono::DateTime<Utc>, _>("created_at"),
                "updatedAt": r.get::<chrono::DateTime<Utc>, _>("updated_at"),
                "state": r.get::<String, _>("state"),
                "relatedJoinRequestId": r.get::<Option<Uuid>, _>("related_join_request_id"),
            })
        })
        .collect();

    Ok(Json(json!({
        "invites": items,
        "nextOffset": has_more.then_some(offset + limit),
    })))
}

/// 邀请行的 Paperclip 风格 JSON 视图。
fn invite_row_json(
    r: &sqlx::postgres::PgRow,
    company_id: Uuid,
) -> Result<Json<serde_json::Value>, AuthError> {
    use sqlx::Row;
    Ok(Json(json!({
        "id": r.get::<Uuid, _>("id"),
        "companyId": company_id,
        "inviteType": r.get::<String, _>("invite_type"),
        "invitedEmail": r.get::<Option<String>, _>("invited_email"),
        "invitedByUserId": r.get::<Option<Uuid>, _>("invited_by_user_id"),
        "allowedJoinTypes": r.get::<Option<String>, _>("allowed_join_types"),
        "accepted": r.get::<bool, _>("accepted"),
        "acceptedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("accepted_at"),
        "revokedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("revoked_at"),
        "expiresAt": r.get::<Option<chrono::DateTime<Utc>>, _>("expires_at"),
        "createdAt": r.get::<chrono::DateTime<Utc>, _>("created_at"),
    })))
}

/// POST /api/invites/:invite_id/revoke
/// 撤销邀请（对齐 Paperclip `/invites/:inviteId/revoke`）。
async fn revoke_invite(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT id, company_id, invite_type::text, invited_email, invited_by_user_id, \
         allowed_join_types::text, accepted, accepted_at, revoked_at, expires_at, created_at \
         FROM invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?;
    let Some(r) = row else {
        return Err(AuthError::bad_request("Invite not found"));
    };

    let company_id: Uuid = r.get("company_id");
    let invite_type: String = r.get("invite_type");
    let accepted_at: Option<chrono::DateTime<Utc>> = r.get("accepted_at");
    let revoked_at: Option<chrono::DateTime<Utc>> = r.get("revoked_at");

    if invite_type == "bootstrap_ceo" {
        assert_instance_admin(&actor)?;
    } else {
        let allowed = decide_access(
            &state.pool,
            &actor,
            &AuthorizationAction::Permission { key: PermissionKey::new("users:invite") },
            Some(company_id),
        )
        .await;
        if !allowed {
            return Err(AuthError::forbidden("Insufficient permissions"));
        }
    }
    if accepted_at.is_some() {
        return Err(AuthError::conflict("Invite already consumed"));
    }
    if revoked_at.is_some() {
        // 幂等：已撤销直接返回当前状态
        return invite_row_json(&r, company_id);
    }

    sqlx::query("UPDATE invites SET revoked_at = NOW() WHERE id = $1")
        .bind(invite_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to revoke invite: {}", e)))?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "invite.revoked",
        &actor,
        "invite",
        invite_id,
        json!({}),
    )
    .await;

    let refreshed = sqlx::query(
        "SELECT id, company_id, invite_type::text, invited_email, invited_by_user_id, \
         allowed_join_types::text, accepted, accepted_at, revoked_at, expires_at, created_at \
         FROM invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to reload invite: {}", e)))?;
    invite_row_json(&refreshed, company_id)
}

#[derive(Debug, Deserialize)]
struct TestResolutionQuery {
    url: Option<String>,
    #[serde(rename = "timeoutMs")]
    timeout_ms: Option<i64>,
}

struct InviteProbeResult {
    ok: bool,
    status_code: Option<u16>,
    latency_ms: Option<u64>,
    error: Option<String>,
}

async fn probe_invite_url(url: url::Url, timeout_ms: i64) -> InviteProbeResult {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms as u64))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return InviteProbeResult { ok: false, status_code: None, latency_ms: None, error: Some(e.to_string()) };
        }
    };
    let start = std::time::Instant::now();
    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let latency = start.elapsed().as_millis() as u64;
            InviteProbeResult {
                ok: status < 500,
                status_code: Some(status),
                latency_ms: Some(latency),
                error: None,
            }
        }
        Err(e) => InviteProbeResult { ok: false, status_code: None, latency_ms: None, error: Some(e.to_string()) },
    }
}

/// GET /api/invites/:token/test-resolution
/// 探测邀请目标的解析可达性（对齐 Paperclip `/invites/:token/test-resolution`）。
async fn get_invite_test_resolution(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<TestResolutionQuery>,
) -> Result<Json<serde_json::Value>, AuthError> {
    use sqlx::Row;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AuthError::bad_request("Invite not found"));
    }
    let row = sqlx::query("SELECT id, revoked_at, expires_at FROM invites WHERE token = $1")
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?;
    let Some(r) = row else {
        return Err(AuthError::bad_request("Invite not found"));
    };
    let revoked_at: Option<chrono::DateTime<Utc>> = r.get("revoked_at");
    let expires_at: Option<chrono::DateTime<Utc>> = r.get("expires_at");
    if revoked_at.is_some() || expires_at.map(|e| Utc::now() > e).unwrap_or(false) {
        return Err(AuthError::bad_request("Invite not found"));
    }
    let invite_id: Uuid = r.get("id");

    let raw_url = query.url.clone().unwrap_or_default();
    if raw_url.trim().is_empty() {
        return Err(AuthError::bad_request("url query parameter is required"));
    }
    let url = url::Url::parse(raw_url.trim())
        .map_err(|_| AuthError::bad_request("url must be an absolute http(s) URL"))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AuthError::bad_request("url must use http or https"));
    }
    let timeout_ms = query.timeout_ms.unwrap_or(5000).clamp(1000, 15000);

    let probe = probe_invite_url(url.clone(), timeout_ms).await;
    Ok(Json(json!({
        "inviteId": invite_id,
        "testResolutionPath": format!("/api/invites/{}/test-resolution", token),
        "requestedUrl": url.to_string(),
        "timeoutMs": timeout_ms,
        "ok": probe.ok,
        "statusCode": probe.status_code,
        "latencyMs": probe.latency_ms,
        "error": probe.error,
    })))
}

/// POST /api/companies/:company_id/invites
/// 创建邀请。
async fn create_invite(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<CreateInviteRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;

    // Permission check: users:invite
    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("users:invite") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions to create invites"));
    }

    let invite_type = match payload.invite_type.as_str() {
        "company_join" => "company_join",
        "bootstrap_ceo" => "bootstrap_ceo",
        _ => return Err(AuthError::bad_request("Invalid invite type")),
    };

    let allowed_join_types = match payload.allowed_join_types.as_deref() {
        Some("human") => "human",
        Some("agent") => "agent",
        Some("both") | None => "both",
        Some(_) => return Err(AuthError::bad_request("Invalid allowed_join_types")),
    };
    if let Some(role) = payload.human_role.as_deref() {
        if !matches!(role, "owner" | "admin" | "operator" | "viewer") {
            return Err(AuthError::bad_request(
                "humanRole must be owner, admin, operator, or viewer",
            ));
        }
    }

    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let now = Utc::now();
    let expires_at = now + chrono::Duration::hours(payload.ttl_hours.unwrap_or(72));

    // Agent invites 不需要 email；human invites 需要 email
    let invited_email = if allowed_join_types == "agent" {
        None
    } else {
        payload.email.as_ref().or(Some(&"".to_string())).cloned()
    };

    // Store invite in database using raw SQL
    // 注意：invite_type 和 allowed_join_types 是数据库枚举类型，需要显式转换
    let invite_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invites (id, company_id, invite_type, invited_by_user_id, invited_email, token,
           allowed_join_types, human_role, defaults_payload, invite_message,
           expires_at, accepted, created_at, updated_at)
           VALUES ($1, $2, $3::invite_type, $4, $5, $6, $7::allowed_join_types,
                   $8::membership_role, $9, $10, $11, false, $12, $12)"#
    )
    .bind(invite_id)
    .bind(company_id)
    .bind(invite_type)
    .bind(user_id)
    .bind(&invited_email)
    .bind(&token)
    .bind(allowed_join_types)
    .bind(payload.human_role.as_deref())
    .bind(payload.defaults_payload.clone())
    .bind(payload.invite_message.clone())
    .bind(expires_at)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to create invite: {}", e)))?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.invite_created",
        &actor,
        "invite",
        invite_id,
        json!({ "inviteType": invite_type, "email": invited_email }),
    )
    .await;

    Ok(Json(json!({
        "id": invite_id,
        "companyId": company_id,
        "token": token,
        "inviteUrl": public_url(&format!("/invite/{token}")),
        "inviteType": invite_type,
        "allowedJoinTypes": allowed_join_types,
        "humanRole": payload.human_role,
        "defaultsPayload": payload.defaults_payload,
        "inviteMessage": payload.invite_message,
        "expiresAt": expires_at,
        "createdAt": now,
        "updatedAt": now,
        "onboardingTextPath": format!("/api/invites/{token}/onboarding.txt"),
        "onboardingTextUrl": public_url(&format!("/api/invites/{token}/onboarding.txt")),
    })))
}

/// GET /api/invites/:token
/// 获取邀请详情。
async fn get_invite(
    actor: Option<Extension<AuthorizationActor>>,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    use sqlx::Row;

    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AuthError::bad_request("Invite not found"));
    }

    let row = sqlx::query(
        r#"SELECT i.id, i.company_id, i.invite_type::text AS invite_type,
                  i.allowed_join_types::text AS allowed_join_types,
                  i.human_role::text AS human_role, i.invite_message,
                  i.accepted, i.revoked_at, i.expires_at, i.created_at, i.updated_at,
                  c.name AS company_name, c.brand_color, c.logo_asset_id,
                  inviter.name AS invited_by_name
           FROM invites i
           JOIN companies c ON c.id = i.company_id
           LEFT JOIN auth_users inviter ON inviter.id = i.invited_by_user_id
           WHERE i.token = $1"#
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?
    .ok_or_else(|| AuthError::bad_request("Invite not found"))?;

    let invite_id = row.get::<Uuid, _>("id");
    let company_id = row.get::<Uuid, _>("company_id");
    let invite_type = row.get::<String, _>("invite_type");
    let allowed_join_types = row.get::<String, _>("allowed_join_types");
    let human_role = row.get::<Option<String>, _>("human_role");
    let invite_message = row.get::<Option<String>, _>("invite_message");
    let accepted = row.get::<bool, _>("accepted");
    let revoked_at = row.get::<Option<chrono::DateTime<Utc>>, _>("revoked_at");
    let expires_at = row.get::<chrono::DateTime<Utc>, _>("expires_at");
    let has_company_logo = row.get::<Option<Uuid>, _>("logo_asset_id").is_some();

    if revoked_at.is_some() {
        return Err(AuthError::bad_request("Invite has been revoked"));
    }

    if Utc::now() > expires_at {
        return Err(AuthError::bad_request("Invite has expired"));
    }

    // The invite landing page must be able to rehydrate an already accepted
    // invite for the user who submitted its join request. Anonymous callers
    // (or other users) still see a single-use invite as unavailable.
    let current_user_id = actor.and_then(|Extension(actor)| match actor {
        AuthorizationActor::Board { user_id, .. } => Some(user_id),
        _ => None,
    });
    let join_request = if let Some(user_id) = current_user_id {
        sqlx::query_as::<_, (String, String)>(
            "SELECT status::text, request_type
             FROM join_requests
             WHERE invite_id = $1 AND requester_user_id = $2
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(invite_id)
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load invite join request: {}", e)))?
    } else {
        None
    };
    if accepted && join_request.is_none() {
        return Err(AuthError::bad_request("Invite has already been used"));
    }

    let resource_urls = invite_resource_urls(&token);
    let company_logo_url = if has_company_logo {
        resource_urls["companyLogoUrl"].clone()
    } else {
        serde_json::Value::Null
    };
    let join_request_status = join_request.as_ref().map(|(status, _)| status);
    let join_request_type = join_request.as_ref().map(|(_, request_type)| request_type);

    let mut response = json!({
        "id": invite_id,
        "companyId": company_id,
        "companyName": row.get::<String, _>("company_name"),
        "companyLogoUrl": company_logo_url,
        "companyBrandColor": row.get::<Option<String>, _>("brand_color"),
        "inviteType": invite_type,
        "allowedJoinTypes": allowed_join_types,
        "humanRole": human_role,
        "inviteMessage": invite_message,
        "invitedByUserName": row.get::<Option<String>, _>("invited_by_name"),
        "expiresAt": expires_at,
        "createdAt": row.get::<chrono::DateTime<Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<Utc>, _>("updated_at"),
        "joinRequestStatus": join_request_status,
        "joinRequestType": join_request_type,
    });
    if let Some(object) = response.as_object_mut() {
        if let Some(resource_object) = resource_urls.as_object() {
            for (key, value) in resource_object {
                if key != "companyLogoUrl" {
                    object.insert(key.clone(), value.clone());
                }
            }
        }
    }
    Ok(Json(response))
}

/// POST /api/invites/:token/accept
/// 接受邀请。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptInviteRequest {
    request_type: Option<String>,
    agent_name: Option<String>,
    adapter_type: Option<String>,
    capabilities: Option<String>,
    agent_defaults_payload: Option<serde_json::Value>,
    responses_webhook_url: Option<String>,
    responses_webhook_method: Option<String>,
    responses_webhook_headers: Option<serde_json::Value>,
    paperclip_api_url: Option<String>,
    webhook_auth_header: Option<String>,
}

fn request_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_default()
        .to_string()
}

async fn accept_invite(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    body: Option<Json<AcceptInviteRequest>>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let payload = body.map(|Json(payload)| payload).unwrap_or_default();
    let request_type = payload
        .request_type
        .as_deref()
        .unwrap_or("human")
        .to_string();
    if !matches!(request_type.as_str(), "human" | "agent") {
        return Err(AuthError::bad_request(
            "requestType must be human or agent",
        ));
    }

    // Find invite
    use sqlx::Row;
    let invite = sqlx::query(
        r#"SELECT id, company_id, invite_type::text AS invite_type,
                  allowed_join_types::text AS allowed_join_types,
                  human_role::text AS human_role, defaults_payload, invite_message,
                  accepted, revoked_at, expires_at, invited_email
           FROM invites WHERE token = $1"#
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?;

    let invite = invite.ok_or_else(|| AuthError::bad_request("Invite not found"))?;
    let invite_id = invite.get::<Uuid, _>("id");
    let company_id = invite.get::<Option<Uuid>, _>("company_id");
    let invite_type = invite.get::<String, _>("invite_type");
    let allowed_join_types = invite.get::<String, _>("allowed_join_types");
    let defaults_payload = invite.get::<Option<serde_json::Value>, _>("defaults_payload");
    let invite_message = invite.get::<Option<String>, _>("invite_message");
    let accepted = invite.get::<bool, _>("accepted");
    let revoked_at = invite.get::<Option<chrono::DateTime<Utc>>, _>("revoked_at");
    let expires_at = invite.get::<chrono::DateTime<Utc>, _>("expires_at");
    let invited_email = invite.get::<Option<String>, _>("invited_email");

    if revoked_at.is_some() {
        return Err(AuthError::bad_request("Invite has been revoked"));
    }

    if Utc::now() > expires_at {
        return Err(AuthError::bad_request("Invite has expired"));
    }

    if invite_type == "bootstrap_ceo" && request_type != "human" {
        return Err(AuthError::bad_request(
            "Bootstrap invites only accept human requests",
        ));
    }
    if request_type == "human" && allowed_join_types == "agent" {
        return Err(AuthError::bad_request("This invite only accepts agents"));
    }
    if request_type == "agent" && allowed_join_types == "human" {
        return Err(AuthError::bad_request("This invite only accepts human users"));
    }

    // An accepted invite can be retried by the same requester to rehydrate its
    // complete join request. This is how the landing page resumes after a
    // refresh or after an approver changes a pending request to approved.
    let existing_request: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status::text
         FROM join_requests
         WHERE invite_id = $1 AND requester_user_id = $2
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(invite_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to load existing join request: {}", e)))?;
    if let Some((request_id, _status)) = existing_request {
        let company_id = company_id.ok_or_else(|| {
            AuthError::internal("Bootstrap invite cannot have a persisted join request")
        })?;
        let request = load_join_requests(&state.pool, company_id, None, None, Some(request_id))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AuthError::internal("Existing join request disappeared"))?;
        return Ok(Json(request));
    }
    if accepted {
        return Err(AuthError::bad_request("Invite has already been used"));
    }

    if request_type == "human" && invite_type == "bootstrap_ceo" {
        match services::auth::board_claim::claim_first_instance_admin(&state.pool, user_id).await? {
            services::auth::board_claim::FirstAdminClaimOutcome::AlreadyClaimed { .. } => {
                return Err(AuthError::conflict(
                    "The instance bootstrap invite has already been used",
                ));
            }
            services::auth::board_claim::FirstAdminClaimOutcome::Claimed { .. } => {}
        }

        if let Some(company_id) = company_id {
            sqlx::query(
                "INSERT INTO company_memberships
                 (company_id, principal_type, principal_id, membership_role, status, created_at, updated_at)
             VALUES ($1, 'user'::principal_type, $2, 'owner'::membership_role,
                     'active'::company_membership_status, NOW(), NOW())
             ON CONFLICT (company_id, principal_type, principal_id)
             DO UPDATE SET membership_role = 'owner'::membership_role,
                           status = 'active'::company_membership_status,
                           updated_at = NOW()",
        )
            .bind(company_id)
            .bind(user_id)
            .execute(&state.pool)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to create bootstrap membership: {}", e)))?;
        }

        let now = Utc::now();
        sqlx::query(
            "UPDATE invites
             SET accepted = true, accepted_by_user_id = $1, accepted_at = $2, updated_at = $2
             WHERE id = $3",
        )
        .bind(user_id)
        .bind(now)
        .bind(invite_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to mark bootstrap invite as used: {}", e)))?;

        if let Some(company_id) = company_id {
            crate::routes::log_activity(
                &state.pool,
                company_id,
                "company.bootstrap_invite_accepted",
                &actor,
                "invite",
                invite_id,
                json!({ "userId": user_id }),
            )
            .await;
        }

        return Ok(Json(json!({
            "bootstrapAccepted": true,
            "userId": user_id,
        })));
    }

    let company_id = company_id.ok_or_else(|| {
        AuthError::bad_request("Company join invite is missing a company")
    })?;
    let requester_email = sqlx::query_scalar::<_, String>(
        "SELECT email FROM auth_users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to load requester: {}", e)))?;
    let now = Utc::now();
    let ip = request_ip(&headers);
    let message = format!("Accepted via invite {}", token);

    let (
        created_agent_id,
        claim_secret,
        claim_secret_hash,
        claim_secret_expires_at,
        request_agent_name,
        request_adapter_type,
        request_agent_defaults_payload,
    ) =
        if request_type == "agent" {
            let agent_name = payload
                .agent_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 120)
                .ok_or_else(|| {
                    AuthError::bad_request("agentName must be 1-120 characters")
                })?
                .to_string();
            let adapter_type = payload
                .adapter_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.chars().count() <= 120)
                .unwrap_or("process")
                .to_string();
            if let Some(capabilities) = payload.capabilities.as_deref() {
                if capabilities.chars().count() > 4000 {
                    return Err(AuthError::bad_request(
                        "capabilities must be at most 4000 characters",
                    ));
                }
            }

            let mut agent_defaults = payload
                .agent_defaults_payload
                .clone()
                .or(defaults_payload.clone())
                .unwrap_or_else(|| json!({}));
            let defaults_object = agent_defaults.as_object_mut().ok_or_else(|| {
                AuthError::bad_request("agentDefaultsPayload must be an object")
            })?;
            if let Some(value) = payload.responses_webhook_url.clone() {
                defaults_object.insert("responsesWebhookUrl".to_string(), json!(value));
            }
            if let Some(value) = payload.responses_webhook_method.clone() {
                defaults_object.insert("responsesWebhookMethod".to_string(), json!(value));
            }
            if let Some(value) = payload.responses_webhook_headers.clone() {
                defaults_object.insert("responsesWebhookHeaders".to_string(), value);
            }
            if let Some(value) = payload.paperclip_api_url.clone() {
                defaults_object.insert("paperclipApiUrl".to_string(), json!(value));
            }
            if let Some(value) = payload.webhook_auth_header.clone() {
                defaults_object.insert("webhookAuthHeader".to_string(), json!(value));
            }
            let adapter_config = defaults_object
                .get("adapterConfig")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let runtime_config = defaults_object
                .get("runtimeConfig")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let agent_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO agents
                     (id, company_id, name, role, status, adapter_type,
                      adapter_config, runtime_config, permissions, metadata,
                      budget_monthly_cents, created_at, updated_at)
                   VALUES ($1, $2, $3, 'general', 'pending_approval', $4,
                           $5, $6,
                           '{"can_create_agents":false,"can_create_skills":false,
                             "trust_preset":"standard","authorization_policy":"manual"}'::jsonb,
                           '{}'::jsonb, 0, $7, $7)"#,
            )
            .bind(agent_id)
            .bind(company_id)
            .bind(&agent_name)
            .bind(&adapter_type)
            .bind(adapter_config)
            .bind(runtime_config)
            .bind(now)
            .execute(&state.pool)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to create invited agent: {}", e)))?;

            let raw_secret = format!("pclip_claim_{}", Uuid::new_v4().simple());
            let mut digest = Sha256::new();
            digest.update(raw_secret.as_bytes());
            let secret_hash = digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            (
                Some(agent_id),
                Some(raw_secret),
                Some(secret_hash),
                Some(now + chrono::Duration::days(7)),
                Some(agent_name),
                Some(adapter_type),
                Some(agent_defaults),
            )
        } else {
            (None, None, None, None, None, None, None)
        };

    // Create the canonical join request. Human and agent requests share one
    // record shape; agent-only fields remain null for human requests.
    let jr_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO join_requests
             (id, invite_id, company_id, requester_user_id, request_type,
              request_email_snapshot, request_ip, agent_name, adapter_type,
              capabilities, agent_defaults_payload, created_agent_id,
              claim_secret_hash, claim_secret_expires_at, status, message,
              reviewed_by_user_id, reviewed_at, rejection_reason, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   $13, $14, 'pending_approval', $15, NULL, NULL, NULL, $16, $16)"#,
    )
    .bind(jr_id)
    .bind(invite_id)
    .bind(company_id)
    .bind(user_id)
    .bind(&request_type)
    .bind(requester_email.or(invited_email))
    .bind(&ip)
    .bind(request_agent_name)
    .bind(request_adapter_type)
    .bind(payload.capabilities.as_deref())
    .bind(request_agent_defaults_payload)
    .bind(created_agent_id)
    .bind(claim_secret_hash)
    .bind(claim_secret_expires_at)
    .bind(&message)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to create join request: {}", e)))?;

    // Mark invite as accepted
    sqlx::query(
        "UPDATE invites
         SET accepted = true, accepted_by_user_id = $1, accepted_at = $2, updated_at = $2
         WHERE id = $3",
    )
        .bind(user_id)
        .bind(now)
        .bind(invite_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to mark invite as used: {}", e)))?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.invite_accepted",
        &actor,
        "invite",
        invite_id,
        json!({ "joinRequestId": jr_id }),
    )
    .await;

    let mut response = load_join_requests(&state.pool, company_id, None, None, Some(jr_id))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AuthError::internal("Created join request disappeared"))?;
    if let Some(object) = response.as_object_mut() {
        object.insert("joinRequestId".to_string(), json!(jr_id));
        if let Some(claim_secret) = claim_secret {
            object.insert("claimSecret".to_string(), json!(claim_secret));
            object.insert(
                "claimApiKeyPath".to_string(),
                json!(format!("/api/join-requests/{jr_id}/claim-api-key")),
            );
            object.insert(
                "onboarding".to_string(),
                json!({
                    "inviteMessage": invite_message,
                    "textInstructions": {
                        "url": public_url(&format!("/api/invites/{token}/onboarding.txt")),
                    },
                    "connectivity": {
                        "connectionCandidates": std::env::var("PAPERCLIP_PUBLIC_URL")
                            .ok()
                            .or_else(|| std::env::var("PARROT_PUBLIC_URL").ok())
                            .into_iter()
                            .collect::<Vec<_>>(),
                        "testResolutionEndpoint": {
                            "method": "GET",
                            "path": format!("/api/invites/{token}/test-resolution"),
                            "url": public_url(&format!(
                                "/api/invites/{token}/test-resolution"
                            )),
                        },
                    },
                }),
            );
        }
    }
    Ok(Json(response))
}

// ============================================================================
// 阶段二：加入请求管理端点
// ============================================================================

/// GET /api/companies/:company_id/join-requests
/// 列出公司的加入请求。
async fn list_join_requests(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<JoinRequestsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AuthError> {
    actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("joins:approve") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    Ok(Json(
        load_join_requests(
            &state.pool,
            company_id,
            params.status.as_deref(),
            params.request_type.as_deref(),
            None,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct JoinRequestsQuery {
    status: Option<String>,
    #[serde(rename = "requestType")]
    request_type: Option<String>,
}

fn access_user_value(
    row: &sqlx::postgres::PgRow,
    id_column: &str,
    email_column: &str,
    name_column: &str,
    image_column: &str,
) -> Option<serde_json::Value> {
    use sqlx::Row;
    let id = row.get::<Option<Uuid>, _>(id_column)?;
    Some(json!({
        "id": id,
        "email": row.get::<Option<String>, _>(email_column),
        "name": row.get::<Option<String>, _>(name_column),
        "image": row.get::<Option<String>, _>(image_column),
    }))
}

async fn load_join_requests(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    status: Option<&str>,
    request_type: Option<&str>,
    request_id: Option<Uuid>,
) -> Result<Vec<serde_json::Value>, AuthError> {
    use sqlx::Row;

    let rows = sqlx::query(
        "SELECT j.id, j.invite_id, j.company_id, j.request_type,
                j.status::text AS status, j.requester_user_id,
                j.request_email_snapshot, j.request_ip, j.agent_name,
                j.adapter_type, j.capabilities, j.agent_defaults_payload,
                j.claim_secret_expires_at, j.claim_secret_consumed_at,
                j.created_agent_id, j.reviewed_by_user_id AS approved_by_user_id,
                j.reviewed_at AS approved_at, j.rejected_by_user_id,
                j.rejected_at, j.message, j.created_at, j.updated_at,
                requester.id AS requester_id, requester.email AS requester_email,
                requester.name AS requester_name, requester.avatar_url AS requester_image,
                approver.id AS approver_id, approver.email AS approver_email,
                approver.name AS approver_name, approver.avatar_url AS approver_image,
                rejecter.id AS rejecter_id, rejecter.email AS rejecter_email,
                rejecter.name AS rejecter_name, rejecter.avatar_url AS rejecter_image,
                i.id AS invite_record_id, i.invite_type::text AS invite_type,
                i.allowed_join_types::text AS allowed_join_types,
                i.human_role::text AS human_role, i.invite_message,
                i.created_at AS invite_created_at, i.expires_at AS invite_expires_at,
                i.revoked_at AS invite_revoked_at, i.accepted_at AS invite_accepted_at,
                inviter.id AS inviter_id, inviter.email AS inviter_email,
                inviter.name AS inviter_name, inviter.avatar_url AS inviter_image
         FROM join_requests j
         LEFT JOIN auth_users requester ON requester.id = j.requester_user_id
         LEFT JOIN auth_users approver ON approver.id = j.reviewed_by_user_id
         LEFT JOIN auth_users rejecter ON rejecter.id = j.rejected_by_user_id
         LEFT JOIN invites i ON i.id = j.invite_id
         LEFT JOIN auth_users inviter ON inviter.id = i.invited_by_user_id
         WHERE j.company_id = $1
           AND ($2::text IS NULL OR j.status::text = $2)
           AND ($3::text IS NULL OR j.request_type = $3)
           AND ($4::uuid IS NULL OR j.id = $4)
         ORDER BY j.created_at DESC",
    )
    .bind(company_id)
    .bind(status)
    .bind(request_type)
    .bind(request_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list join requests: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let requester_id = row.get::<Option<Uuid>, _>("requester_user_id");
            let invite = row
                .get::<Option<Uuid>, _>("invite_record_id")
                .map(|id| {
                    json!({
                        "id": id,
                        "inviteType": row.get::<Option<String>, _>("invite_type"),
                        "allowedJoinTypes": row.get::<Option<String>, _>("allowed_join_types"),
                        "humanRole": row.get::<Option<String>, _>("human_role"),
                        "inviteMessage": row.get::<Option<String>, _>("invite_message"),
                        "createdAt": row.get::<Option<chrono::DateTime<Utc>>, _>("invite_created_at"),
                        "expiresAt": row.get::<Option<chrono::DateTime<Utc>>, _>("invite_expires_at"),
                        "revokedAt": row.get::<Option<chrono::DateTime<Utc>>, _>("invite_revoked_at"),
                        "acceptedAt": row.get::<Option<chrono::DateTime<Utc>>, _>("invite_accepted_at"),
                        "invitedByUser": access_user_value(
                            &row,
                            "inviter_id",
                            "inviter_email",
                            "inviter_name",
                            "inviter_image",
                        ),
                    })
                });
            json!({
                "id": row.get::<Uuid, _>("id"),
                "inviteId": row.get::<Option<Uuid>, _>("invite_id"),
                "companyId": row.get::<Uuid, _>("company_id"),
                "requestType": row.get::<String, _>("request_type"),
                "status": row.get::<String, _>("status"),
                "requestIp": row.get::<String, _>("request_ip"),
                "requestingUserId": requester_id,
                "requestEmailSnapshot": row.get::<Option<String>, _>("request_email_snapshot"),
                "agentName": row.get::<Option<String>, _>("agent_name"),
                "adapterType": row.get::<Option<String>, _>("adapter_type"),
                "capabilities": row.get::<Option<String>, _>("capabilities"),
                "agentDefaultsPayload": row.get::<Option<serde_json::Value>, _>("agent_defaults_payload"),
                "claimSecretExpiresAt": row.get::<Option<chrono::DateTime<Utc>>, _>("claim_secret_expires_at"),
                "claimSecretConsumedAt": row.get::<Option<chrono::DateTime<Utc>>, _>("claim_secret_consumed_at"),
                "createdAgentId": row.get::<Option<Uuid>, _>("created_agent_id"),
                "approvedByUserId": row.get::<Option<Uuid>, _>("approved_by_user_id"),
                "approvedAt": row.get::<Option<chrono::DateTime<Utc>>, _>("approved_at"),
                "rejectedByUserId": row.get::<Option<Uuid>, _>("rejected_by_user_id"),
                "rejectedAt": row.get::<Option<chrono::DateTime<Utc>>, _>("rejected_at"),
                "createdAt": row.get::<chrono::DateTime<Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<Utc>, _>("updated_at"),
                "requesterUser": access_user_value(
                    &row,
                    "requester_id",
                    "requester_email",
                    "requester_name",
                    "requester_image",
                ),
                "approvedByUser": access_user_value(
                    &row,
                    "approver_id",
                    "approver_email",
                    "approver_name",
                    "approver_image",
                ),
                "rejectedByUser": access_user_value(
                    &row,
                    "rejecter_id",
                    "rejecter_email",
                    "rejecter_name",
                    "rejecter_image",
                ),
                "invite": invite,
                // Kept for older clients which still render the legacy fields.
                "principalType": "user",
                "principalId": requester_id,
                "message": row.get::<Option<String>, _>("message"),
            })
        })
        .collect())
}

/// POST /api/companies/:company_id/join-requests/:request_id/approve
/// 批准加入请求。
async fn approve_join_request(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, request_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("joins:approve") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    // Check join request exists and is pending
    let jr: Option<(Uuid, Uuid, String, String, Option<Uuid>, Option<String>)> = sqlx::query_as(
        r#"SELECT j.id, j.requester_user_id, j.status::text, j.request_type,
                  j.created_agent_id, i.human_role::text
           FROM join_requests j
           LEFT JOIN invites i ON i.id = j.invite_id
           WHERE j.id = $1 AND j.company_id = $2"#
    )
    .bind(request_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find join request: {}", e)))?;

    let (_jr_id, principal_id, status, request_type, created_agent_id, requested_role) =
        jr.ok_or_else(|| AuthError::bad_request("Join request not found"))?;

    if status != "pending_approval" {
        return Err(AuthError::bad_request("Join request already reviewed"));
    }

    // Approve: update status and create membership
    let now = Utc::now();
    sqlx::query(
        r#"UPDATE join_requests SET status = 'approved', reviewed_by_user_id = $1,
           reviewed_at = $2, updated_at = $2 WHERE id = $3"#
    )
    .bind(user_id)
    .bind(now)
    .bind(request_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to approve join request: {}", e)))?;

    if request_type == "agent" {
        let agent_id = created_agent_id
            .ok_or_else(|| AuthError::internal("Agent join request has no created agent"))?;
        let updated = sqlx::query(
            "UPDATE agents
             SET status = 'idle', updated_at = $1
             WHERE id = $2 AND company_id = $3 AND status = 'pending_approval'",
        )
        .bind(now)
        .bind(agent_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to activate invited agent: {}", e)))?
        .rows_affected();
        if updated == 0 {
            return Err(AuthError::conflict(
                "Invited agent is no longer awaiting approval",
            ));
        }
    } else {
        // Human requests create a company membership with the role carried by
        // the invite. Agent requests are represented by the agent row instead.
        let membership_id = Uuid::new_v4();
        let membership_role = requested_role.unwrap_or_else(|| "operator".to_string());
        if !matches!(
            membership_role.as_str(),
            "owner" | "admin" | "operator" | "viewer"
        ) {
            return Err(AuthError::internal("Join request has an invalid membership role"));
        }
        sqlx::query(
            r#"INSERT INTO company_memberships (id, company_id, principal_type, principal_id,
               membership_role, status, created_at, updated_at)
               VALUES ($1, $2, 'user'::principal_type, $3, $4::membership_role,
                       'active'::company_membership_status, $5, $5)
               ON CONFLICT (company_id, principal_type, principal_id)
               DO UPDATE SET membership_role = EXCLUDED.membership_role,
                             status = EXCLUDED.status,
                             updated_at = EXCLUDED.updated_at"#,
        )
        .bind(membership_id)
        .bind(company_id)
        .bind(principal_id)
        .bind(&membership_role)
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to create membership: {}", e)))?;
    }

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.join_request_approved",
        &actor,
        "join_request",
        request_id,
        json!({ "principalId": principal_id, "requestType": request_type }),
    )
    .await;

    let request = load_join_requests(&state.pool, company_id, None, None, Some(request_id))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AuthError::internal("Approved join request disappeared"))?;
    Ok(Json(request))
}

/// POST /api/companies/:company_id/join-requests/:request_id/reject
/// 拒绝加入请求。
async fn reject_join_request(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, request_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<RejectJoinRequestPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("joins:approve") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    // Check join request exists and is pending
    let request_meta: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT request_type, created_agent_id
         FROM join_requests
         WHERE id = $1 AND company_id = $2",
    )
    .bind(request_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find join request: {}", e)))?;

    let (request_type, created_agent_id): (String, Option<Uuid>) =
        request_meta.ok_or_else(|| AuthError::bad_request("Join request not found"))?;
    let status: String = sqlx::query_scalar(
        "SELECT status::text FROM join_requests WHERE id = $1 AND company_id = $2",
    )
    .bind(request_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to load join request status: {}", e)))?;
    if status != "pending_approval" {
        return Err(AuthError::bad_request("Join request already reviewed"));
    }

    let now = Utc::now();
    sqlx::query(
        r#"UPDATE join_requests SET status = 'rejected', reviewed_by_user_id = $1,
           reviewed_at = $2, rejected_by_user_id = $1, rejected_at = $2,
           rejection_reason = $3, updated_at = $2 WHERE id = $4"#
    )
    .bind(user_id)
    .bind(now)
    .bind(&payload.reason)
    .bind(request_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to reject join request: {}", e)))?;

    if request_type == "agent" {
        if let Some(agent_id) = created_agent_id {
            sqlx::query(
                "UPDATE agents
                 SET status = 'terminated', updated_at = $1
                 WHERE id = $2 AND company_id = $3 AND status = 'pending_approval'",
            )
            .bind(now)
            .bind(agent_id)
            .bind(company_id)
            .execute(&state.pool)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to reject invited agent: {}", e)))?;
        }
    }

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.join_request_rejected",
        &actor,
        "join_request",
        request_id,
        json!({ "reason": payload.reason, "requestType": request_type }),
    )
    .await;

    let request = load_join_requests(&state.pool, company_id, None, None, Some(request_id))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AuthError::internal("Rejected join request disappeared"))?;
    Ok(Json(request))
}

#[derive(Debug, Clone, Deserialize)]
struct RejectJoinRequestPayload {
    reason: Option<String>,
}

// ============================================================================
// 阶段二：成员管理端点
// ============================================================================

/// GET /api/companies/:company_id/members
/// 列出公司成员及当前操作者在该公司的访问能力。
async fn list_members(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    actor_user_id(&actor)?;

    if !actor.is_instance_admin() && !crate::routes::has_company_access(&actor, company_id) {
        return Err(AuthError::forbidden("Company access required"));
    }

    let members = load_company_members(&state.pool, company_id).await?;
    let can_manage_members = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("members:manage"),
        },
        Some(company_id),
    )
    .await;
    let can_invite_users = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("users:invite"),
        },
        Some(company_id),
    )
    .await;
    let can_approve_join_requests = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("joins:approve"),
        },
        Some(company_id),
    )
    .await;
    let current_user_role = match actor.role_in(company_id).map(api_membership_role) {
        Some(role) => Some(role.to_string()),
        None => sqlx::query_scalar::<_, String>(
            "SELECT membership_role::text
             FROM company_memberships
             WHERE company_id = $1 AND principal_type = 'user'::principal_type
               AND principal_id = $2 AND status::text = 'active'",
        )
        .bind(company_id)
        .bind(actor_user_id(&actor)?)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load current member role: {}", e)))?,
    };

    Ok(Json(json!({
        "members": members,
        "access": {
            "currentUserRole": current_user_role,
            "canManageMembers": can_manage_members,
            "canInviteUsers": can_invite_users,
            "canApproveJoinRequests": can_approve_join_requests,
        },
    })))
}

/// PATCH /api/companies/:company_id/members/:member_id
/// 更新成员信息。
async fn update_member(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = assert_member_management(&state.pool, &actor, company_id).await?;
    if payload.membership_role.is_none() && payload.status.is_none() {
        return Err(AuthError::bad_request(
            "membershipRole or status is required",
        ));
    }

    let role = payload
        .membership_role
        .as_ref()
        .and_then(|value| value.as_deref());
    let status = payload.status.as_deref();
    let member = update_member_access(
        &state.pool,
        company_id,
        member_id,
        role,
        status,
        None,
        user_id,
    )
    .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.member_role_updated",
        &actor,
        "company_membership",
        member_id,
        json!({
            "membershipRole": &payload.membership_role,
            "status": &payload.status,
        }),
    )
    .await;

    Ok(Json(member))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMemberPayload {
    /// `Some(None)` 表示客户端显式传了 null。数据库成员角色不能为 null，
    /// 因此 null 被视为“不修改角色”，同时仍满足 Paperclip 的契约校验。
    membership_role: Option<Option<String>>,
    status: Option<String>,
}

/// PATCH /api/companies/:company_id/members/:member_id/role-and-grants
/// 更新成员角色和权限授予。
async fn update_member_role_and_grants(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateRoleAndGrantsPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = assert_member_role_management(&state.pool, &actor, company_id).await?;
    if payload.membership_role.is_none()
        && payload.role.is_none()
        && payload.status.is_none()
    {
        return Err(AuthError::bad_request(
            "membershipRole or status is required",
        ));
    }

    let role = payload
        .membership_role
        .as_ref()
        .and_then(|value| value.as_deref())
        .or(payload.role.as_deref());
    let status = payload.status.as_deref();
    let member = update_member_access(
        &state.pool,
        company_id,
        member_id,
        role,
        status,
        Some(payload.grants),
        user_id,
    )
    .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.member_role_and_grants_updated",
        &actor,
        "company_membership",
        member_id,
        json!({
            "membershipRole": &payload.membership_role,
            "role": &payload.role,
            "status": &payload.status,
        }),
    )
    .await;

    Ok(Json(member))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRoleAndGrantsPayload {
    membership_role: Option<Option<String>>,
    status: Option<String>,
    #[serde(default)]
    grants: Vec<MemberGrantInput>,
    /// 兼容早期 Parrot 客户端发送的 `role` 字段。
    role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberGrantInput {
    permission_key: String,
    #[serde(default)]
    scope: Option<serde_json::Value>,
}

/// PATCH /api/companies/:company_id/members/:member_id/permissions
/// 只替换成员的显式权限授予集合。
async fn update_member_permissions(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberPermissionsPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = assert_member_role_management(&state.pool, &actor, company_id).await?;
    let member = update_member_access(
        &state.pool,
        company_id,
        member_id,
        None,
        None,
        Some(payload.grants),
        user_id,
    )
    .await?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.member_permissions_updated",
        &actor,
        "company_membership",
        member_id,
        json!({ "grantCount": member["grants"].as_array().map_or(0, Vec::len) }),
    )
    .await;

    Ok(Json(member))
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateMemberPermissionsPayload {
    #[serde(default)]
    grants: Vec<MemberGrantInput>,
}

/// POST /api/companies/:company_id/members/:member_id/archive
/// 归档成员。
async fn archive_member(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    body: Option<Json<ArchiveMemberPayload>>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let _user_id = assert_member_management(&state.pool, &actor, company_id).await?;
    let reassignment = body.and_then(|Json(payload)| payload.reassignment);

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to start archive transaction: {}", e)))?;
    let principal_id: Uuid = sqlx::query_scalar(
        "SELECT principal_id
         FROM company_memberships
         WHERE id = $1 AND company_id = $2 AND principal_type = 'user'::principal_type
           AND status::text NOT IN ('archived', 'inactive')",
    )
    .bind(member_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find member: {}", e)))?
    .ok_or_else(|| AuthError::not_found("Active user membership not found"))?;

    let member_role: String = sqlx::query_scalar(
        "SELECT membership_role::text FROM company_memberships WHERE id = $1",
    )
    .bind(member_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to load member role: {}", e)))?;
    if member_role == "owner" {
        let active_owner_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM company_memberships
             WHERE company_id = $1 AND principal_type = 'user'::principal_type
               AND membership_role = 'owner'::membership_role
               AND status::text = 'active'",
        )
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to count company owners: {}", e)))?;
        if active_owner_count <= 1 {
            return Err(AuthError::conflict(
                "The company must keep at least one owner",
            ));
        }
    }

    if let Some(reassignment) = &reassignment {
        if reassignment.assignee_agent_id.is_some() && reassignment.assignee_user_id.is_some() {
            return Err(AuthError::bad_request(
                "Choose either an agent or user reassignment target",
            ));
        }
        if let Some(agent_id) = reassignment.assignee_agent_id {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND company_id = $2)",
            )
            .bind(agent_id)
            .bind(company_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to validate agent target: {}", e)))?;
            if !exists {
                return Err(AuthError::bad_request("Reassignment agent is not in this company"));
            }
        }
        if let Some(assignee_user_id) = reassignment.assignee_user_id {
            if assignee_user_id == principal_id {
                return Err(AuthError::bad_request("A member cannot be reassigned to themselves"));
            }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(
                     SELECT 1 FROM company_memberships
                     WHERE company_id = $1 AND principal_type = 'user'::principal_type
                       AND principal_id = $2 AND status::text = 'active'
                 )",
            )
            .bind(company_id)
            .bind(assignee_user_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to validate user target: {}", e)))?;
            if !exists {
                return Err(AuthError::bad_request("Reassignment user is not an active member"));
            }
        }
    }

    let (assignee_agent_id, assignee_user_id) = reassignment
        .as_ref()
        .map(|value| (value.assignee_agent_id, value.assignee_user_id))
        .unwrap_or((None, None));
    let active_issue_statuses = vec![
        "backlog".to_string(),
        "todo".to_string(),
        "in_progress".to_string(),
        "in_review".to_string(),
        "blocked".to_string(),
        "failed".to_string(),
        "timed_out".to_string(),
    ];
    let reassigned_issue_count = sqlx::query(
        "UPDATE issues
         SET assignee_agent_id = $2, assignee_user_id = $3, updated_at = NOW()
         WHERE company_id = $1 AND assignee_user_id = $4
           AND status::text = ANY($5::text[])
         RETURNING id",
    )
    .bind(company_id)
    .bind(assignee_agent_id)
    .bind(assignee_user_id)
    .bind(principal_id)
    .bind(&active_issue_statuses)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to reassign member issues: {}", e)))?
    .len();

    let archived = sqlx::query(
        "UPDATE company_memberships
         SET status = 'archived'::company_membership_status, updated_at = NOW()
         WHERE id = $1 AND company_id = $2",
    )
    .bind(member_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to archive member: {}", e)))?
    .rows_affected();
    if archived == 0 {
        return Err(AuthError::not_found("Active user membership not found"));
    }
    tx.commit()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to commit member archive: {}", e)))?;

    let member = load_company_member(&state.pool, company_id, member_id)
        .await?
        .ok_or_else(|| AuthError::internal("Archived member disappeared"))?;

    crate::routes::log_activity(
        &state.pool,
        company_id,
        "company.member_archived",
        &actor,
        "company_membership",
        member_id,
        json!({ "reassignedIssueCount": reassigned_issue_count }),
    )
    .await;

    Ok(Json(json!({
        "member": member,
        "reassignedIssueCount": reassigned_issue_count,
    })))
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveMemberPayload {
    reassignment: Option<ArchiveReassignment>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveReassignment {
    assignee_agent_id: Option<Uuid>,
    assignee_user_id: Option<Uuid>,
}

fn api_membership_role(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::Owner => "owner",
        MembershipRole::Admin => "admin",
        MembershipRole::Operator => "operator",
        MembershipRole::Viewer => "viewer",
    }
}

fn api_member_status(status: &str) -> &'static str {
    match status {
        "pending" => "pending",
        "active" => "active",
        "suspended" => "suspended",
        "archived" | "inactive" => "archived",
        _ => "archived",
    }
}

fn db_member_status(status: &str) -> Result<&'static str, AuthError> {
    match status {
        "pending" => Ok("pending"),
        "active" => Ok("active"),
        "suspended" => Ok("suspended"),
        // `inactive` remains accepted for old API clients; new writes use the
        // Paperclip-compatible `archived` value.
        "archived" | "inactive" => Ok("archived"),
        _ => Err(AuthError::bad_request(
            "status must be pending, active, suspended, or archived",
        )),
    }
}

async fn assert_member_management(
    pool: &sqlx::PgPool,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<Uuid, AuthError> {
    let user_id = actor_user_id(actor)?;
    let allowed = decide_access(
        pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("members:manage"),
        },
        Some(company_id),
    )
    .await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }
    Ok(user_id)
}

async fn assert_member_role_management(
    pool: &sqlx::PgPool,
    actor: &AuthorizationActor,
    company_id: Uuid,
) -> Result<Uuid, AuthError> {
    let user_id = actor_user_id(actor)?;
    let can_assign_roles = decide_access(
        pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("roles:assign"),
        },
        Some(company_id),
    )
    .await;
    let can_manage_members = decide_access(
        pool,
        actor,
        &AuthorizationAction::Permission {
            key: PermissionKey::new("members:manage"),
        },
        Some(company_id),
    )
    .await;
    if !can_assign_roles && !can_manage_members {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }
    Ok(user_id)
}

async fn load_company_members(
    pool: &sqlx::PgPool,
    company_id: Uuid,
) -> Result<Vec<serde_json::Value>, AuthError> {
    use sqlx::Row;

    let member_rows = sqlx::query(
        "SELECT cm.id, cm.company_id, cm.principal_id,
                cm.membership_role::text AS membership_role,
                cm.status::text AS status, cm.created_at, cm.updated_at,
                u.id AS user_id, u.email AS user_email, u.name AS user_name,
                u.avatar_url AS user_image
         FROM company_memberships cm
         LEFT JOIN auth_users u
           ON cm.principal_type = 'user'::principal_type AND u.id = cm.principal_id
         WHERE cm.company_id = $1 AND cm.principal_type = 'user'::principal_type
         ORDER BY cm.created_at ASC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list members: {}", e)))?;

    let grant_rows = sqlx::query(
        "SELECT id, company_id, principal_id, permission_key, scope,
                granted_by_user_id, created_at, updated_at
         FROM principal_permission_grants
         WHERE company_id = $1 AND principal_type = 'user'::principal_type
         ORDER BY created_at ASC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list member grants: {}", e)))?;

    let mut grants_by_principal: HashMap<Uuid, Vec<serde_json::Value>> = HashMap::new();
    for grant in grant_rows {
        let principal_id = grant.get::<Uuid, _>("principal_id");
        grants_by_principal
            .entry(principal_id)
            .or_default()
            .push(json!({
                "id": grant.get::<Uuid, _>("id"),
                "companyId": grant.get::<Uuid, _>("company_id"),
                "principalType": "user",
                "principalId": principal_id,
                "permissionKey": grant.get::<String, _>("permission_key"),
                "scope": grant.get::<serde_json::Value, _>("scope"),
                "grantedByUserId": grant.get::<Uuid, _>("granted_by_user_id"),
                "createdAt": grant.get::<chrono::DateTime<Utc>, _>("created_at"),
                "updatedAt": grant.get::<chrono::DateTime<Utc>, _>("updated_at"),
            }));
    }

    let owner_count = member_rows
        .iter()
        .filter(|row| {
            row.get::<String, _>("membership_role") == "owner"
                && api_member_status(row.get::<String, _>("status").as_str()) == "active"
        })
        .count();

    Ok(member_rows
        .into_iter()
        .map(|row| {
            let member_id = row.get::<Uuid, _>("id");
            let principal_id = row.get::<Uuid, _>("principal_id");
            let role = row.get::<String, _>("membership_role");
            let status = api_member_status(row.get::<String, _>("status").as_str());
            let user = row
                .get::<Option<Uuid>, _>("user_id")
                .map(|id| {
                    json!({
                        "id": id,
                        "email": row.get::<Option<String>, _>("user_email"),
                        "name": row.get::<Option<String>, _>("user_name"),
                        "image": row.get::<Option<String>, _>("user_image"),
                    })
                });
            let can_archive = status != "archived"
                && (role != "owner" || owner_count > 1);
            let removal_reason = if status == "archived" {
                Some("Member is already archived".to_string())
            } else if role == "owner" && owner_count <= 1 {
                Some("The company must keep at least one owner".to_string())
            } else {
                None
            };

            json!({
                "id": member_id,
                "companyId": row.get::<Uuid, _>("company_id"),
                "principalType": "user",
                "principalId": principal_id,
                "status": status,
                "membershipRole": role,
                "createdAt": row.get::<chrono::DateTime<Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<Utc>, _>("updated_at"),
                "user": user,
                "grants": grants_by_principal.remove(&principal_id).unwrap_or_default(),
                "removal": {
                    "canArchive": can_archive,
                    "reason": removal_reason,
                },
            })
        })
        .collect())
}

async fn load_company_member(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    member_id: Uuid,
) -> Result<Option<serde_json::Value>, AuthError> {
    let member_id_string = member_id.to_string();
    Ok(load_company_members(pool, company_id)
        .await?
        .into_iter()
        .find(|member| {
            member.get("id").and_then(serde_json::Value::as_str)
                == Some(member_id_string.as_str())
        }))
}

async fn update_member_access(
    pool: &sqlx::PgPool,
    company_id: Uuid,
    member_id: Uuid,
    role: Option<&str>,
    status: Option<&str>,
    grants: Option<Vec<MemberGrantInput>>,
    granted_by_user_id: Uuid,
) -> Result<serde_json::Value, AuthError> {
    let role = match role {
        Some(role) if matches!(role, "owner" | "admin" | "operator" | "viewer") => Some(role),
        Some(_) => {
            return Err(AuthError::bad_request(
                "membershipRole must be owner, admin, operator, or viewer",
            ));
        }
        None => None,
    };
    let status = status.map(db_member_status).transpose()?;
    if let Some(grants) = &grants {
        for grant in grants {
            if grant.permission_key.trim().is_empty() || grant.permission_key.len() > 100 {
                return Err(AuthError::bad_request("permissionKey must be 1-100 characters"));
            }
            if let Some(scope) = &grant.scope {
                if !scope.is_object() {
                    return Err(AuthError::bad_request("grant scope must be an object or null"));
                }
            }
        }
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to start member update: {}", e)))?;
    let principal_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT principal_id
         FROM company_memberships
         WHERE id = $1 AND company_id = $2 AND principal_type = 'user'::principal_type",
    )
    .bind(member_id)
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find member: {}", e)))?;
    let principal_id = principal_id.ok_or_else(|| AuthError::not_found("User membership not found"))?;

    sqlx::query(
        "UPDATE company_memberships
         SET membership_role = COALESCE($1::membership_role, membership_role),
             status = COALESCE($2::company_membership_status, status),
             updated_at = NOW()
         WHERE id = $3 AND company_id = $4",
    )
    .bind(role)
    .bind(status)
    .bind(member_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to update member: {}", e)))?;

    if let Some(grants) = grants {
        sqlx::query(
            "DELETE FROM principal_permission_grants
             WHERE company_id = $1 AND principal_type = 'user'::principal_type
               AND principal_id = $2",
        )
        .bind(company_id)
        .bind(principal_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to replace member grants: {}", e)))?;

        for grant in grants {
            sqlx::query(
                "INSERT INTO principal_permission_grants
                     (company_id, principal_type, principal_id, permission_key,
                      scope, granted_by_user_id, created_at, updated_at)
                 VALUES ($1, 'user'::principal_type, $2, $3, $4, $5, NOW(), NOW())",
            )
            .bind(company_id)
            .bind(principal_id)
            .bind(grant.permission_key.trim())
            .bind(grant.scope.unwrap_or_else(|| json!({})))
            .bind(granted_by_user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to create member grant: {}", e)))?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to commit member update: {}", e)))?;

    load_company_member(pool, company_id, member_id)
        .await?
        .ok_or_else(|| AuthError::internal("Updated member disappeared"))
}

// 阶段三的实例管理员端点（promote / demote）由 `routes::auth` 提供：
// 那里校验目标用户存在、禁止自我降级、并使用与 live 唯一索引匹配的
// `ON CONFLICT (user_id, role)`；用户列表由 `routes::user_directory` 的
// `/admin/users` 提供。此前本文件另有一套 `/api/admin/...` 重复实现，既因
// 重复注册挂到 `/api/api/...` 死路径，又因 `ON CONFLICT (user_id)` 与
// live 唯一索引不匹配而必然执行失败，故删除。
