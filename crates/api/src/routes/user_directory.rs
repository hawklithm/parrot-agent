use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::UserDirectoryQuery;
use services::auth::AuthorizationActor;
use uuid::Uuid;

/// GET /companies/:companyId/user-directory
/// List company user directory with search/pagination
pub async fn list_company_user_directory(
    Path(company_id): Path<Uuid>,
    Query(query): Query<UserDirectoryQuery>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .user_directory_service
        .list_company_users(company_id, query)
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

/// GET /api/admin/users
/// List instance admin user directory with search filtering
pub async fn list_admin_user_directory(
    Query(query): Query<UserDirectoryQuery>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if crate::routes::assert_instance_admin(&actor).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state.user_directory_service.list_admin_users(query).await {
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

/// Router setup for user directory endpoints
pub fn user_directory_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/companies/:companyId/user-directory",
            axum::routing::get(list_company_user_directory),
        )
        .route(
            "/api/admin/users",
            axum::routing::get(list_admin_user_directory),
        )
}
