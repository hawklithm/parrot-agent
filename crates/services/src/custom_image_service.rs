use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CustomImageError {
    #[error("Environment not found: {0}")]
    EnvironmentNotFound(Uuid),

    #[error("Setup session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("Invalid session status: expected {expected}, got {actual}")]
    InvalidSessionStatus { expected: String, actual: String },

    #[error("Session expired: {0}")]
    SessionExpired(Uuid),

    #[error("Token generation failed: {0}")]
    TokenGenerationFailed(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type CustomImageResult<T> = Result<T, CustomImageError>;

/// Setup session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SetupSessionStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
    Failed,
}

impl std::fmt::Display for SetupSessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupSessionStatus::Pending => write!(f, "pending"),
            SetupSessionStatus::InProgress => write!(f, "in_progress"),
            SetupSessionStatus::Completed => write!(f, "completed"),
            SetupSessionStatus::Cancelled => write!(f, "cancelled"),
            SetupSessionStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Custom image setup session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImageSetupSession {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub company_id: Uuid,
    pub status: SetupSessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
}

/// Terminal session token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSessionToken {
    pub session_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Custom image template overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImageTemplate {
    pub environment_id: Uuid,
    pub base_image: String,
    pub current_version: Option<String>,
    pub last_built_at: Option<DateTime<Utc>>,
    pub status: String,
}

/// Create setup session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSetupSessionRequest {
    pub environment_id: Uuid,
    pub company_id: Uuid,
}

/// Finish setup session request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishSetupSessionRequest {
    pub session_id: Uuid,
    pub success: bool,
    pub failure_reason: Option<String>,
}

/// Custom image service trait
#[async_trait]
pub trait CustomImageService: Send + Sync {
    /// Create a new setup session
    async fn create_setup_session(
        &self,
        request: CreateSetupSessionRequest,
    ) -> CustomImageResult<CustomImageSetupSession>;

    /// Get setup session by ID
    async fn get_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession>;

    /// Start setup session (transition from pending to in_progress)
    async fn start_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession>;

    /// Finish setup session (transition to completed or failed)
    async fn finish_setup_session(
        &self,
        request: FinishSetupSessionRequest,
    ) -> CustomImageResult<CustomImageSetupSession>;

    /// Cancel setup session
    async fn cancel_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession>;

    /// Get custom image template overview
    async fn get_image_template(&self, environment_id: Uuid) -> CustomImageResult<CustomImageTemplate>;

    /// Create terminal session token for WebSocket access
    async fn create_terminal_token(&self, session_id: Uuid) -> CustomImageResult<TerminalSessionToken>;

    /// Validate terminal session token
    async fn validate_terminal_token(&self, token: &str) -> CustomImageResult<Uuid>;
}

/// Default implementation of custom image service
pub struct DefaultCustomImageService {
    session_timeout_minutes: i64,
    terminal_token_ttl_seconds: i64,
}

impl DefaultCustomImageService {
    pub fn new(session_timeout_minutes: i64, terminal_token_ttl_seconds: i64) -> Self {
        Self {
            session_timeout_minutes,
            terminal_token_ttl_seconds,
        }
    }

    pub fn with_defaults() -> Self {
        Self {
            session_timeout_minutes: 30,
            terminal_token_ttl_seconds: 300, // 5 minutes
        }
    }

    /// Check if session is expired
    fn is_session_expired(&self, session: &CustomImageSetupSession) -> bool {
        if session.status != SetupSessionStatus::Pending && session.status != SetupSessionStatus::InProgress {
            return false;
        }

        let now = Utc::now();
        let elapsed = now.signed_duration_since(session.created_at);
        elapsed.num_minutes() >= self.session_timeout_minutes
    }

    /// Generate JWT token for terminal access
    fn generate_terminal_token(&self, session_id: Uuid) -> CustomImageResult<String> {
        // TODO: Implement JWT token generation
        // For now, return a simple token format
        let token = format!("terminal_{}_{}", session_id, Utc::now().timestamp());
        Ok(token)
    }

    /// Parse and validate terminal token
    fn parse_terminal_token(&self, token: &str) -> CustomImageResult<Uuid> {
        // TODO: Implement JWT token validation
        // For now, parse simple token format
        if let Some(parts) = token.strip_prefix("terminal_") {
            if let Some((session_id_str, _timestamp)) = parts.split_once('_') {
                if let Ok(session_id) = Uuid::parse_str(session_id_str) {
                    return Ok(session_id);
                }
            }
        }

        Err(CustomImageError::TokenGenerationFailed(
            "Invalid token format".to_string(),
        ))
    }
}

#[async_trait]
impl CustomImageService for DefaultCustomImageService {
    async fn create_setup_session(
        &self,
        request: CreateSetupSessionRequest,
    ) -> CustomImageResult<CustomImageSetupSession> {
        let now = Utc::now();
        let session = CustomImageSetupSession {
            id: Uuid::new_v4(),
            environment_id: request.environment_id,
            company_id: request.company_id,
            status: SetupSessionStatus::Pending,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            failure_reason: None,
        };

        // TODO: Persist to database
        Ok(session)
    }

