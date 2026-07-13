use crate::app_state::AppState;
use crate::errors::AppError;
use axum::{Router, 
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
use uuid::Uuid;

/// POST /companies/:companyId/secrets/remote-import/preview
/// Preview secrets from external provider (scan and detect conflicts)
pub async fn preview_remote_import(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<RemoteSecretImportPreviewRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match state.secret_remote_import_service.preview(company_id, request).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/secrets/remote-import
/// Execute batch import from external provider
pub async fn execute_remote_import(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(request): Json<RemoteSecretImportRequest>,
) -> Response {
    // TODO: Add permission check - assertCanManageSecrets

    match state.secret_remote_import_service.execute(company_id, request).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Router setup for secret remote import endpoints
pub fn secret_remote_import_routes() -> Router<AppState> {
    axum::Router::new()
        .route(
            "/companies/:companyId/secrets/remote-import/preview",
            axum::routing::post(preview_remote_import),
        )
        .route(
            "/companies/:companyId/secrets/remote-import",
            axum::routing::post(execute_remote_import),
        )
}
