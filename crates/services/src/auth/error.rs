use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::decision::DecisionReason;

/// 认证授权统一错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// 未认证（401）
    Unauthenticated {
        message: String,
    },
    /// 禁止访问（403）
    Forbidden {
        reason: String,
        code: Option<String>,
    },
    /// 无效请求（400）
    BadRequest {
        message: String,
    },
    /// 内部错误（500）
    Internal {
        message: String,
    },
    /// Token相关错误
    InvalidToken {
        reason: String,
    },
    /// Token已过期
    TokenExpired,
    /// 会话无效
    InvalidSession {
        reason: String,
    },
    /// API密钥无效
    InvalidApiKey {
        reason: String,
    },
}

impl AuthError {
    /// 创建未认证错误
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::Unauthenticated {
            message: message.into(),
        }
    }

    /// 创建禁止访问错误
    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::Forbidden {
            reason: reason.into(),
            code: None,
        }
    }

    /// 创建禁止访问错误（带错误码）
    pub fn forbidden_with_code(reason: impl Into<String>, code: impl Into<String>) -> Self {
        Self::Forbidden {
            reason: reason.into(),
            code: Some(code.into()),
        }
    }

    /// 创建无效请求错误
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    /// 创建内部错误
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// 创建无效Token错误
    pub fn invalid_token(reason: impl Into<String>) -> Self {
        Self::InvalidToken {
            reason: reason.into(),
        }
    }

    /// 创建Token过期错误
    pub fn token_expired() -> Self {
        Self::TokenExpired
    }

    /// 创建无效会话错误
    pub fn invalid_session(reason: impl Into<String>) -> Self {
        Self::InvalidSession {
            reason: reason.into(),
        }
    }

    /// 创建无效API密钥错误
    pub fn invalid_api_key(reason: impl Into<String>) -> Self {
        Self::InvalidApiKey {
            reason: reason.into(),
        }
    }

    /// 从DecisionReason创建AuthError
    pub fn from_decision_reason(reason: &DecisionReason, explanation: String) -> Self {
        match reason {
            DecisionReason::DenyUnauthenticated => Self::unauthenticated(explanation),
            DecisionReason::DenyMissingPermission { .. }
            | DecisionReason::DenyNotCompanyMember { .. }
            | DecisionReason::DenyInsufficientRole { .. }
            | DecisionReason::DenyCrossCompanyAccess { .. }
            | DecisionReason::DenyApiKeyScopeRestriction { .. }
            | DecisionReason::DenyLowTrustBoundary { .. }
            | DecisionReason::DenyBudgetExceeded { .. }
            | DecisionReason::DenyQuotaExhausted { .. } => {
                Self::forbidden_with_code(explanation, reason.to_string())
            }
            DecisionReason::DenyResourceNotFound { .. } => Self::bad_request(explanation),
            DecisionReason::DenyCustom { .. } => Self::forbidden(explanation),
            _ => Self::internal("Unknown decision reason"),
        }
    }

    /// 获取HTTP状态码
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthenticated { .. }
            | Self::InvalidToken { .. }
            | Self::TokenExpired
            | Self::InvalidSession { .. }
            | Self::InvalidApiKey { .. } => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 获取错误代码（用于客户端国际化）
    pub fn error_code(&self) -> String {
        match self {
            Self::Unauthenticated=> "auth.unauthenticated",
            Self::Forbidden { code: Some(code), .. } => code.clone(),
            Self::Forbidden { .. } => "auth.forbidden",
            Self::BadRequest { .. } => "auth.bad_request",
            Self::Internal { .. } => "auth.internal_error",
            Self::InvalidToken { .. } => "auth.invalid_token",
            Self::TokenExpired => "auth.token_expired",
            Self::InvalidSession { .. } => "auth.invalid_session",
            Self::InvalidApiKey { .. } => "auth.invalid_api_key",
        }
        .to_string()
    }

    /// 获取用户可见的错误消息（隐藏内部实现细节）
    pub fn user_message(&self) -> String {
        match self {
            Self::Unauthenticated { .. } => "Authentication required",
            Self::Forbidden { reason, .. } => reason.clone(),
            Self::BadRequest { message } => message.clone(),
            Self::Internal { .. } => "Internal server error",
            Self::InvalidToken { .. } => "Invalid authentication token",
            Self::TokenExpired => "Authentication token has expired",
            Self::InvalidSession { .. } => "Invalid or expired session",
            Self::InvalidApiKey { .. } => "Invalid API key",
        }
        .to_string()
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for AuthError {}

/// 错误响应结构（JSON序列化）
#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let code = self.error_code();
        let message = self.user_message();

        // 仅在开发模式下返回详细信息（避免泄露内部实现）
        let details = if cfg!(debug_assertions) {
            Some(format!("{:?}", self))
        } else {
            None
        };

        let body = ErrorResponse {
            error: message,
            code,
            details,
        };

        (status, Json(body)).into_response()
    }
}

/// 认证授权结果类型别名
pub type AuthResult<T> = Result<T, AuthError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_unauthenticated() {
        let err = AuthError::unauthenticated("User not logged in");
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "auth.unauthenticated");
    }

    #[test]
    fn test_auth_error_forbidden() {
        let err = AuthError::forbidden("Insufficient permissions");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "auth.forbidden");
    }

    #[test]
    fn test_auth_error_forbidden_with_code() {
        let err = AuthError::forbidden_with_code("Not company member", "auth.not_company_member");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(err.error_code(), "auth.not_company_member");
    }

    #[test]
    fn test_auth_error_bad_request() {
        let err = AuthError::bad_request("Invalid input");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.error_code(), "auth.bad_request");
    }

    #[test]
    fn test_auth_error_internal() {
        let err = AuthError::internal("Database connection failed");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.error_code(), "auth.internal_error");
    }

    #[test]
    fn test_auth_error_token_expired() {
        let err = AuthError::token_expired();
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.error_code(), "auth.token_expired");
    }

    #[test]
    fn test_auth_error_display() {
        let err = AuthError::unauthenticated("Test message");
        assert_eq!(format!("{}", err), "Authentication required");
    }

    #[test]
    fn test_auth_result_ok() {
        let result: AuthResult<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_auth_result_err() {
        let result: AuthResult<i32> = Err(AuthError::unauthenticated("Test"));
        assert!(result.is_err());
    }
}
