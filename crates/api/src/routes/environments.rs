use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use services::environment_service::EnvironmentService;
use std::sync::Arc;
use uuid::Uuid;

/// Environment probe result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProbeResult {
    pub ok: bool,
    pub driver: String,
    pub summary: String,
    pub details: Option<serde_json::Value>,
}

/// Lease acquisition result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseAcquisitionResult {
    pub lease_id: Uuid,
    pub status: String,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Delete blast radius analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBlastRadiusResult {
    pub environment_id: Uuid,
    pub can_delete: bool,
    pub dependent_workspaces: usize,
    pub dependent_agents: Vec<Uuid>,
    pub dependent_issues: Vec<Uuid>,
    pub warnings: Vec<String>,
}

/// POST /environments/:id/probe
/// Probe environment for readiness
pub async fn probe_environment(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentService>>,
) -> Result<Json<EnvironmentProbeResult>, StatusCode> {
    service
        .probe_environment(environment_id)
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

/// POST /environments/:id/acquire
/// Acquire lease for environment
pub async fn acquire_lease(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentService>>,
) -> Result<Json<LeaseAcquisitionResult>, StatusCode> {
    service
        .acquire_lease(environment_id)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else if matches!(e, services::ServiceError::Conflict(_)) {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// GET /environments/:id/delete-blast-radius
/// Analyze impact of deleting environment
pub async fn get_delete_blast_radius(
    Path(environment_id): Path<Uuid>,
    State(service): State<Arc<dyn EnvironmentService>>,
) -> Result<Json<DeleteBlastRadiusResult>, StatusCode> {
    service
        .get_delete_blast_radius(environment_id)
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

/// Register environment diagnostic routes
pub fn environment_diagnostic_routes() -> Router<Arc<dyn EnvironmentService>> {
    Router::new()
        .route("/environments/:id/probe", post(probe_environment))
        .route("/environments/:id/acquire", post(acquire_lease))
        .route(
            "/environments/:id/delete-blast-radius",
            get(get_delete_blast_radius),
        )
}
