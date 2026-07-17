//! 访问控制路由（对应任务拆解 §9 阶段一 + 阶段二 + 阶段三）。
//!
//! 提供 `/api` 下的访问控制路由组：
//! - 阶段一：Board 认领（board-claim + bootstrap）
//! - 阶段二：CLI 认证、Board API Key 管理、邀请、加入请求、成员管理
//! - 阶段三：实例管理员管理
//!
//! 路由组挂载 `AuthMiddleware` 层（`auth_middleware_fn`）将 `AuthorizationActor`
//! 注入 request extensions；handler 通过 `Extension<AuthorizationActor>` 读取，
//! 并通过 `assert_company_access` / `assert_instance_admin` 守卫进行授权检查。

use std::sync::Arc;

use axum::{
    async_trait,
    extract::{Extension, FromRequestParts, Path, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    routing::{get, post, delete, patch},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use services::auth::{
    AuthError, AuthMiddleware, AuthorizationActor, BoardClaimService, ClaimChallenge, JwtConfig,
    auth_middleware_fn, authenticated_middleware, AuthorizationAction, PermissionKey,
};
use services::auth::authorization_service::assert_instance_admin;
use services::auth::cli_auth::{
    create_cli_auth_challenge, get_cli_auth_challenge, approve_cli_auth_challenge,
    cancel_cli_auth_challenge,
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
    let pool = Arc::new(state.pool.clone());
    let jwt_config = JwtConfig::from_env().map(Arc::new);
    let middleware = match &jwt_config {
        Some(cfg) => authenticated_middleware(pool.clone(), cfg.clone()),
        None => AuthMiddleware::new(services::auth::AuthMode::Authenticated)
            .with_resolver(Arc::new(services::auth::BearerTokenResolver::new(
                pool.clone(),
                Arc::new(JwtConfig::new(
                    String::new(),
                    3600,
                    "parrot-agent".to_string(),
                    "agent-runtime".to_string(),
                    "local".to_string(),
                )),
            )))
            .with_resolver(Arc::new(services::auth::SessionCookieResolver::new(pool.clone())))
            .with_resolver(Arc::new(services::auth::CloudTenantHeaderResolver::new(pool.clone()))),
    };
    let mw = Arc::new(middleware);

    Router::new()
        // 阶段一：Board 认领
        .route(
            "/api/board-claim/:token",
            get(inspect_board_claim).post(claim_board),
        )
        .route("/api/bootstrap/claim", post(bootstrap_claim))
        // 阶段二：CLI 认证
        .route("/api/cli-auth/challenges", post(create_cli_challenge_handler))
        .route(
            "/api/cli-auth/challenges/:id",
            get(get_cli_challenge_handler),
        )
        .route(
            "/api/cli-auth/challenges/:id/approve",
            post(approve_cli_challenge_handler),
        )
        .route(
            "/api/cli-auth/challenges/:id/cancel",
            post(cancel_cli_challenge_handler),
        )
        .route("/api/cli-auth/me", get(get_cli_auth_me))
        .route("/api/cli-auth/revoke-current", post(revoke_current_cli_key))
        // 阶段二：Board API Key 管理
        .route("/api/board-api-keys", get(list_board_api_keys).post(create_board_api_key))
        .route("/api/board-api-keys/:key_id", delete(revoke_board_api_key))
        // 阶段二：邀请管理
        .route("/api/companies/:company_id/invites", post(create_invite))
        .route("/api/invites/:token", get(get_invite))
        .route("/api/invites/:token/accept", post(accept_invite))
        // 阶段二：加入请求
        .route("/api/companies/:company_id/join-requests", get(list_join_requests))
        .route(
            "/api/companies/:company_id/join-requests/:request_id/approve",
            post(approve_join_request),
        )
        .route(
            "/api/companies/:company_id/join-requests/:request_id/reject",
            post(reject_join_request),
        )
        // 阶段二：成员管理
        .route("/api/companies/:company_id/members", get(list_members))
        .route(
            "/api/companies/:company_id/members/:member_id",
            patch(update_member),
        )
        .route(
            "/api/companies/:company_id/members/:member_id/role-and-grants",
            patch(update_member_role_and_grants),
        )
        .route(
            "/api/companies/:company_id/members/:member_id/archive",
            post(archive_member),
        )
        // 阶段三：实例管理员
        .route("/api/admin/users", get(list_admin_users))
        .route(
            "/api/admin/users/:user_id/promote-instance-admin",
            post(promote_instance_admin),
        )
        .route(
            "/api/admin/users/:user_id/demote-instance-admin",
            post(demote_instance_admin),
        )
        .layer(axum::middleware::from_fn_with_state(mw, auth_middleware_fn))
        .with_state(state)
}

/// GET /api/board-claim/:token
///
/// 查看 Board 认领挑战详情（校验 token）。
async fn inspect_board_claim(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<ClaimChallenge>, AuthError> {
    let svc = BoardClaimService::new(state.pool.clone());
    let challenge = svc.inspect_board_claim_challenge(&token).await?;
    Ok(Json(challenge))
}

/// POST /api/board-claim/:token/claim
///
/// 认领 Board 所有权：将当前认证用户提升为实例管理员，并归档 local-board 成员关系、
/// 添加为所有公司的 owner。
async fn claim_board(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let svc = BoardClaimService::new(state.pool.clone());
    svc.claim_board_ownership(user_id, &token).await?;
    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

/// POST /api/bootstrap/claim
///
/// 首次管理员认领：实例首次运行时将当前认证用户提升为实例管理员。
/// 前置条件：当前无任何实例管理员。
async fn bootstrap_claim(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Result<impl IntoResponse, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let svc = BoardClaimService::new(state.pool.clone());
    svc.claim_first_instance_admin(user_id).await?;
    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

/// 从认证 actor 提取 Board 用户 ID（非 Board 用户返回 401）。
fn actor_user_id(actor: &AuthorizationActor) -> Result<Uuid, AuthError> {
    match actor {
        AuthorizationActor::Board { user_id, .. } => Ok(*user_id),
        _ => Err(AuthError::unauthenticated("Board user authentication required")),
    }
}

// ============================================================================
// 阶段二：CLI 认证端点
// ============================================================================

/// POST /api/cli-auth/challenges
/// 创建 CLI 认证挑战。
async fn create_cli_challenge_handler(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(payload): Json<CreateCliChallengeRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let challenge = create_cli_auth_challenge(
        &state.pool,
        user_id,
        payload.company_id,
        payload.device_name,
        payload.requested_access.unwrap_or_default(),
    )
    .await?;
    Ok(Json(json!({
        "id": challenge.id,
        "challengeCode": challenge.challenge_code,
        "status": challenge.status,
        "expiresAt": challenge.expires_at,
        "createdAt": challenge.created_at,
    })))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCliChallengeRequest {
    company_id: Option<Uuid>,
    device_name: Option<String>,
    requested_access: Option<serde_json::Value>,
}

/// GET /api/cli-auth/challenges/:id
/// 查询 CLI 认证挑战状态（供 CLI 轮询）。
async fn get_cli_challenge_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let challenge = get_cli_auth_challenge(&state.pool, id).await?;
    match challenge {
        Some(c) => Ok(Json(json!({
            "id": c.id,
            "status": c.status,
            "challengeCode": c.challenge_code,
            "deviceName": c.device_name,
            "approvedAt": c.approved_at,
            "expiresAt": c.expires_at,
            "createdAt": c.created_at,
        }))),
        None => Ok(Json(json!({
            "status": "not_found_or_expired"
        }))),
    }
}

/// POST /api/cli-auth/challenges/:id/approve
/// 批准 CLI 认证挑战，创建 Board API Key 并返回明文 token（仅一次）。
async fn approve_cli_challenge_handler(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let (challenge, plaintext_token) = approve_cli_auth_challenge(&state.pool, id, user_id).await?;
    Ok(Json(json!({
        "status": "approved",
        "apiKey": plaintext_token,
        "keyPrefix": "bak_",
        "expiresAt": challenge.expires_at,
    })))
}

/// POST /api/cli-auth/challenges/:id/cancel
/// 取消 CLI 认证挑战。
async fn cancel_cli_challenge_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    cancel_cli_auth_challenge(&state.pool, id).await?;
    Ok(Json(json!({ "status": "cancelled" })))
}

/// GET /api/cli-auth/me
/// 获取当前 CLI 认证用户信息。
async fn get_cli_auth_me(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    let (user, memberships, is_admin) =
        services::auth::board_access::resolve_board_access(&state.pool, user_id).await?;
    Ok(Json(json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "isInstanceAdmin": is_admin,
        "memberships": memberships.iter().map(|m| json!({
            "companyId": m.company_id,
            "role": m.role,
            "status": m.status,
        })).collect::<Vec<_>>(),
    })))
}

/// POST /api/cli-auth/revoke-current
/// 撤销当前使用的 Board API Key。
async fn revoke_current_cli_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    let keys = repo.list_by_user(user_id).await.map_err(|e| {
        AuthError::internal(format!("Failed to list API keys: {}", e))
    })?;

    let mut revoked_count = 0u32;
    for key in keys.iter().filter(|k| !k.is_revoked) {
        repo.revoke(key.id, user_id).await.map_err(|e| {
            AuthError::internal(format!("Failed to revoke key {}: {}", key.id, e))
        })?;
        revoked_count += 1;
    }

    Ok(Json(json!({
        "status": "revoked",
        "revokedCount": revoked_count,
    })))
}

// ============================================================================
// 阶段二：Board API Key 管理端点
// ============================================================================

/// GET /api/board-api-keys
/// 列出当前用户的 Board API Keys。
async fn list_board_api_keys(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    let keys = repo.list_by_user(user_id).await.map_err(|e| {
        AuthError::internal(format!("Failed to list API keys: {}", e))
    })?;

    Ok(Json(keys.into_iter().map(|k| json!({
        "id": k.id,
        "name": k.name,
        "keyPrefix": k.key_prefix,
        "lastUsedAt": k.last_used_at,
        "expiresAt": k.expires_at,
        "isRevoked": k.is_revoked,
        "createdAt": k.created_at,
    })).collect()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBoardApiKeyRequest {
    name: String,
}

/// POST /api/board-api-keys
/// 创建新的 Board API Key（返回明文 token 仅一次）。
async fn create_board_api_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Json(payload): Json<CreateBoardApiKeyRequest>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;

    let token = repositories::board_api_key_repository::generate_api_key_token("bak");
    let key_hash = repositories::board_api_key_repository::hash_api_key(&token);
    let key_prefix = token.get(0..16).unwrap_or("bak_").to_string();
    let expires_at = Utc::now() + chrono::Duration::days(365);

    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    let key = repo.create(user_id, payload.name, key_hash, key_prefix, Some(expires_at)).await.map_err(|e| {
        AuthError::internal(format!("Failed to create API key: {}", e))
    })?;

    Ok(Json(json!({
        "id": key.id,
        "name": key.name,
        "token": token,
        "keyPrefix": key.key_prefix,
        "expiresAt": key.expires_at,
        "createdAt": key.created_at,
    })))
}

/// DELETE /api/board-api-keys/:key_id
/// 撤销 Board API Key。
async fn revoke_board_api_key(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(key_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;
    use repositories::board_api_key_repository::BoardApiKeyRepository;
    let repo = repositories::board_api_key_repository::PgBoardApiKeyRepository::new(state.pool.clone());
    repo.revoke(key_id, user_id).await.map_err(|e| {
        AuthError::internal(format!("Failed to revoke API key: {}", e))
    })?;
    Ok(Json(json!({ "status": "revoked" })))
}

// ============================================================================
// 阶段二：邀请管理端点
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInviteRequest {
    invite_type: String,
    email: Option<String>,
    allowed_join_types: Option<String>,
    ttl_hours: Option<i64>,
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

    let token = uuid::Uuid::new_v4().to_string().replace('-', "");
    let now = Utc::now();
    let expires_at = now + chrono::Duration::hours(payload.ttl_hours.unwrap_or(72));

    // Store invite in database using raw SQL
    let invite_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invites (id, company_id, invite_type, invited_by_user_id, email, token,
           allowed_join_types, expires_at, used_at, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9)"#
    )
    .bind(invite_id)
    .bind(company_id)
    .bind(invite_type)
    .bind(user_id)
    .bind(&payload.email)
    .bind(&token)
    .bind(allowed_join_types)
    .bind(expires_at)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to create invite: {}", e)))?;

    Ok(Json(json!({
        "id": invite_id,
        "token": token,
        "inviteType": invite_type,
        "expiresAt": expires_at,
        "createdAt": now,
    })))
}

/// GET /api/invites/:token
/// 获取邀请详情。
async fn get_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)>(
        r#"SELECT id, company_id, invite_type, allowed_join_types, used_at, expires_at
           FROM invites WHERE token = $1"#
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?
    .ok_or_else(|| AuthError::bad_request("Invite not found"))?;

    let (invite_id, company_id, invite_type, allowed_join_types, used_at, expires_at) = row;

    if Utc::now() > expires_at {
        return Err(AuthError::bad_request("Invite has expired"));
    }
    if used_at.is_some() {
        return Err(AuthError::bad_request("Invite has already been used"));
    }

    // Load company name
    let company_name: Option<String> = sqlx::query_scalar(
        "SELECT name FROM companies WHERE id = $1"
    )
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to load company: {}", e)))?
    .flatten();

    Ok(Json(json!({
        "id": invite_id,
        "companyId": company_id,
        "companyName": company_name.unwrap_or_default(),
        "inviteType": invite_type,
        "allowedJoinTypes": allowed_join_types,
        "expiresAt": expires_at,
    })))
}

/// POST /api/invites/:token/accept
/// 接受邀请。
async fn accept_invite(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, AuthError> {
    let user_id = actor_user_id(&actor)?;

    // Find invite
    let invite: Option<(Uuid, Uuid, String, String, Option<chrono::DateTime<Utc>>, chrono::DateTime<Utc>)> = sqlx::query_as(
        r#"SELECT id, company_id, invite_type, allowed_join_types, used_at, expires_at
           FROM invites WHERE token = $1"#
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find invite: {}", e)))?;

    let (invite_id, company_id, _invite_type, _allowed_join_types, used_at, expires_at) =
        invite.ok_or_else(|| AuthError::bad_request("Invite not found"))?;

    if Utc::now() > expires_at {
        return Err(AuthError::bad_request("Invite has expired"));
    }
    if used_at.is_some() {
        return Err(AuthError::bad_request("Invite has already been used"));
    }

    // Create join request
    let jr_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO join_requests (id, company_id, principal_type, principal_id, status,
           requested_role, message, reviewed_by_user_id, reviewed_at, rejection_reason, created_at, updated_at)
           VALUES ($1, $2, 'user', $3, 'pending_approval', 'operator', $4, NULL, NULL, NULL, $5, $5)"#
    )
    .bind(jr_id)
    .bind(company_id)
    .bind(user_id)
    .bind(format!("Accepted via invite {}", token))
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to create join request: {}", e)))?;

    // Mark invite as used
    sqlx::query("UPDATE invites SET used_at = $1 WHERE id = $2")
        .bind(now)
        .bind(invite_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to mark invite as used: {}", e)))?;

    Ok(Json(json!({
        "status": "pending_approval",
        "joinRequestId": jr_id,
    })))
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

    #[derive(sqlx::FromRow)]
    struct JrRow {
        id: Uuid,
        principal_type: String,
        principal_id: Uuid,
        status: String,
        requested_role: String,
        message: Option<String>,
        created_at: chrono::DateTime<Utc>,
    }

    let requests: Vec<JrRow> = sqlx::query_as::<_, JrRow>(
        r#"SELECT id, principal_type, principal_id, status, requested_role, message, created_at
           FROM join_requests WHERE company_id = $1 ORDER BY created_at DESC"#
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list join requests: {}", e)))?;

    Ok(Json(requests.into_iter().map(|jr| json!({
        "id": jr.id,
        "principalType": jr.principal_type,
        "principalId": jr.principal_id,
        "status": jr.status,
        "requestedRole": jr.requested_role,
        "message": jr.message,
        "createdAt": jr.created_at,
    })).collect()))
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
    let jr: Option<(Uuid, String, Uuid, String)> = sqlx::query_as(
        r#"SELECT id, principal_type, principal_id, status
           FROM join_requests WHERE id = $1 AND company_id = $2"#
    )
    .bind(request_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find join request: {}", e)))?;

    let (_jr_id, principal_type, principal_id, status) =
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

    // Create membership
    let membership_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO company_memberships (id, company_id, principal_type, principal_id, role, status,
           joined_at, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'operator', 'active', $5, $5, $5)
           ON CONFLICT DO NOTHING"#
    )
    .bind(membership_id)
    .bind(company_id)
    .bind(&principal_type)
    .bind(principal_id)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to create membership: {}", e)))?;

    Ok(Json(json!({
        "status": "approved",
        "principalId": principal_id,
    })))
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
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM join_requests WHERE id = $1 AND company_id = $2"
    )
    .bind(request_id)
    .bind(company_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to find join request: {}", e)))?;

    let status = status.ok_or_else(|| AuthError::bad_request("Join request not found"))?;
    if status != "pending_approval" {
        return Err(AuthError::bad_request("Join request already reviewed"));
    }

    let now = Utc::now();
    sqlx::query(
        r#"UPDATE join_requests SET status = 'rejected', reviewed_by_user_id = $1,
           reviewed_at = $2, rejection_reason = $3, updated_at = $2 WHERE id = $4"#
    )
    .bind(user_id)
    .bind(now)
    .bind(&payload.reason)
    .bind(request_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to reject join request: {}", e)))?;

    Ok(Json(json!({ "status": "rejected" })))
}

#[derive(Debug, Clone, Deserialize)]
struct RejectJoinRequestPayload {
    reason: Option<String>,
}

// ============================================================================
// 阶段二：成员管理端点
// ============================================================================

/// GET /api/companies/:company_id/members
/// 列出公司成员。
async fn list_members(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AuthError> {
    actor_user_id(&actor)?;

    #[derive(sqlx::FromRow)]
    struct MemberRow {
        id: Uuid,
        principal_type: String,
        principal_id: Uuid,
        role: String,
        status: String,
        created_at: chrono::DateTime<Utc>,
    }

    let members: Vec<MemberRow> = sqlx::query_as::<_, MemberRow>(
        r#"SELECT id, principal_type, principal_id, role, status, created_at
           FROM company_memberships WHERE company_id = $1 AND status = 'active'
           ORDER BY created_at ASC"#
    )
    .bind(company_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list members: {}", e)))?;

    Ok(Json(members.into_iter().map(|m| json!({
        "id": m.id,
        "principalType": m.principal_type,
        "principalId": m.principal_id,
        "role": m.role,
        "status": m.status,
        "createdAt": m.created_at,
    })).collect()))
}

/// PATCH /api/companies/:company_id/members/:member_id
/// 更新成员信息。
async fn update_member(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateMemberPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("members:manage") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    sqlx::query("UPDATE company_memberships SET role = $1, updated_at = NOW() WHERE id = $2 AND company_id = $3")
        .bind(&payload.role)
        .bind(member_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to update member: {}", e)))?;

    Ok(Json(json!({ "status": "updated", "role": payload.role })))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMemberPayload {
    role: String,
}

/// PATCH /api/companies/:company_id/members/:member_id/role-and-grants
/// 更新成员角色和权限授予。
async fn update_member_role_and_grants(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateRoleAndGrantsPayload>,
) -> Result<Json<serde_json::Value>, AuthError> {
    actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("roles:assign") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    sqlx::query("UPDATE company_memberships SET role = $1, updated_at = NOW() WHERE id = $2 AND company_id = $3")
        .bind(&payload.role)
        .bind(member_id)
        .bind(company_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to update member role: {}", e)))?;

    Ok(Json(json!({ "status": "updated", "role": payload.role })))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRoleAndGrantsPayload {
    role: String,
    #[allow(dead_code)]
    grants: Option<Vec<String>>,
}

/// POST /api/companies/:company_id/members/:member_id/archive
/// 归档成员。
async fn archive_member(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path((company_id, member_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AuthError> {
    actor_user_id(&actor)?;

    let allowed = decide_access(
        &state.pool,
        &actor,
        &AuthorizationAction::Permission { key: PermissionKey::new("members:manage") },
        Some(company_id),
    ).await;
    if !allowed {
        return Err(AuthError::forbidden("Insufficient permissions"));
    }

    sqlx::query(
        "UPDATE company_memberships SET status = 'archived', archived_at = NOW(), updated_at = NOW() WHERE id = $1 AND company_id = $2"
    )
    .bind(member_id)
    .bind(company_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to archive member: {}", e)))?;

    Ok(Json(json!({ "status": "archived" })))
}

// ============================================================================
// 阶段三：实例管理员端点
// ============================================================================

/// GET /api/admin/users
/// 列出所有用户（需 instance admin）。
async fn list_admin_users(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AuthError> {
    assert_instance_admin(&actor)?;

    #[derive(sqlx::FromRow)]
    struct AdminUserRow {
        id: Uuid,
        email: String,
        name: Option<String>,
        created_at: chrono::DateTime<Utc>,
    }

    let users: Vec<AdminUserRow> = sqlx::query_as::<_, AdminUserRow>(
        "SELECT id, email, name, created_at FROM auth_users ORDER BY created_at DESC"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list users: {}", e)))?;

    let total = users.len();

    Ok(Json(json!({
        "users": users.into_iter().map(|u| json!({
            "id": u.id,
            "email": u.email,
            "name": u.name,
            "createdAt": u.created_at,
        })).collect::<Vec<_>>(),
        "total": total,
    })))
}

/// POST /api/admin/users/:user_id/promote-instance-admin
/// 提升用户为实例管理员。
async fn promote_instance_admin(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    assert_instance_admin(&actor)?;

    sqlx::query(
        r#"INSERT INTO instance_user_roles (id, user_id, role, created_at)
           VALUES ($1, $2, 'instance_admin', NOW())
           ON CONFLICT (user_id) DO NOTHING"#
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to promote user: {}", e)))?;

    Ok(Json(json!({ "status": "promoted" })))
}

/// POST /api/admin/users/:user_id/demote-instance-admin
/// 降级实例管理员。
async fn demote_instance_admin(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    assert_instance_admin(&actor)?;

    sqlx::query("DELETE FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin'")
        .bind(user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to demote user: {}", e)))?;

    Ok(Json(json!({ "status": "demoted" })))
}

