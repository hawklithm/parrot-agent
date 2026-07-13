use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{Router, 
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{AvailableSkillsResponse, SkillDetails, SkillIndexResponse};
use services::skill_registry_service::SkillRegistryService;

/// GET /api/skills/available
/// List all available skills (public access)
pub async fn list_available_skills(
    State(state): State<AppState>,
) -> Response {
    match state.skill_registry_service.list_available_skills().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/index
/// Get skill index with metadata (authenticated)
pub async fn get_skill_index(
    State(state): State<AppState>,
) -> Response {
    // TODO: Add authentication check

    match state.skill_registry_service.get_skill_index().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/:skillName
/// Get skill details with examples (authenticated)
pub async fn get_skill_details(
    Path(skill_name): Path<String>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add authentication check

    match state.skill_registry_service.get_skill_details(&skill_name).await {
        Ok(details) => (StatusCode::OK, Json(details)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(_) => {
                (StatusCode::NOT_FOUND, e.to_string()).into_response()
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
    }
}

/// Router setup for skills endpoints
pub fn skill_routes() -> Router<AppState> {
    axum::Router::new()
        .route("/api/skills/available", axum::routing::get(list_available_skills))
        .route("/api/skills/index", axum::routing::get(get_skill_index))
        .route("/api/skills/:skillName", axum::routing::get(get_skill_details))
}
