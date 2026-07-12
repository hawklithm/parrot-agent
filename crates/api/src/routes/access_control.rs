//! 访问控制路由（对应任务拆解 §9 阶段一）。
//!
//! 提供 `/api` 下的访问控制路由组：
//! - `GET  /api/board-claim/:token`：查看 Board 认领挑战详情。
//! - `POST /api/board-claim/:token/claim`：认领 Board 所有权（转移实例管理员）。
//! - `POST /api/bootstrap/claim`：首次管理员认领（实例首次运行时将当前用户提升为实例管理员）。
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
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use services::auth::{
    AuthError, AuthMiddleware, AuthorizationActor, BoardClaimService, ClaimChallenge, JwtConfig,
    auth_middleware_fn, authenticated_middleware,
};
use services::auth::authorization_service::assert_instance_admin;

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
        .route(
            "/api/board-claim/:token",
            get(inspect_board_claim).post(claim_board),
        )
        .route("/api/bootstrap/claim", post(bootstrap_claim))
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

