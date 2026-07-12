use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Repository error: {0}")]
    Repository(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl From<crate::agent_service::ServiceError> for ServiceError {
    fn from(e: crate::agent_service::ServiceError) -> Self {
        match e {
            crate::agent_service::ServiceError::Repository(re) => {
                ServiceError::Repository(re.to_string())
            }
            crate::agent_service::ServiceError::NotFound(msg) => ServiceError::NotFound(msg),
            crate::agent_service::ServiceError::InvalidInput(msg) => ServiceError::InvalidInput(msg),
            crate::agent_service::ServiceError::Unauthorized(msg) => ServiceError::Unauthorized(msg),
            crate::agent_service::ServiceError::Forbidden(msg) => ServiceError::Forbidden(msg),
            crate::agent_service::ServiceError::Conflict(msg) => ServiceError::Conflict(msg),
            crate::agent_service::ServiceError::Internal(msg) => ServiceError::Internal(msg),
            crate::agent_service::ServiceError::ReportingCycle => {
                ServiceError::InvalidState("Reporting cycle detected".to_string())
            }
            crate::agent_service::ServiceError::TerminalState => {
                ServiceError::InvalidState("Agent in terminal state".to_string())
            }
            crate::agent_service::ServiceError::ConfigurationFrozen => {
                ServiceError::Conflict("Configuration frozen (pending approval)".to_string())
            }
        }
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;
