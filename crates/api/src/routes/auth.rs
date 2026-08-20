//! 认证路由（对应任务拆解 §8 阶段一）。
//!
//! 提供 `/api/auth` 路由组：
//! - `GET  /api/auth/get-session`：从 request extensions 提取 `AuthorizationActor`，
//!   返回当前会话信息（未登录返回 `{ session: null }`）。
//! - `GET  /api/auth/profile`：查询当前 Board 用户资料（未认证返回 401）。
//! - `PATCH /api/auth/profile`：更新当前 Board 用户资料（name / avatar_url）。
//!
//! 认证通过 `AuthMiddleware` 中间件层注入 `AuthorizationActor` 到 request extensions，
//! handler 通过 `extract_actor` 读取。

use axum::{
    extract::{Extension, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use services::auth::authorization_service::assert_instance_admin;
use services::auth::{auth_cookie_prefix, AuthError, AuthorizationActor};
use services::auth::{ActorSource, MembershipRole};

use crate::app_state::AppState;

use repositories::auth_repositories::{AuthSessionRepository, AuthUserRepository, PgAuthSessionRepository, PgAuthUserRepository};
use repositories::models::auth::{AuthSession, AuthUser};
use sha2::{Digest, Sha256};

/// 构建 `/api/auth` 路由组，并挂载认证中间件层。
pub fn auth_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/sign-up/email", post(sign_up_email))
        .route("/auth/sign-in/email", post(sign_in_email))
        .route("/auth/sign-out", post(sign_out))
        .route("/auth/get-session", get(get_session))
        .route("/auth/profile", get(get_profile).patch(update_profile))
        // --- P3: Admin routes (AU1-AU5) ---
        .route("/admin/users/:user_id/promote-instance-admin", post(promote_instance_admin))
        .route("/admin/users/:user_id/demote-instance-admin", post(demote_instance_admin))
        .route("/admin/users/:user_id/company-access", get(get_user_company_access).put(update_user_company_access))
        .route("/join-requests/:request_id/claim-api-key", post(claim_join_request_api_key))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct EmailAuthRequest { email: String, password: String, #[serde(default)] name: Option<String> }

fn password_digest(password: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"parrot-auth-password-v1:");
    h.update(password.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn session_cookie(token: &str, max_age: i64) -> Result<HeaderValue, AuthError> {
    let prefix = auth_cookie_prefix(&std::env::var("INSTANCE_ID").unwrap_or_else(|_| "default".to_string()));
    let secure = std::env::var("PAPERCLIP_PUBLIC_URL").ok().map(|v| v.starts_with("https://")).unwrap_or(false);
    let secure_attr = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}-session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}", prefix, token, max_age, secure_attr))
        .map_err(|_| AuthError::internal("Failed to build session cookie"))
}

async fn sign_up_email(
    State(state): State<AppState>, Json(payload): Json<EmailAuthRequest>,
) -> Result<Response, AuthError> {
    let email = payload.email.trim().to_ascii_lowercase();
    if email.is_empty() || payload.password.len() < 8 { return Err(AuthError::bad_request("Email and a password of at least 8 characters are required")); }
    let repo = PgAuthUserRepository::new(state.pool.clone());
    if repo.find_by_email(&email).await.map_err(|e| AuthError::internal(e.to_string()))?.is_some() {
        return Err(AuthError::bad_request("An account with this email already exists"));
    }
    let mut user = AuthUser::new_with_password(email, password_digest(&payload.password), payload.name);
    user.record_login();
    let user = repo.create(user).await.map_err(|e| AuthError::internal(e.to_string()))?;
    create_session_response(&state, user).await
}

async fn sign_in_email(
    State(state): State<AppState>, Json(payload): Json<EmailAuthRequest>,
) -> Result<Response, AuthError> {
    let email = payload.email.trim().to_ascii_lowercase();
    let repo = PgAuthUserRepository::new(state.pool.clone());
    let mut user = repo.find_by_email(&email).await.map_err(|e| AuthError::internal(e.to_string()))?
        .ok_or_else(|| AuthError::unauthenticated("Invalid email or password"))?;
    if user.password_hash.as_deref() != Some(password_digest(&payload.password).as_str()) || !user.is_active {
        return Err(AuthError::unauthenticated("Invalid email or password"));
    }
    user.record_login();
    let user = repo.update(user).await.map_err(|e| AuthError::internal(e.to_string()))?;
    create_session_response(&state, user).await
}

async fn create_session_response(state: &AppState, user: AuthUser) -> Result<Response, AuthError> {
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let session = AuthSession::new(user.id, token.clone(), 30 * 24 * 60 * 60, None, None);
    PgAuthSessionRepository::new(state.pool.clone()).create(session).await.map_err(|e| AuthError::internal(e.to_string()))?;
    let mut response = Json(json!({"user": {"id": user.id, "email": user.email, "name": user.name}})).into_response();
    response.headers_mut().append(header::SET_COOKIE, session_cookie(&token, 30 * 24 * 60 * 60)?);
    Ok(response)
}

async fn sign_out(
    State(state): State<AppState>, headers: axum::http::HeaderMap,
) -> Result<Response, AuthError> {
    let cookie_name = format!("{}-session", auth_cookie_prefix(&std::env::var("INSTANCE_ID").unwrap_or_else(|_| "default".to_string())));
    if let Some(token) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).and_then(|v| v.split(';').find_map(|p| p.trim().strip_prefix(&format!("{}=", cookie_name)))) {
        if let Some(session) = PgAuthSessionRepository::new(state.pool.clone()).find_by_token(token).await.map_err(|e| AuthError::internal(e.to_string()))? {
            PgAuthSessionRepository::new(state.pool.clone()).delete(session.id).await.map_err(|e| AuthError::internal(e.to_string()))?;
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().append(header::SET_COOKIE, session_cookie("", 0)?);
    Ok(response)
}

/// 构造 `get-session` 的 `session` 视图。
///
/// 对齐 Paperclip `authSessionSchema.session`：
/// - Board 用户：userId / companyId / role / isInstanceAdmin / source。
/// - Agent：agentId / companyId / source。
/// - 匿名：null（前端据此判断未登录）。
pub(crate) fn build_session_view(actor: &AuthorizationActor) -> serde_json::Value {
    match actor {
        AuthorizationActor::Board {
            user_id,
            company_id,
            source,
            memberships,
            is_instance_admin,
        } => {
            let role = memberships
                .iter()
                .find(|m| m.company_id == *company_id && m.status.is_active())
                .map(|m| match m.role {
                    MembershipRole::Owner => "owner",
                    MembershipRole::Admin => "admin",
                    MembershipRole::Operator => "operator",
                    MembershipRole::Viewer => "viewer",
                })
                .unwrap_or("member");
            json!({
                "userId": user_id.to_string(),
                "companyId": company_id.to_string(),
                "role": role,
                "isInstanceAdmin": *is_instance_admin,
                "source": match source {
                    ActorSource::Session => "session",
                    ActorSource::BoardKey => "board_key",
                    ActorSource::LocalImplicit => "local_implicit",
                    ActorSource::CloudTenant => "cloud_tenant",
                    _ => "unknown",
                },
            })
        }
        AuthorizationActor::Agent {
            agent_id,
            company_id,
            source,
            ..
        } => {
            json!({
                "agentId": agent_id.to_string(),
                "companyId": company_id.to_string(),
                "source": match source {
                    ActorSource::AgentKey => "agent_key",
                    ActorSource::AgentJwt => "agent_jwt",
                    _ => "unknown",
                },
            })
        }
        AuthorizationActor::None => serde_json::Value::Null,
    }
}

/// 当前用户资料的 JSON 视图（GET /profile 与 GET /get-session 共用）。
fn user_profile_value(user: &AuthUser, is_instance_admin: bool) -> serde_json::Value {
    json!({
        "id": user.id.to_string(),
        "email": user.email,
        "name": user.name,
        "avatarUrl": user.avatar_url,
        "isInstanceAdmin": is_instance_admin,
    })
}

/// GET /api/auth/get-session
///
/// 对齐 Paperclip `authSessionSchema`：返回 `{ session, user }`。
/// - Board 用户：附带当前用户资料（`user` 非 null）。
/// - Agent / 匿名：`user` 为 null。
async fn get_session(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<Response, AuthError> {
    let session = build_session_view(&actor);
    let user = match &actor {
        AuthorizationActor::Board { user_id, .. } => {
            let repo = PgAuthUserRepository::new(state.pool.clone());
            let loaded = repo
                .find_by_id(*user_id)
                .await
                .map_err(|e| AuthError::internal(format!("Failed to load user: {}", e)))?;
            loaded.map(|u| user_profile_value(&u, actor.is_instance_admin()))
        }
        _ => None,
    };
    Ok(Json(json!({ "session": session, "user": user })).into_response())
}

/// 当前用户资料响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_instance_admin: bool,
}

/// GET /api/auth/profile
///
/// 查询当前 Board 用户资料；未认证返回 401。
async fn get_profile(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AuthError> {
    let user_id = match actor {
        AuthorizationActor::Board { user_id, is_instance_admin, .. } => {
            (user_id, is_instance_admin)
        }
        _ => return Err(AuthError::unauthenticated("Authentication required")),
    };

    let repo = PgAuthUserRepository::new(state.pool.clone());
    let user = repo
        .find_by_id(user_id.0)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load user: {}", e)))?
        .ok_or_else(|| AuthError::unauthenticated("User not found"))?;

    Ok(Json(UserProfile {
        id: user.id,
        email: user.email,
        name: user.name,
        avatar_url: user.avatar_url,
        is_instance_admin: user_id.1,
    }))
}

/// 资料更新请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// PATCH /api/auth/profile
///
/// 更新当前 Board 用户资料（name / avatar_url）；未认证返回 401。
async fn update_profile(
    Extension(actor): Extension<AuthorizationActor>,
    State(state): State<AppState>,
    Json(payload): Json<ProfileUpdate>,
) -> Result<impl IntoResponse, AuthError> {
    let user_id = match actor {
        AuthorizationActor::Board { user_id, .. } => user_id,
        _ => return Err(AuthError::unauthenticated("Authentication required")),
    };

    let repo = PgAuthUserRepository::new(state.pool.clone());
    let mut user = repo
        .find_by_id(user_id)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load user: {}", e)))?
        .ok_or_else(|| AuthError::unauthenticated("User not found"))?;

    if let Some(name) = payload.name {
        user.name = Some(name);
    }
    if let Some(avatar_url) = payload.avatar_url {
        user.avatar_url = Some(avatar_url);
    }

    let updated = repo
        .update(user)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to update user: {}", e)))?;

    Ok(Json(UserProfile {
        id: updated.id,
        email: updated.email,
        name: updated.name,
        avatar_url: updated.avatar_url,
        is_instance_admin: actor.is_instance_admin(),
    }))
}

// ============================================================================
// P3: Admin Handlers (AU1-AU5)
// ============================================================================
//
// 这些管理端点目前未实现真实语义。按对齐约束（handoff：不要用 mock、
// 空数组或固定成功响应冒充生产能力），不再伪造成功，统一返回 501
// feature-disabled 错误。

/// AU1: POST /admin/users/:user_id/promote-instance-admin
async fn promote_instance_admin(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(user_id): Path<Uuid>,
) -> Result<Response, AuthError> {
    assert_instance_admin(&actor)?;
    if !actor.is_board() {
        return Err(AuthError::forbidden("Only a Board user can manage instance administrators"));
    }
    let target = PgAuthUserRepository::new(state.pool.clone())
        .find_by_id(user_id)
        .await
        .map_err(|error| AuthError::internal(error.to_string()))?
        .ok_or_else(|| AuthError::bad_request("Target user does not exist"))?;
    let granted_by = actor
        .principal_id()
        .ok_or_else(|| AuthError::unauthenticated("Authenticated user is required"))?;
    sqlx::query(
        "INSERT INTO instance_user_roles (user_id, role)
         VALUES ($1, 'instance_admin')
         ON CONFLICT (user_id, role) DO NOTHING",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    Ok(Json(json!({
        "userId": target.id,
        "email": target.email,
        "isInstanceAdmin": true,
        "grantedByUserId": granted_by,
    }))
    .into_response())
}

/// AU2: POST /admin/users/:user_id/demote-instance-admin
async fn demote_instance_admin(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(user_id): Path<Uuid>,
) -> Result<Response, AuthError> {
    assert_instance_admin(&actor)?;
    if !actor.is_board() {
        return Err(AuthError::forbidden("Only a Board user can manage instance administrators"));
    }
    if actor.principal_id() == Some(user_id) {
        return Err(AuthError::conflict("An instance administrator cannot demote themselves"));
    }
    let deleted = sqlx::query(
        "DELETE FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin'",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?
    .rows_affected();
    if deleted == 0 {
        return Err(AuthError::bad_request("Target user is not an instance administrator"));
    }
    Ok(Json(json!({
        "userId": user_id,
        "isInstanceAdmin": false,
    }))
    .into_response())
}

/// AU3: GET /admin/users/:user_id/company-access
async fn get_user_company_access(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(user_id): Path<Uuid>,
) -> Result<Response, AuthError> {
    assert_instance_admin(&actor)?;
    let rows = sqlx::query(
        "SELECT cm.company_id, c.name, c.issue_prefix,
                cm.membership_role::text AS role, cm.status::text AS status,
                cm.created_at, cm.updated_at
         FROM company_memberships cm
         JOIN companies c ON c.id = cm.company_id
         WHERE cm.principal_type = 'user'::principal_type
           AND cm.principal_id = $1
         ORDER BY c.name ASC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    use sqlx::Row;
    let access = rows
        .into_iter()
        .map(|row| {
            json!({
                "companyId": row.get::<Uuid, _>("company_id"),
                "companyName": row.get::<String, _>("name"),
                "issuePrefix": row.get::<String, _>("issue_prefix"),
                "role": row.get::<String, _>("role"),
                "status": row.get::<String, _>("status"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "userId": user_id, "access": access })).into_response())
}

/// AU4: PUT /admin/users/:user_id/company-access
async fn update_user_company_access(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, AuthError> {
    assert_instance_admin(&actor)?;
    let company_id = payload
        .get("companyId")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AuthError::bad_request("companyId is required"))
        .and_then(|value| Uuid::parse_str(value).map_err(|_| AuthError::bad_request("companyId must be a UUID")))?;
    let role = payload
        .get("role")
        .and_then(|value| value.as_str())
        .unwrap_or("operator");
    let status = payload
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("active");
    if !matches!(role, "owner" | "admin" | "operator" | "viewer") {
        return Err(AuthError::bad_request("role must be owner, admin, operator, or viewer"));
    }
    if !matches!(status, "active" | "inactive") {
        return Err(AuthError::bad_request("status must be active or inactive"));
    }
    let user_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| AuthError::internal(error.to_string()))?;
    if !user_exists {
        return Err(AuthError::bad_request("Target user does not exist"));
    }
    let company_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM companies WHERE id = $1)")
        .bind(company_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|error| AuthError::internal(error.to_string()))?;
    if !company_exists {
        return Err(AuthError::bad_request("Target company does not exist"));
    }
    let row = sqlx::query(
        "INSERT INTO company_memberships
             (company_id, principal_type, principal_id, membership_role, status)
         VALUES ($1, 'user'::principal_type, $2, $3::membership_role, $4::company_membership_status)
         ON CONFLICT (company_id, principal_type, principal_id)
         DO UPDATE SET membership_role = EXCLUDED.membership_role,
                       status = EXCLUDED.status,
                       updated_at = NOW()
         RETURNING id, company_id, membership_role::text AS role, status::text AS status,
                   created_at, updated_at",
    )
    .bind(company_id)
    .bind(user_id)
    .bind(role)
    .bind(status)
    .fetch_one(&state.pool)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    use sqlx::Row;
    Ok(Json(json!({
        "userId": user_id,
        "membershipId": row.get::<Uuid, _>("id"),
        "companyId": row.get::<Uuid, _>("company_id"),
        "role": row.get::<String, _>("role"),
        "status": row.get::<String, _>("status"),
        "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
        "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
    }))
    .into_response())
}

/// AU5: POST /join-requests/:request_id/claim-api-key
async fn claim_join_request_api_key(
    State(state): State<AppState>,
    Path(request_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, AuthError> {
    let claim_secret = payload
        .get("claimSecret")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AuthError::bad_request("claimSecret is required"))?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|error| AuthError::internal(error.to_string()))?;
    let row = sqlx::query(
        "SELECT company_id, request_type, status::text AS status, created_agent_id,
                claim_secret_hash, claim_secret_expires_at, claim_secret_consumed_at
         FROM join_requests
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?
    .ok_or_else(|| AuthError::bad_request("Join request not found"))?;
    use sqlx::Row;
    let request_type = row.get::<String, _>("request_type");
    if request_type != "agent" {
        return Err(AuthError::bad_request("Only agent join requests can claim API keys"));
    }
    if row.get::<String, _>("status") != "approved" {
        return Err(AuthError::conflict("Join request must be approved before key claim"));
    }
    let agent_id = row
        .get::<Option<Uuid>, _>("created_agent_id")
        .ok_or_else(|| AuthError::conflict("Join request has no created agent"))?;
    let expected_hash = row
        .get::<Option<String>, _>("claim_secret_hash")
        .ok_or_else(|| AuthError::conflict("Join request is missing claim secret metadata"))?;
    let mut digest = Sha256::new();
    digest.update(claim_secret.as_bytes());
    let presented_hash = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if expected_hash != presented_hash {
        return Err(AuthError::forbidden("Invalid claim secret"));
    }
    if let Some(expires_at) = row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("claim_secret_expires_at") {
        if expires_at <= chrono::Utc::now() {
            return Err(AuthError::conflict("Claim secret expired"));
        }
    }
    if row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("claim_secret_consumed_at").is_some() {
        return Err(AuthError::conflict("Claim secret already used"));
    }
    let company_id = row.get::<Uuid, _>("company_id");
    let existing_key: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_api_keys WHERE agent_id = $1)",
    )
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    if existing_key {
        return Err(AuthError::conflict("API key already claimed"));
    }
    let raw_key = format!("aak_{}", Uuid::new_v4().simple());
    let mut key_digest = Sha256::new();
    key_digest.update(raw_key.as_bytes());
    let key_hash = key_digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let key_id = Uuid::new_v4();
    let created_at = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO agent_api_keys
             (id, agent_id, company_id, name, key_hash, scope, created_at, updated_at)
         VALUES ($1, $2, $3, 'initial-join-key', $4, $5, $6, $6)",
    )
    .bind(key_id)
    .bind(agent_id)
    .bind(company_id)
    .bind(key_hash)
    .bind(json!({ "scope_type": "standard", "agent_id": agent_id, "company_id": company_id }))
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    sqlx::query(
        "UPDATE join_requests
         SET claim_secret_consumed_at = $2, updated_at = $2
         WHERE id = $1 AND claim_secret_consumed_at IS NULL",
    )
    .bind(request_id)
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| AuthError::internal(error.to_string()))?;
    tx.commit()
        .await
        .map_err(|error| AuthError::internal(error.to_string()))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "keyId": key_id,
            "token": raw_key,
            "agentId": agent_id,
            "createdAt": created_at,
        })),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::access_test_support::board_with_role;
    use services::auth::MembershipRole;

    #[test]
    fn session_view_includes_company_role_and_source() {
        let actor = board_with_role(Uuid::new_v4(), MembershipRole::Owner);
        let view = build_session_view(&actor);
        assert_eq!(view["role"], "owner");
        assert!(view.get("userId").is_some());
        assert!(view.get("companyId").is_some());
        assert!(view.get("source").is_some());
        assert_eq!(view["isInstanceAdmin"], serde_json::Value::Bool(false));
    }

    #[test]
    fn session_view_for_viewer_uses_read_only_role() {
        let actor = board_with_role(Uuid::new_v4(), MembershipRole::Viewer);
        assert_eq!(build_session_view(&actor)["role"], "viewer");
    }

    #[test]
    fn session_view_for_anonymous_is_null() {
        assert!(build_session_view(&AuthorizationActor::none()).is_null());
    }
}
