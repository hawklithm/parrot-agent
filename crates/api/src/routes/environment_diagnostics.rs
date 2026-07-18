use crate::app_state::AppState;
use axum::{Router, 
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::AcquireEnvironmentLeaseRequest;
use uuid::Uuid;

/// POST /environments/:id/probe
/// Probe environment connectivity and health
pub async fn probe(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - user must have access to environment

    match state.environment_diagnostics_service.probe(environment_id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /environments/:id/acquire
/// Acquire exclusive lease for environment access
pub async fn acquire_lease(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<AcquireEnvironmentLeaseRequest>,
) -> Response {
    // TODO: Add permission check - assertCanAccessEnvironment

    match state.environment_diagnostics_service.acquire_lease(environment_id, request).await {
        Ok(lease) => (StatusCode::CREATED, Json(lease)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /environments/:id/delete-blast-radius
/// Analyze impact of deleting an environment
pub async fn delete_blast_radius(
    Path(environment_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    // TODO: Add permission check - assertCanManageEnvironments

    match state.environment_diagnostics_service.delete_blast_radius(environment_id).await {
        Ok(analysis) => (StatusCode::OK, Json(analysis)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for environment diagnostic endpoints
pub fn environment_diagnostics_routes() -> Router<AppState> {
    axum::Router::new()
        .route("/environments/:id/acquire", axum::routing::post(acquire_lease))
        .route(
            "/environments/:id/delete-blast-radius",
            axum::routing::get(delete_blast_radius),
        )
}
