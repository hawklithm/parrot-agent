use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    AcquireEnvironmentLeaseRequest, EnvironmentDeleteBlastRadius, EnvironmentLease,
    EnvironmentProbeResult,
};
use services::environment_diagnostics_service::EnvironmentDiagnosticsService;
use std::sync::Arc;
use uuid::Uuid;

/// POST /environments/:id/probe
/// Probe environment connectivity and health
pub async fn probe(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentDiagnosticsService>>,
) -> Response {
    // TODO: Add permission check - user must have access to environment

    match service.probe(environment_id).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /environments/:id/acquire
/// Acquire exclusive lease for environment access
pub async fn acquire_lease(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentDiagnosticsService>>,
    Json(request): Json<AcquireEnvironmentLeaseRequest>,
) -> Response {
    // TODO: Add permission check - assertCanAccessEnvironment

    match service.acquire_lease(environment_id, request).await {
        Ok(lease) => (StatusCode::CREATED, Json(lease)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// GET /environments/:id/delete-blast-radius
/// Analyze impact of deleting an environment
pub async fn delete_blast_radius(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentDiagnosticsService>>,
) -> Response {
    // TODO: Add permission check - assertCanManageEnvironments

    match service.delete_blast_radius(environment_id).await {
        Ok(analysis) => (StatusCode::OK, Json(analysis)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for environment diagnostic endpoints
pub fn environment_diagnostics_routes(
    service: Arc<dyn EnvironmentDiagnosticsService>,
) -> axum::Router {
    axum::Router::new()
        .route("/environments/:id/probe", axum::routing::post(probe))
        .route("/environments/:id/acquire", axum::routing::post(acquire_lease))
        .route(
            "/environments/:id/delete-blast-radius",
            axum::routing::get(delete_blast_radius),
        )
        .with_state(service)
}
