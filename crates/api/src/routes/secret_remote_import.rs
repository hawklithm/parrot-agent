use crate::app_state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use models::{RemoteSecretImportPreviewRequest, RemoteSecretImportRequest};
use services::auth::AuthorizationActor;
use uuid::Uuid;

fn service_error_status(error: &services::errors::ServiceError) -> StatusCode {
    use services::errors::ServiceError;
    match error {
        ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
        ServiceError::Validation(_) | ServiceError::InvalidInput(_) | ServiceError::BadRequest(_) => StatusCode::BAD_REQUEST,
        ServiceError::Conflict(_) => StatusCode::CONFLICT,
        ServiceError::Forbidden(_) => StatusCode::FORBIDDEN,
        ServiceError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        ServiceError::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// POST /companies/:companyId/secrets/remote-import/preview
/// Preview secrets from external provider (scan and detect conflicts)
pub async fn preview_remote_import(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<RemoteSecretImportPreviewRequest>,
) -> Response {
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .secret_remote_import_service
        .preview(company_id, request)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (service_error_status(&e), e.to_string()).into_response(),
    }
}

/// POST /companies/:companyId/secrets/remote-import
/// Execute batch import from external provider
pub async fn execute_remote_import(
    Path(company_id): Path<Uuid>,
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Json(request): Json<RemoteSecretImportRequest>,
) -> Response {
    if crate::routes::assert_company_access(&actor, company_id, true).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }

    match state
        .secret_remote_import_service
        .execute(company_id, request)
        .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => (service_error_status(&e), e.to_string()).into_response(),
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
