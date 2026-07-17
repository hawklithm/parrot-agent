use crate::app_state::AppState;
use axum::{Router, 
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::OpenClawInvitePromptRequest;
use uuid::Uuid;

/// POST /companies/:companyId/openclaw/invite-prompt
/// Generate personalized OpenClaw invite prompt
pub async fn generate_invite_prompt(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<OpenClawInvitePromptRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageMembers(companyId, auth)

    match state.openclaw_service.generate_invite_prompt(company_id, request).await {
        Ok(prompt_response) => (StatusCode::OK, Json(prompt_response)).into_response(),
        Err(e) => {
            let status = match e {
                services::errors::ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
                services::errors::ServiceError::Unauthorized(_) => StatusCode::FORBIDDEN,
                services::errors::ServiceError::Conflict(_) => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, e.to_string()).into_response()
        }
    }
}

/// Router setup for OpenClaw endpoints
pub fn openclaw_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/companies/:companyId/openclaw/invite-prompt",
            axum::routing::post(generate_invite_prompt),
        )
}
