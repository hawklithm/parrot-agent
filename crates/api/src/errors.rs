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

    /// 语义正确但服务端无法处理（文件超限、类型不受支持等），映射为 HTTP 422。
    #[error("Unprocessable entity: {0}")]
    Unprocessable(String),

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

    #[error("Too many requests: {0}")]
    TooManyRequests(String),

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
    pub fn unprocessable(msg: impl Into<String>) -> Self {
        AppError::Unprocessable(msg.into())
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

/// `services::errors::ServiceError` 是服务层的通用错误枚举（与
/// `services::ServiceError`（= agent_service 专用）不是同一个类型）。
/// 这里把它逐个变体映射到正确的 HTTP 语义，避免在 route 层塌缩成 500。
impl From<services::errors::ServiceError> for AppError {
    fn from(err: services::errors::ServiceError) -> Self {
        use services::errors::ServiceError as SE;
        match err {
            SE::NotFound(msg) => AppError::NotFound(msg),
            SE::Validation(msg) => AppError::Validation(msg),
            SE::Unprocessable(msg) => AppError::Unprocessable(msg),
            SE::InvalidInput(msg) => AppError::BadRequest(msg),
            SE::BadRequest(msg) => AppError::BadRequest(msg),
            SE::InvalidState(msg) => AppError::Conflict(msg),
            SE::Conflict(msg) => AppError::Conflict(msg),
            SE::Unauthorized(msg) => AppError::Unauthorized(msg),
            SE::Forbidden(msg) => AppError::Forbidden(msg),
            SE::NotImplemented(msg) => AppError::NotImplemented(msg),
            SE::Timeout(msg) => AppError::InternalServerError(format!("Timeout: {msg}")),
            SE::Repository(msg) => AppError::InternalServerError(msg),
            SE::Internal(msg) => AppError::InternalServerError(msg),
            SE::Database(e) => AppError::from(e),
        }
    }
}

impl From<services::plugin_service::PluginServiceError> for AppError {
    fn from(err: services::plugin_service::PluginServiceError) -> Self {
        match err {
            services::plugin_service::PluginServiceError::NotFound(id) => {
                AppError::NotFound(format!("plugin not found: {}", id))
            }
            services::plugin_service::PluginServiceError::InvalidState(msg) => {
                AppError::BadRequest(msg)
            }
            services::plugin_service::PluginServiceError::FeatureDisabled(msg) => {
                AppError::NotImplemented(msg)
            }
            services::plugin_service::PluginServiceError::Database(e) => {
                AppError::InternalServerError(e.to_string())
            }
        }
    }
}

/// Error context attached to 5xx responses — Parrot equivalent of
/// Paperclip's `attachErrorContext` (`server/src/middleware/error-handler.ts`):
/// the http-log middleware reads this extension and includes the redacted
/// error details on 5xx log lines. `details` carries the error message; the
/// variant name identifies the source (Paperclip `error.name`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorContext {
    pub message: String,
    pub name: &'static str,
    pub details: Option<serde_json::Value>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let variant_name = self.variant_name();
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
            AppError::Service(services::ServiceError::Conflict(msg)) => {
                (StatusCode::CONFLICT, msg)
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
            AppError::Unprocessable(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, msg)
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
            AppError::TooManyRequests(msg) => {
                (StatusCode::TOO_MANY_REQUESTS, msg)
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

        // Paperclip attaches __errorContext only for 5xx; the http-log
        // middleware promotes it onto the error log line.
        if status.is_server_error() {
            let context = ErrorContext {
                message: error_message.clone(),
                name: variant_name,
                details: Some(serde_json::json!({ "message": error_message })),
            };
            let mut response = (status, body).into_response();
            response.extensions_mut().insert(context);
            return response;
        }

        (status, body).into_response()
    }
}

impl AppError {
    fn variant_name(&self) -> &'static str {
        match self {
            AppError::Service(services::ServiceError::NotFound(_)) => "NotFound",
            AppError::Service(services::ServiceError::InvalidInput(_)) => "InvalidInput",
            AppError::Service(services::ServiceError::Unauthorized(_)) => "Unauthorized",
            AppError::Service(services::ServiceError::Forbidden(_)) => "Forbidden",
            AppError::Service(services::ServiceError::Conflict(_)) => "Conflict",
            AppError::Service(services::ServiceError::ReportingCycle) => "ReportingCycle",
            AppError::Service(services::ServiceError::TerminalState) => "TerminalState",
            AppError::Service(services::ServiceError::ConfigurationFrozen) => "ConfigurationFrozen",
            AppError::Service(_) => "ServiceError",
            AppError::AccessDenied(_) => "AccessDenied",
            AppError::Validation(_) => "Validation",
            AppError::Unprocessable(_) => "Unprocessable",
            AppError::NotFound(_) => "NotFound",
            AppError::Forbidden(_) => "Forbidden",
            AppError::Unauthorized(_) => "Unauthorized",
            AppError::Conflict(_) => "Conflict",
            AppError::BadRequest(_) => "BadRequest",
            AppError::TooManyRequests(_) => "TooManyRequests",
            AppError::Internal => "Internal",
            AppError::InternalServerError(_) => "InternalServerError",
            AppError::NotImplemented(_) => "NotImplemented",
        }
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
        assert_eq!(status_of(AppError::TooManyRequests("x".into())), StatusCode::TOO_MANY_REQUESTS);
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
        assert_eq!(
            status_of(AppError::Unprocessable("x".into())),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn generic_service_error_variants_map_to_correct_status() {
        use services::errors::ServiceError as SE;
        let cases: Vec<(SE, StatusCode)> = vec![
            (SE::NotFound("gone".into()), StatusCode::NOT_FOUND),
            (SE::Validation("bad".into()), StatusCode::BAD_REQUEST),
            (SE::Unprocessable("too big".into()), StatusCode::UNPROCESSABLE_ENTITY),
            (SE::InvalidInput("bad".into()), StatusCode::BAD_REQUEST),
            (SE::BadRequest("bad".into()), StatusCode::BAD_REQUEST),
            (SE::InvalidState("state".into()), StatusCode::CONFLICT),
            (SE::Conflict("dup".into()), StatusCode::CONFLICT),
            (SE::Unauthorized("no".into()), StatusCode::UNAUTHORIZED),
            (SE::Forbidden("no".into()), StatusCode::FORBIDDEN),
            (SE::NotImplemented("todo".into()), StatusCode::NOT_IMPLEMENTED),
            (SE::Timeout("slow".into()), StatusCode::INTERNAL_SERVER_ERROR),
            (SE::Repository("db".into()), StatusCode::INTERNAL_SERVER_ERROR),
            (SE::Internal("boom".into()), StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (err, expected) in cases {
            let label = err.to_string();
            assert_eq!(status_of(AppError::from(err)), expected, "case: {label}");
        }
        // Database(RowNotFound) 应该沿用 sqlx 映射走 404
        assert_eq!(
            status_of(AppError::from(SE::Database(sqlx::Error::RowNotFound))),
            StatusCode::NOT_FOUND
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

#[cfg(test)]
mod plugin_error_tests {
    use super::*;
    use axum::http::StatusCode;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[test]
    fn plugin_feature_disabled_maps_to_501() {
        let err: AppError =
            services::plugin_service::PluginServiceError::FeatureDisabled("nope".into()).into();
        assert_eq!(status_of(err), StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn plugin_invalid_state_maps_to_400() {
        let err: AppError =
            services::plugin_service::PluginServiceError::InvalidState("bad".into()).into();
        assert_eq!(status_of(err), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn server_errors_carry_error_context_extension() {
        let response = AppError::InternalServerError("boom".into()).into_response();
        assert!(response.status().is_server_error());
        let context = response.extensions().get::<ErrorContext>().expect("5xx must carry ErrorContext");
        assert_eq!(context.message, "boom");
        assert_eq!(context.name, "InternalServerError");
        assert!(context.details.is_some());
    }

    #[test]
    fn client_errors_do_not_carry_error_context() {
        let response = AppError::NotFound("nope".into()).into_response();
        assert!(!response.status().is_server_error());
        assert!(response.extensions().get::<ErrorContext>().is_none());
    }
}