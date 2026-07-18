use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use uuid::Uuid;

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

// ============================================================================
// P2: Skill 补齐 Handlers (SK1-SK38)
// ============================================================================

/// SK1: GET /skills/catalog
async fn get_skill_catalog(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.get_catalog().await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK2: GET /skills/catalog/:catalog_id
async fn get_skill_catalog_detail(
    State(state): State<AppState>,
    Path(catalog_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.get_catalog_detail(catalog_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK3: GET /skills/catalog/files
async fn get_skill_catalog_files(
    State(state): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.get_catalog_files().await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK4: GET /companies/:company_id/skills/categories
async fn list_skill_categories(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.get_categories(company_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK5: GET /companies/:company_id/skills/:skill_id
async fn get_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.get_skill_by_id(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK6: GET /companies/:company_id/skills/:skill_id/fork-precheck
async fn fork_skill_precheck(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.fork_precheck(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK7: GET /companies/:company_id/skills/:skill_id/versions
async fn list_skill_versions(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_skill_versions(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK8: GET /companies/:company_id/skills/:skill_id/versions/:version_id
async fn get_skill_version(
    State(state): State<AppState>,
    Path((company_id, skill_id, version_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.get_skill_version(company_id, skill_id, version_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK9-SK12: Test input management
async fn list_skill_test_inputs(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_test_inputs(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn create_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.skill_registry_service.create_test_input(company_id, skill_id, payload).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id, input_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.update_test_input(company_id, skill_id, input_id, payload).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_input(
    State(state): State<AppState>,
    Path((company_id, skill_id, input_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_test_input(company_id, skill_id, input_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK13-SK16: Test run template management
async fn list_skill_test_run_templates(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_test_run_templates(company_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn create_skill_test_run_template(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.skill_registry_service.create_test_run_template(company_id, payload).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_test_run_template(
    State(state): State<AppState>,
    Path((company_id, template_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.update_test_run_template(company_id, template_id, payload).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_run_template(
    State(state): State<AppState>,
    Path((company_id, template_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_test_run_template(company_id, template_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK17-SK20: Test run management
async fn list_skill_test_runs(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_test_runs(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn get_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.get_test_run(company_id, skill_id, run_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn cancel_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.cancel_test_run(company_id, skill_id, run_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_test_run(
    State(state): State<AppState>,
    Path((company_id, skill_id, run_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_test_run(company_id, skill_id, run_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK21: Star / SK22: Unstar
async fn star_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.star_skill(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn unstar_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.unstar_skill(company_id, skill_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK23: Fork
async fn fork_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.fork_skill(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK24: Audit
async fn audit_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.audit_skill(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK25: Install update
async fn install_skill_update(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.install_skill_update(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK26: Reset
async fn reset_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.reset_skill(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK27: Update status
async fn get_skill_update_status(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.get_skill_update_status(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK28-SK31: Comments
async fn list_skill_comments(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_skill_comments(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn add_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.skill_registry_service.add_skill_comment(company_id, skill_id, payload).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.update_skill_comment(company_id, skill_id, comment_id, payload).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_comment(
    State(state): State<AppState>,
    Path((company_id, skill_id, comment_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_skill_comment(company_id, skill_id, comment_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK32-SK34: Files
async fn list_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    state.skill_registry_service.list_skill_files(company_id, skill_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn update_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.update_skill_files(company_id, skill_id, payload).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn delete_skill_files(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_skill_files(company_id, skill_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// SK35: Import
async fn import_company_skill(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.skill_registry_service.import_skill(company_id, payload).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// SK36: Install catalog
async fn install_skill_catalog(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.install_catalog(company_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK37: Scan projects
async fn scan_skill_projects(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.skill_registry_service.scan_projects(company_id).await.map(Json).map_err(|e| AppError::InternalServerError(e.to_string()))
}

/// SK38: Delete skill
async fn delete_company_skill(
    State(state): State<AppState>,
    Path((company_id, skill_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state.skill_registry_service.delete_skill(company_id, skill_id).await.map_err(|e| AppError::InternalServerError(e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Router setup for skills endpoints
pub fn skill_routes() -> Router<AppState> {
    axum::Router::new()
        .route("/skills/available", axum::routing::get(list_available_skills))
        .route("/skills/index", axum::routing::get(get_skill_index))
        .route("/skills/:skillName", axum::routing::get(get_skill_details))
        // --- P2: SK routes ---
        .route("/skills/catalog", get(get_skill_catalog))
        .route("/skills/catalog/:catalog_id", get(get_skill_catalog_detail))
        .route("/skills/catalog/files", get(get_skill_catalog_files))
        .route("/companies/:company_id/skills/categories", get(list_skill_categories))
        .route("/companies/:company_id/skills/:skill_id", get(get_company_skill).delete(delete_company_skill))
        .route("/companies/:company_id/skills/:skill_id/fork-precheck", get(fork_skill_precheck))
        .route("/companies/:company_id/skills/:skill_id/versions", get(list_skill_versions))
        .route("/companies/:company_id/skills/:skill_id/versions/:version_id", get(get_skill_version))
        .route("/companies/:company_id/skills/:skill_id/test-inputs", get(list_skill_test_inputs).post(create_skill_test_input))
        .route("/companies/:company_id/skills/:skill_id/test-inputs/:input_id", patch(update_skill_test_input).delete(delete_skill_test_input))
        .route("/companies/:company_id/skill-test-run-templates", get(list_skill_test_run_templates).post(create_skill_test_run_template))
        .route("/companies/:company_id/skill-test-run-templates/:template_id", patch(update_skill_test_run_template).delete(delete_skill_test_run_template))
        .route("/companies/:company_id/skills/:skill_id/test-runs", get(list_skill_test_runs))
        .route("/companies/:company_id/skills/:skill_id/test-runs/:run_id", get(get_skill_test_run).delete(delete_skill_test_run))
        .route("/companies/:company_id/skills/:skill_id/test-runs/:run_id/cancel", post(cancel_skill_test_run))
        .route("/companies/:company_id/skills/:skill_id/star", post(star_company_skill).delete(unstar_company_skill))
        .route("/companies/:company_id/skills/:skill_id/fork", post(fork_company_skill))
        .route("/companies/:company_id/skills/:skill_id/audit", post(audit_company_skill))
        .route("/companies/:company_id/skills/:skill_id/install-update", post(install_skill_update))
        .route("/companies/:company_id/skills/:skill_id/reset", post(reset_company_skill))
        .route("/companies/:company_id/skills/:skill_id/update-status", get(get_skill_update_status))
        .route("/companies/:company_id/skills/:skill_id/comments", get(list_skill_comments).post(add_skill_comment))
        .route("/companies/:company_id/skills/:skill_id/comments/:comment_id", patch(update_skill_comment).delete(delete_skill_comment))
        .route("/companies/:company_id/skills/:skill_id/files", get(list_skill_files).patch(update_skill_files).delete(delete_skill_files))
        .route("/companies/:company_id/skills/import", post(import_company_skill))
        .route("/companies/:company_id/skills/install-catalog", post(install_skill_catalog))
        .route("/companies/:company_id/skills/scan-projects", post(scan_skill_projects))
}
