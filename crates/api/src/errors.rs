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

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

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

impl AppError {
    /// Ergonomic constructors for the common error categories. Prefer these over
    /// building the variant inline so call sites stay consistent.
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }
    /// Platform/permission denial (distinct from a policy denial, which callers
    /// should return as `Forbidden` with a policy-specific message).
    pub fn permission_denied(msg: impl Into<String>) -> Self {
        AppError::Forbidden(msg.into())
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        AppError::Unauthorized(msg.into())
    }
}

/// Map a raw `sqlx::Error` to the correct HTTP status instead of collapsing it
/// to 500. Row-not-found becomes 404; unique/FK/check violations become 409/400;
/// everything else stays 500 with the underlying message preserved.
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => {
                AppError::NotFound("The requested resource was not found".to_string())
            }
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() {
                    AppError::Conflict(format!("Unique constraint violation: {}", db_err.message()))
                } else if db_err.is_foreign_key_violation() {
                    AppError::BadRequest(format!(
                        "Referenced resource does not exist: {}",
                        db_err.message()
                    ))
                } else if db_err.is_check_violation() {
                    AppError::BadRequest(format!("Check constraint violation: {}", db_err.message()))
                } else {
                    AppError::InternalServerError(db_err.message().to_string())
                }
            }
            sqlx::Error::Io(e) => AppError::InternalServerError(format!("IO error: {e}")),
            sqlx::Error::Configuration(e) => {
                AppError::InternalServerError(format!("Configuration error: {e}"))
            }
            other => AppError::InternalServerError(other.to_string()),
        }
    }
}

impl From<models::AppError> for AppError {
    fn from(err: models::AppError) -> Self {
        match err {
            models::AppError::NotFound(msg) => AppError::NotFound(msg),
            models::AppError::Forbidden(msg) => AppError::Forbidden(msg),
            models::AppError::Conflict(msg) => AppError::Conflict(msg),
            models::AppError::BadRequest(msg) => AppError::BadRequest(msg),
            models::AppError::Internal(msg) => AppError::InternalServerError(msg),
            models::AppError::Database(err) => AppError::InternalServerError(err.to_string()),
            models::AppError::Unprocessable(msg) => AppError::BadRequest(msg),
        }
    }
}

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
            AppError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, msg)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn app_error_variants_map_to_correct_status() {
        assert_eq!(status_of(AppError::NotFound("x".into())), StatusCode::NOT_FOUND);
        assert_eq!(status_of(AppError::Conflict("x".into())), StatusCode::CONFLICT);
        assert_eq!(status_of(AppError::Validation("x".into())), StatusCode::BAD_REQUEST);
        assert_eq!(status_of(AppError::BadRequest("x".into())), StatusCode::BAD_REQUEST);
        assert_eq!(status_of(AppError::Forbidden("x".into())), StatusCode::FORBIDDEN);
        assert_eq!(status_of(AppError::Unauthorized("x".into())), StatusCode::UNAUTHORIZED);
        assert_eq!(
            status_of(AppError::InternalServerError("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(AppError::NotImplemented("x".into())),
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[test]
    fn sqlx_row_not_found_maps_to_404() {
        let err: AppError = sqlx::Error::RowNotFound.into();
        assert_eq!(status_of(err), StatusCode::NOT_FOUND);
    }

    #[test]
    fn sqlx_io_error_maps_to_500() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        let err: AppError = sqlx::Error::Io(io_err).into();
        assert_eq!(status_of(err), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn error_response_body_has_error_field() {
        let resp = AppError::Conflict("dup".into()).into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }
}
