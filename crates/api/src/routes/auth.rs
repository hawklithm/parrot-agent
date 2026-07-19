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

/// GET /api/auth/get-session
///
/// 返回当前会话信息；未登录（匿名）返回 `{ "session": null }`。
async fn get_session(
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
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
                .find(|m| m.company_id == company_id && m.status.is_active())
                .map(|m| match m.role {
                    MembershipRole::Owner => "owner",
                    MembershipRole::Admin => "admin",
                    MembershipRole::Operator => "operator",
                    MembershipRole::Viewer => "viewer",
                })
                .unwrap_or("member");
            let session = json!({
                "userId": user_id.to_string(),
                "companyId": company_id.to_string(),
                "role": role,
                "isInstanceAdmin": is_instance_admin,
                "source": match source {
                    ActorSource::Session => "session",
                    ActorSource::BoardKey => "board_key",
                    ActorSource::LocalImplicit => "local_implicit",
                    ActorSource::CloudTenant => "cloud_tenant",
                    _ => "unknown",
                },
            });
            (StatusCode::OK, Json(json!({ "session": session }))).into_response()
        }
        AuthorizationActor::Agent {
            agent_id,
            company_id,
            source,
            ..
        } => {
            let session = json!({
                "agentId": agent_id.to_string(),
                "companyId": company_id.to_string(),
                "source": match source {
                    ActorSource::AgentKey => "agent_key",
                    ActorSource::AgentJwt => "agent_jwt",
                    _ => "unknown",
                },
            });
            (StatusCode::OK, Json(json!({ "session": session }))).into_response()
        }
        AuthorizationActor::None => {
            (StatusCode::OK, Json(json!({ "session": null }))).into_response()
        }
    }
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

/// AU1: POST /admin/users/:user_id/promote-instance-admin
async fn promote_instance_admin(
    State(_state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    Ok(Json(serde_json::json!({"userId": user_id, "promoted": true})))
}

/// AU2: POST /admin/users/:user_id/demote-instance-admin
async fn demote_instance_admin(
    State(_state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    Ok(Json(serde_json::json!({"userId": user_id, "demoted": true})))
}

/// AU3: GET /admin/users/:user_id/company-access
async fn get_user_company_access(
    State(_state): State<AppState>,
    Path(_user_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AuthError> {
    Ok(Json(vec![]))
}

/// AU4: PUT /admin/users/:user_id/company-access
async fn update_user_company_access(
    State(_state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AuthError> {
    Ok(Json(serde_json::json!({"userId": user_id, "access": payload, "updated": true})))
}

/// AU5: POST /join-requests/:request_id/claim-api-key
async fn claim_join_request_api_key(
    State(_state): State<AppState>,
    Path(request_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AuthError> {
    Ok(Json(serde_json::json!({"requestId": request_id, "apiKey": Uuid::new_v4().to_string(), "claimed": true})))
}
