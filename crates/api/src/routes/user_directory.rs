use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::AdminUserDirectoryQuery;
use services::auth::AuthorizationActor;
use uuid::Uuid;

/// GET /companies/:companyId/user-directory
///
/// Paperclip（`access.ts:4465-4470`）不接收任何查询参数，直接返回
/// `{ users }`（全部活跃用户成员，按成员更新时间倒序）。
pub async fn list_company_user_directory(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .user_directory_service
        .list_company_users(company_id)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            let status = match e {
                services::errors::ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
                services::errors::ServiceError::Unauthorized(_) => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// GET /admin/users
///
/// Paperclip（`access.ts:4783-4841`）返回**裸数组**，前端
/// `parrot-web-ui/src/api/access.ts:402` 亦声明为数组并用
/// `.find()` / `[0]` 取值，故此处同样返回数组而非 `{users,total}`。
pub async fn list_admin_user_directory(
    Query(query): Query<AdminUserDirectoryQuery>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if crate::routes::assert_instance_admin(&actor).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state.user_directory_service.list_admin_users(query).await {
        Ok(response) => (StatusCode::OK, Json(response.users)).into_response(),
        Err(e) => {
            let status = match e {
                services::errors::ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
                services::errors::ServiceError::Unauthorized(_) => StatusCode::FORBIDDEN,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Router setup for user directory endpoints
pub fn user_directory_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/companies/:companyId/user-directory",
            axum::routing::get(list_company_user_directory),
        )
        .route("/admin/users", axum::routing::get(list_admin_user_directory))
}
