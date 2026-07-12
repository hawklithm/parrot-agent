use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{AvailableSkillsResponse, SkillDetails, SkillIndexResponse};
use services::skill_registry_service::SkillRegistryService;
use std::sync::Arc;

/// GET /api/skills/available
/// List all available skills (public access)
pub async fn list_available_skills(
    State(service): State<Arc<dyn SkillRegistryService>>,
) -> Response {
    match service.list_available_skills().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/index
/// Get skill index with metadata (authenticated)
pub async fn get_skill_index(
    State(service): State<Arc<dyn SkillRegistryService>>,
) -> Response {
    // TODO: Add authentication check

    match service.get_skill_index().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /api/skills/:skillName
/// Get skill details with examples (authenticated)
pub async fn get_skill_details(
    Path(skill_name): Path<String>,
    State(service): State<Arc<dyn SkillRegistryService>>,
) -> Response {
    // TODO: Add authentication check

    match service.get_skill_details(&skill_name).await {
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
pub fn skill_routes(service: Arc<dyn SkillRegistryService>) -> axum::Router {
    axum::Router::new()
        .route("/api/skills/available", axum::routing::get(list_available_skills))
        .route("/api/skills/index", axum::routing::get(get_skill_index))
        .route("/api/skills/:skillName", axum::routing::get(get_skill_details))
        .with_state(service)
}