    async fn get_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession> {
        // TODO: Load from database
        Err(CustomImageError::SessionNotFound(session_id))
    }

    async fn start_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession> {
        // TODO: Load session from database
        let mut session = self.get_setup_session(session_id).await?;

        if session.status != SetupSessionStatus::Pending {
            return Err(CustomImageError::InvalidSessionStatus {
                expected: "pending".to_string(),
                actual: session.status.to_string(),
            });
        }

        if self.is_session_expired(&session) {
            return Err(CustomImageError::SessionExpired(session_id));
        }

        let now = Utc::now();
        session.status = SetupSessionStatus::InProgress;
        session.started_at = Some(now);
        session.updated_at = now;

        // TODO: Update in database
        Ok(session)
    }

    async fn finish_setup_session(
        &self,
        request: FinishSetupSessionRequest,
    ) -> CustomImageResult<CustomImageSetupSession> {
        // TODO: Load session from database
        let mut session = self.get_setup_session(request.session_id).await?;

        if session.status != SetupSessionStatus::InProgress {
            return Err(CustomImageError::InvalidSessionStatus {
                expected: "in_progress".to_string(),
                actual: session.status.to_string(),
            });
        }

        let now = Utc::now();
        session.status = if request.success {
            SetupSessionStatus::Completed
        } else {
            SetupSessionStatus::Failed
        };
        session.completed_at = Some(now);
        session.updated_at = now;
        session.failure_reason = request.failure_reason;

        // TODO: Update in database
        Ok(session)
    }

    async fn cancel_setup_session(&self, session_id: Uuid) -> CustomImageResult<CustomImageSetupSession> {
        // TODO: Load session from database
        let mut session = self.get_setup_session(session_id).await?;

        if session.status == SetupSessionStatus::Completed || session.status == SetupSessionStatus::Failed {
            return Err(CustomImageError::InvalidSessionStatus {
                expected: "pending or in_progress".to_string(),
                actual: session.status.to_string(),
            });
        }

        let now = Utc::now();
        session.status = SetupSessionStatus::Cancelled;
        session.updated_at = now;

        // TODO: Update in database
        Ok(session)
    }

    async fn get_image_template(&self, environment_id: Uuid) -> CustomImageResult<CustomImageTemplate> {
        // TODO: Load from database or environment configuration
        Ok(CustomImageTemplate {
            environment_id,
            base_image: "ubuntu:22.04".to_string(),
            current_version: None,
            last_built_at: None,
            status: "not_built".to_string(),
        })
    }

    async fn create_terminal_token(&self, session_id: Uuid) -> CustomImageResult<TerminalSessionToken> {
        // Verify session exists and is in progress
        let session = self.get_setup_session(session_id).await?;

        if session.status != SetupSessionStatus::InProgress {
            return Err(CustomImageError::InvalidSessionStatus {
                expected: "in_progress".to_string(),
                actual: session.status.to_string(),
            });
        }

        let token = self.generate_terminal_token(session_id)?;
        let expires_at = Utc::now() + chrono::Duration::seconds(self.terminal_token_ttl_seconds);

        Ok(TerminalSessionToken {
            session_id,
            token,
            expires_at,
        })
    }

    async fn validate_terminal_token(&self, token: &str) -> CustomImageResult<Uuid> {
        let session_id = self.parse_terminal_token(token)?;

        // Verify session exists and is still active
        let session = self.get_setup_session(session_id).await?;

        if session.status != SetupSessionStatus::InProgress {
            return Err(CustomImageError::InvalidSessionStatus {
                expected: "in_progress".to_string(),
                actual: session.status.to_string(),
            });
        }

        Ok(session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_setup_session() {
        let service = DefaultCustomImageService::with_defaults();

        let request = CreateSetupSessionRequest {
            environment_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        };

        let session = service.create_setup_session(request).await.unwrap();
        assert_eq!(session.status, SetupSessionStatus::Pending);
        assert!(session.started_at.is_none());
        assert!(session.completed_at.is_none());
    }

    #[tokio::test]
    async fn test_session_status_transitions() {
        let service = DefaultCustomImageService::with_defaults();

        let request = CreateSetupSessionRequest {
            environment_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        };

        let session = service.create_setup_session(request).await.unwrap();
        assert_eq!(session.status, SetupSessionStatus::Pending);
    }

    #[test]
    fn test_terminal_token_generation() {
        let service = DefaultCustomImageService::with_defaults();
        let session_id = Uuid::new_v4();

        let token = service.generate_terminal_token(session_id).unwrap();
        assert!(token.starts_with("terminal_"));

        let parsed_id = service.parse_terminal_token(&token).unwrap();
        assert_eq!(parsed_id, session_id);
    }
}
