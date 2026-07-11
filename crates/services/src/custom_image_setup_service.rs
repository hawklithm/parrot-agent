use async_trait::async_trait;
use chrono::Utc;
use models::custom_image_setup::{
    ConnectionPayload, ConnectionType, CustomImageSetupSession, SetupSessionResult,
    SetupSessionStatus, TerminalSessionToken,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::ServiceError;

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Custom image setup service trait
#[async_trait]
pub trait CustomImageSetupService: Send + Sync {
    /// Get setup session details
    async fn get_setup_session(&self, session_id: Uuid) -> ServiceResult<SetupSessionResult>;

    /// Create terminal session token for WebSocket authentication
    async fn create_terminal_session_token(&self, session_id: Uuid) -> ServiceResult<TerminalSessionToken>;
}

/// Default implementation of CustomImageSetupService
pub struct CustomImageSetupServiceImpl {
    // In production: would contain SetupSessionRepository, TerminalSessionStore
}

impl CustomImageSetupServiceImpl {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for CustomImageSetupServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CustomImageSetupService for CustomImageSetupServiceImpl {
    async fn get_setup_session(&self, session_id: Uuid) -> ServiceResult<SetupSessionResult> {
        // Placeholder: In production, would query database
        let now = Utc::now();
        let session = CustomImageSetupSession {
            id: session_id,
            environment_id: Uuid::new_v4(),
            provider: "kubernetes".to_string(),
            status: SetupSessionStatus::WaitingForUser,
            started_by_user_id: Some(Uuid::new_v4()),
            expires_at: Some(now + chrono::Duration::hours(1)),
            finished_at: None,
            failure_reason: None,
            connection_summary: None,
            created_at: now,
            updated_at: now,
        };

        Ok(SetupSessionResult {
            session,
            connection_payload: None,
        })
    }

    async fn create_terminal_session_token(&self, session_id: Uuid) -> ServiceResult<TerminalSessionToken> {
        // Placeholder: In production, would generate JWT and store in TerminalSessionStore
        let now = Utc::now();
        let token_id = Uuid::new_v4().to_string();
        let token = format!("ts_{}", Uuid::new_v4().simple());

        Ok(TerminalSessionToken {
            id: token_id.clone(),
            token,
            expires_at: now + chrono::Duration::minutes(5),
            setup_session_id: session_id,
            environment_id: Uuid::new_v4(),
            connection_type: ConnectionType::Ssh,
            websocket_path: format!("/ws/terminal/{}", token_id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_setup_session() {
        let service = CustomImageSetupServiceImpl::new();
        let session_id = Uuid::new_v4();
        let result = service.get_setup_session(session_id).await;
        assert!(result.is_ok());
        let session_result = result.unwrap();
        assert_eq!(session_result.session.id, session_id);
    }

    #[tokio::test]
    async fn test_create_terminal_session_token() {
        let service = CustomImageSetupServiceImpl::new();
        let session_id = Uuid::new_v4();
        let result = service.create_terminal_session_token(session_id).await;
        assert!(result.is_ok());
        let token = result.unwrap();
        assert_eq!(token.setup_session_id, session_id);
        assert!(token.token.starts_with("ts_"));
    }
}
