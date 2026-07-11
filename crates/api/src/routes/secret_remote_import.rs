use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use services::secret_provider_service::SecretProviderConfigService;
use models::secret_provider::{ConflictResolution, RemoteImportPreview, RemoteImportResult};
use std::sync::Arc;
use uuid::Uuid;

/// Remote import preview request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImportPreviewRequest {
    pub config_id: Uuid,
    pub filters: Option<serde_json::Value>,
}

/// Remote import execution request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImportExecuteRequest {
    pub config_id: Uuid,
    pub secret_keys: Vec<String>,
    pub conflict_resolution: ConflictResolution,
}

/// POST /companies/:companyId/secrets/remote-import/preview
/// Preview secrets from remote provider before importing
pub async fn remote_import_preview(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
    Json(request): Json<RemoteImportPreviewRequest>,
) -> Result<Json<RemoteImportPreview>, StatusCode> {
    service
        .remote_import_preview(company_id, request.config_id, request.filters)
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// POST /companies/:companyId/secrets/remote-import
/// Execute remote import of secrets from external provider
pub async fn remote_import_execute(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretProviderConfigService>>,
    Json(request): Json<RemoteImportExecuteRequest>,
) -> Result<Json<RemoteImportResult>, StatusCode> {
    // TODO: Extract created_by_user_id from auth context
    let created_by_user_id = Uuid::new_v4(); // Placeholder

    service
        .remote_import_execute(
            company_id,
            request.config_id,
            request.secret_keys,
            request.conflict_resolution,
            created_by_user_id,
        )
        .await
        .map(Json)
        .map_err(|e| {
            if matches!(e, services::ServiceError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else if matches!(e, services::ServiceError::Unauthorized(_)) {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })
}

/// Register secret remote import routes
pub fn secret_remote_import_routes() -> Router<Arc<dyn SecretProviderConfigService>> {
    Router::new()
        .route(
            "/companies/:company_id/secrets/remote-import/preview",
            post(remote_import_preview),
        )
        .route(
            "/companies/:company_id/secrets/remote-import",
            post(remote_import_execute),
        )
}
