use crate::app_state::AppState;
use axum::{Router, 
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::OpenClawInvitePromptRequest;
use uuid::Uuid;
use services::auth::AuthorizationActor;

/// POST /companies/:companyId/openclaw/invite-prompt
/// Generate personalized OpenClaw invite prompt
pub async fn generate_invite_prompt(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    actor: Option<Extension<AuthorizationActor>>,
    Json(request): Json<OpenClawInvitePromptRequest>,
) -> Response {
    let actor = match actor { Some(Extension(actor)) => actor, None => return StatusCode::UNAUTHORIZED.into_response() };
    if actor.company_id() != Some(company_id) { return StatusCode::FORBIDDEN.into_response(); }
    let allowed = match actor {
        AuthorizationActor::Board { is_instance_admin, .. } => is_instance_admin || actor.principal_id().is_some(),
        AuthorizationActor::Agent { agent_id, .. } => sqlx::query_scalar::<_, String>("SELECT role FROM agents WHERE id=$1 AND company_id=$2 AND status <> 'terminated'").bind(agent_id).bind(company_id).fetch_optional(&state.pool).await.map(|role| role.as_deref()==Some("ceo")).unwrap_or(false),
        AuthorizationActor::None => false,
    };
    if !allowed { return StatusCode::FORBIDDEN.into_response(); }

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
