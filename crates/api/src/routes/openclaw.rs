use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{OpenClawInvitePromptRequest, OpenClawInvitePromptResponse};
use services::openclaw_service::OpenClawService;
use std::sync::Arc;
use uuid::Uuid;

/// POST /companies/:companyId/openclaw/invite-prompt
/// Generate personalized OpenClaw invite prompt
pub async fn generate_invite_prompt(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn OpenClawService>>,
    Json(request): Json<OpenClawInvitePromptRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageMembers(companyId, auth)

    match service.generate_invite_prompt(company_id, request).await {
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
pub fn openclaw_routes(service: Arc<dyn OpenClawService>) -> axum::Router {
    axum::Router::new()
        .route(
            "/companies/:companyId/openclaw/invite-prompt",
            axum::routing::post(generate_invite_prompt),
        )
        .with_state(service)
}
