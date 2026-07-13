use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{Router, 
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    CreateEnvironmentCustomImageTerminalSessionTokenRequest,
    EnvironmentCustomImageSetupSessionResult, EnvironmentCustomImageTerminalSessionToken,
};
use services::custom_image_setup_service::CustomImageSetupService;
use uuid::Uuid;

/// GET /environment-custom-image-setup-sessions/:sessionId
/// Get setup session details (status, connection info)
pub async fn get_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - user must have access to environment

    match state.custom_image_setup_service.get_session(session_id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// POST /environment-custom-image-setup-sessions/:sessionId/terminal-session-token
/// Create terminal session token for WebSocket authentication
pub async fn create_terminal_session_token(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<CreateEnvironmentCustomImageTerminalSessionTokenRequest>,
) -> Response {
    // TODO: Add permission check - assertCanAccessEnvironment

    match state.custom_image_setup_service
        .create_terminal_session_token(session_id, request)
        .await
    {
        Ok(token) => (StatusCode::CREATED, Json(token)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for custom image setup endpoints
pub fn custom_image_setup_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/environment-custom-image-setup-sessions/:sessionId",
            axum::routing::get(get_session),
        )
        .route(
            "/environment-custom-image-setup-sessions/:sessionId/terminal-session-token",
            axum::routing::post(create_terminal_session_token),
        )
}
