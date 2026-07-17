use crate::app_state::AppState;
use axum::{Router, 
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::UserDirectoryQuery;
use uuid::Uuid;

/// GET /companies/:companyId/user-directory
/// List company user directory with search/pagination
pub async fn list_company_user_directory(
    Path(company_id): Path<Uuid>,
    Query(query): Query<UserDirectoryQuery>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - user must be company member

    match state.user_directory_service.list_company_users(company_id, query).await {
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
) -> Response {
    // TODO: Add permission check - assertIsInstanceAdmin

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
