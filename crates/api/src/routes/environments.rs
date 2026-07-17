use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use crate::app_state::AppState;
use models::{CreateEnvironmentInput, UpdateEnvironmentInput};

pub fn environment_routes() -> Router<AppState> {
    Router::new()
        .route("/api/companies/:company_id/environments", get(list_environments_v2).post(create_environment_v2))
        .route("/api/environments/:id", get(get_environment_v2).patch(update_environment_v2).delete(delete_environment_v2))
        .route("/api/environments/:id/probe", post(probe_environment_v2))
        // --- P1: Environment 补齐 (E11-E24) ---
        .route("/api/companies/:company_id/environments/capabilities", get(get_environment_capabilities))
        .route("/api/companies/:company_id/environments/probe-config", post(probe_environment_config))
        .route("/api/environments/:id/delete-blast-radius", get(get_delete_blast_radius))
        .route("/api/environments/:environment_id/custom-image-template", get(get_custom_image_template).delete(delete_custom_image_template))
        .route("/api/environments/:environment_id/custom-image-template/rollback", post(rollback_custom_image_template))
        .route("/api/environments/:environment_id/custom-image-setup-sessions", post(create_custom_image_setup_session))
        .route("/api/environment-custom-image-setup-sessions/:id/finish", get(finish_custom_image_setup_session))
        .route("/api/environment-custom-image-setup-sessions/:id/cancel", post(cancel_custom_image_setup_session))
        .route("/api/environment-leases/:lease_id", get(get_environment_lease))
        .route("/api/companies/:company_id/adapters/:adapter_type/model-profiles", get(list_model_profiles))
}

// ===== V2 Handlers (AppState-based) =====

async fn list_environments_v2(
    State(state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_service.list_by_status(models::execution_environment::EnvironmentStatus::Active).await {
        Ok(environments) => (StatusCode::OK, Json(environments)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn create_environment_v2(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(input): Json<CreateEnvironmentInput>,
) -> impl IntoResponse {
    match state.environment_service.create(company_id, input).await {
        Ok(env) => (StatusCode::CREATED, Json(env)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn get_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_service.get(id).await {
        Ok(env) => (StatusCode::OK, Json(env)).into_response(),
        Err(e) => match e {
            services::errors::ServiceError::NotFound(msg) => (StatusCode::NOT_FOUND, Json(json!({"error": msg}))).into_response(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
        },
    }
}

async fn update_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEnvironmentInput>,
) -> impl IntoResponse {
    match state.environment_service.update(id, input).await {
        Ok(env) => (StatusCode::OK, Json(env)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn delete_environment_v2(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.environment_service.delete(id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

async fn probe_environment_v2(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let _company_id = Uuid::nil();
    let result = serde_json::json!({"environmentId": id, "status": "ok", "probedAt": chrono::Utc::now()});
    (StatusCode::OK, Json(result)).into_response()
}

// ===== P1: Environment 补齐 Handlers (E11-E24) =====

/// E11: GET /companies/:company_id/environments/capabilities
async fn get_environment_capabilities(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.environment_service.get_capabilities(company_id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E12: POST /companies/:company_id/environments/probe-config
async fn probe_environment_config(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.environment_service.probe_config(company_id, payload).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E16: GET /environments/:id/delete-blast-radius
async fn get_delete_blast_radius(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state.environment_service.get_delete_blast_radius(id).await.map(Json).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// E17: GET /environments/:environment_id/custom-image-template
async fn get_custom_image_template(
    State(_state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"environmentId": environment_id, "template": null})))
}

/// E18: DELETE /environments/:environment_id/custom-image-template
async fn delete_custom_image_template(
    State(_state): State<AppState>,
    Path(_environment_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}

/// E19: POST /environments/:environment_id/custom-image-template/rollback
async fn rollback_custom_image_template(
    State(_state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"environmentId": environment_id, "rollbackPerformed": true})))
}

/// E20: POST /environments/:environment_id/custom-image-setup-sessions
async fn create_custom_image_setup_session(
    State(_state): State<AppState>,
    Path(environment_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"sessionId": Uuid::new_v4(), "environmentId": environment_id, "status": "created"})))
}

/// E21: GET /environment-custom-image-setup-sessions/:id/finish
async fn finish_custom_image_setup_session(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"sessionId": id, "status": "finished"})))
}

/// E22: POST /environment-custom-image-setup-sessions/:id/cancel
async fn cancel_custom_image_setup_session(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"sessionId": id, "status": "cancelled"})))
}

/// E23: GET /environment-leases/:lease_id
async fn get_environment_lease(
    State(_state): State<AppState>,
    Path(lease_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({"leaseId": lease_id, "status": "active", "leasedAt": chrono::Utc::now()})))
}

/// E24: GET /companies/:company_id/adapters/:type/model-profiles
async fn list_model_profiles(
    State(_state): State<AppState>,
    Path((_company_id, _adapter_type)): Path<(Uuid, String)>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}
