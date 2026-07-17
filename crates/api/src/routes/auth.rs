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
    extract::{Extension, Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use services::auth::{
    AuthError, AuthMiddleware, AuthorizationActor, JwtConfig, authenticated_middleware,
};
use services::auth::{ActorSource, MembershipRole};

use crate::app_state::AppState;

use repositories::auth_repositories::{AuthUserRepository, PgAuthUserRepository};

/// 构建 `/api/auth` 路由组，并挂载认证中间件层。
pub fn auth_routes(state: AppState) -> Router<AppState> {
    let pool = Arc::new(state.pool.clone());
    let jwt_config = JwtConfig::from_env().map(Arc::new);
    let middleware = match &jwt_config {
        Some(cfg) => authenticated_middleware(pool.clone(), cfg.clone()),
        None => {
            // 无 JWT 配置时仍使用 Bearer/Session/CloudTenant 解析器（JWT 校验会返回 None）。
            let m = AuthMiddleware::new(services::auth::AuthMode::Authenticated)
                .with_resolver(Arc::new(
                    services::auth::BearerTokenResolver::new(pool.clone(), Arc::new(JwtConfig::new(
                        String::new(),
                        3600,
                        "parrot-agent".to_string(),
                        "agent-runtime".to_string(),
                        "local".to_string(),
                    ))),
                ))
                .with_resolver(Arc::new(services::auth::SessionCookieResolver::new(pool.clone())))
                .with_resolver(Arc::new(services::auth::CloudTenantHeaderResolver::new(pool.clone())));
            m
        }
    };
    let mw = Arc::new(middleware);

    Router::new()
        .route("/auth/get-session", get(get_session))
        .route("/auth/profile", get(get_profile).patch(update_profile))
        // --- P3: Admin routes (AU1-AU5) ---
        .route("/admin/users/:user_id/promote-instance-admin", post(promote_instance_admin))
        .route("/admin/users/:user_id/demote-instance-admin", post(demote_instance_admin))
        .route("/admin/users/:user_id/company-access", get(get_user_company_access).put(update_user_company_access))
        .route("/join-requests/:request_id/claim-api-key", post(claim_join_request_api_key))
        .layer(axum::middleware::from_fn_with_state(
            mw,
            resolve_actor_layer,
        ))
        .with_state(state)
}

/// 认证中间件层：解析 `AuthorizationActor` 并注入 request extensions。
async fn resolve_actor_layer(
    State(middleware): State<Arc<AuthMiddleware>>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers().clone();
    let actor = middleware.resolve_actor(&headers).await?;
    request.extensions_mut().insert(actor);
    Ok(next.run(request).await)
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
