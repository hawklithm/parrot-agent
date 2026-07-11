use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use models::{AvailableSkillsResponse, SkillDetail, SkillIndexEntry};
use services::skills_service::SkillsService;
use std::sync::Arc;

/// GET /api/skills/available
/// List all available skills (public access)
pub async fn list_available_skills(
    State(service): State<Arc<dyn SkillsService>>,
) -> Result<Json<AvailableSkillsResponse>, StatusCode> {
    service
        .list_available_skills()
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /api/skills/index
/// Get skill index with metadata (requires authentication)
pub async fn get_skill_index(
    State(service): State<Arc<dyn SkillsService>>,
) -> Result<Json<Vec<SkillIndexEntry>>, StatusCode> {
    service
        .get_skill_index()
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /api/skills/:skillName
/// Get detailed information about a specific skill (requires authentication)
pub async fn get_skill_details(
    Path(skill_name): Path<String>,
    State(service): State<Arc<dyn SkillsService>>,
) -> Result<Json<SkillDetail>, StatusCode> {
    service
        .get_skill_details(&skill_name)
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

/// Register skills API routes
pub fn skills_routes() -> Router<Arc<dyn SkillsService>> {
    Router::new()
        .route("/skills/available", get(list_available_skills))
        .route("/skills/index", get(get_skill_index))
        .route("/skills/:skill_name", get(get_skill_details))
}
