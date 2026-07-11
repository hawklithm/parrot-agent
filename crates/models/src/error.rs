use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: Option<String>,
    pub source: String,
}

impl PolicyDecision {
    pub fn allow(source: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: None,
            source: source.into(),
        }
    }

    pub fn deny(source: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipUpdateResult {
    pub changed: bool,
    pub change_kind: Option<String>,
}
