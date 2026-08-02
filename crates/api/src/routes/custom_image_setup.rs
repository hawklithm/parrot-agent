use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::CreateEnvironmentCustomImageTerminalSessionTokenRequest;
use services::auth::AuthorizationActor;
use uuid::Uuid;

/// GET /environment-custom-image-setup-sessions/:sessionId
/// Get setup session details (status, connection info)
pub async fn get_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
) -> Response {
    if let Err(response) = assert_session_access(&state, &actor, session_id, true).await {
        return response;
    }

    match state
        .custom_image_setup_service
        .get_session(session_id)
        .await
    {
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
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<CreateEnvironmentCustomImageTerminalSessionTokenRequest>,
) -> Response {
    if let Err(response) = assert_session_access(&state, &actor, session_id, false).await {
        return response;
    }

    match state
        .custom_image_setup_service
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

async fn assert_session_access(
    state: &AppState,
    actor: &AuthorizationActor,
    session_id: Uuid,
    read_only: bool,
) -> Result<(), Response> {
    let company_id = sqlx::query_scalar::<_, Uuid>("SELECT e.company_id FROM environment_custom_image_setup_sessions s JOIN environments e ON e.id = s.environment_id WHERE s.id = $1")
        .bind(session_id).fetch_optional(&state.pool).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "failed to resolve setup session").into_response())?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "setup session not found").into_response())?;
    crate::routes::assert_company_access(actor, company_id, !read_only)
        .map_err(|_| StatusCode::FORBIDDEN.into_response())
}
