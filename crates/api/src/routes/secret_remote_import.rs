use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use models::{
    RemoteSecretImportPreviewRequest, RemoteSecretImportPreviewResult, RemoteSecretImportRequest,
    RemoteSecretImportResult,
};
use services::secret_remote_import_service::SecretRemoteImportService;
use std::sync::Arc;
use uuid::Uuid;

/// POST /companies/:companyId/secrets/remote-import/preview
/// Preview secrets from external provider (scan and detect conflicts)
pub async fn preview_remote_import(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretRemoteImportService>>,
    Json(request): Json<RemoteSecretImportPreviewRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.preview(company_id, request).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/secrets/remote-import
/// Execute batch import from external provider
pub async fn execute_remote_import(
    Path(company_id): Path<Uuid>,
    State(service): State<Arc<dyn SecretRemoteImportService>>,
    Json(request): Json<RemoteSecretImportRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match service.execute(company_id, request).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for secret remote import endpoints
pub fn secret_remote_import_routes(
    service: Arc<dyn SecretRemoteImportService>,
) -> axum::Router {
    axum::Router::new()
        .route(
            "/companies/:companyId/secrets/remote-import/preview",
            axum::routing::post(preview_remote_import),
        )
        .route(
            "/companies/:companyId/secrets/remote-import",
            axum::routing::post(execute_remote_import),
        )
        .with_state(service)
}
