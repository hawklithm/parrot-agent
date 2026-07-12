use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// AppError - 统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Service error: {0}")]
    Service(#[from] services::ServiceError),

    #[error("Access denied: {0}")]
    AccessDenied(#[from] access::AccessError),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error")]
    Internal,

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

/// ApiError type alias for backwards compatibility
pub type ApiError = AppError;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Service(services::ServiceError::NotFound(msg)) => {
                (StatusCode::NOT_FOUND, msg)
            }
            AppError::Service(services::ServiceError::InvalidInput(msg)) => {
                (StatusCode::BAD_REQUEST, msg)
            }
            AppError::Service(services::ServiceError::Unauthorized(msg)) => {
                (StatusCode::UNAUTHORIZED, msg)
            }
            AppError::Service(services::ServiceError::Forbidden(msg)) => {
                (StatusCode::FORBIDDEN, msg)
            }
            AppError::Service(services::ServiceError::ReportingCycle) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "Reporting cycle detected".to_string())
            }
            AppError::Service(services::ServiceError::TerminalState) => {
                (StatusCode::CONFLICT, "Agent in terminal state".to_string())
            }
            AppError::Service(services::ServiceError::ConfigurationFrozen) => {
                (StatusCode::CONFLICT, "Configuration frozen (pending approval)".to_string())
            }
            AppError::Service(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Service error".to_string())
            }
            AppError::AccessDenied(err) => {
                (StatusCode::FORBIDDEN, err.to_string())
            }
            AppError::Validation(msg) => {
                (StatusCode::BAD_REQUEST, msg)
            }
            AppError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, msg)
            }
            AppError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, msg)
            }
            AppError::Conflict(msg) => {
                (StatusCode::CONFLICT, msg)
            }
            AppError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, msg)
            }
            AppError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            AppError::InternalServerError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
            AppError::NotImplemented(msg) => {
                (StatusCode::NOT_IMPLEMENTED, msg)
            }
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
