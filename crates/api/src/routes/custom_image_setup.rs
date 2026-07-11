use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use models::custom_image_setup::{SetupSessionResult, TerminalSessionToken};
use services::custom_image_setup_service::CustomImageSetupService;
use std::sync::Arc;
use uuid::Uuid;

/// GET /environment-custom-image-setup-sessions/:sessionId
/// Get setup session details
pub async fn get_setup_session(
    Path(session_id): Path<Uuid>,
    State(service): State<Arc<dyn CustomImageSetupService>>,
) -> Result<Json<SetupSessionResult>, StatusCode> {
    service
        .get_setup_session(session_id)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// POST /environment-custom-image-setup-sessions/:sessionId/terminal-session-token
/// Generate terminal session token for WebSocket authentication
pub async fn create_terminal_session_token(
    Path(session_id): Path<Uuid>,
    State(service): State<Arc<dyn CustomImageSetupService>>,
) -> Result<Json<TerminalSessionToken>, StatusCode> {
    service
        .create_terminal_session_token(session_id)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// Register custom image setup session routes
pub fn custom_image_setup_routes() -> Router<Arc<dyn CustomImageSetupService>> {
    Router::new()
        .route(
            "/environment-custom-image-setup-sessions/:session_id",
            get(get_setup_session),
        )
        .route(
            "/environment-custom-image-setup-sessions/:session_id/terminal-session-token",
            post(create_terminal_session_token),
        )
}
