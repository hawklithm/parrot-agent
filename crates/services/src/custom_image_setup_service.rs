use async_trait::async_trait;
use models::{
    CreateEnvironmentCustomImageTerminalSessionTokenRequest,
    EnvironmentCustomImageSetupSessionResult, EnvironmentCustomImageTerminalSessionToken,
};
use base64::Engine;
use uuid::Uuid;

use crate::errors::ServiceResult;

/// Service for custom image setup session management
#[async_trait]
pub trait CustomImageSetupService: Send + Sync {
    /// Get setup session details (status, connection info)
    async fn get_session(
        &self,
        session_id: Uuid,
    ) -> ServiceResult<EnvironmentCustomImageSetupSessionResult>;

    /// Create terminal session token for WebSocket authentication
    async fn create_terminal_session_token(
        &self,
        session_id: Uuid,
        request: CreateEnvironmentCustomImageTerminalSessionTokenRequest,
    ) -> ServiceResult<EnvironmentCustomImageTerminalSessionToken>;
}

/// Mock implementation for testing
pub struct MockCustomImageSetupService;

#[async_trait]
impl CustomImageSetupService for MockCustomImageSetupService {
    async fn get_session(
        &self,
        session_id: Uuid,
    ) -> ServiceResult<EnvironmentCustomImageSetupSessionResult> {
        use chrono::Utc;
        use models::{
            EnvironmentCustomImageConnectionPayload, EnvironmentCustomImageSetupConnectionSummary,
            EnvironmentCustomImageSetupConnectionType, EnvironmentCustomImageSetupSession,
            EnvironmentCustomImageSetupSessionStatus,
        };

        let now = Utc::now();
        Ok(EnvironmentCustomImageSetupSessionResult {
            session: EnvironmentCustomImageSetupSession {
                id: session_id,
                environment_id: Uuid::new_v4(),
                template_id: Some(Uuid::new_v4()),
                promoted_template_id: None,
                provider: "fake".to_string(),
                provider_lease_id: Some("lease-123".to_string()),
                environment_lease_id: Some(Uuid::new_v4()),
                status: EnvironmentCustomImageSetupSessionStatus::Running,
                started_by_user_id: Some("user-123".to_string()),
                started_by_agent_id: None,
                base_template_ref: Some("base-image:latest".to_string()),
                expires_at: Some(now + chrono::Duration::hours(2)),
                finished_at: None,
                failure_reason: None,
                connection_summary: Some(EnvironmentCustomImageSetupConnectionSummary {
                    connection_type: EnvironmentCustomImageSetupConnectionType::Ssh,
                    username: Some("root".to_string()),
                    host_redacted: true,
                    port_redacted: true,
                    label: Some("Setup Terminal".to_string()),
                    instructions: Some("Connect via SSH to customize the environment".to_string()),
                }),
                connection_secret_ref: Some("secret-ref-456".to_string()),
                metadata: Some(serde_json::json!({"imageSize": "2.3GB"})),
                created_at: now - chrono::Duration::minutes(10),
                updated_at: now,
            },
            connection_payload: Some(EnvironmentCustomImageConnectionPayload {
                connection_type: "ssh".to_string(),
                command: Some("ssh -p 2222 root@setup-session-abc123.internal".to_string()),
                token: None,
                expires_at: Some(now + chrono::Duration::hours(2)),
                metadata: Some(serde_json::json!({"fingerprint": "SHA256:abcd1234..."})),
            }),
        })
    }

    async fn create_terminal_session_token(
        &self,
        session_id: Uuid,
        _request: CreateEnvironmentCustomImageTerminalSessionTokenRequest,
    ) -> ServiceResult<EnvironmentCustomImageTerminalSessionToken> {
        use chrono::Utc;

        let now = Utc::now();
        let token_id = format!("term-{}", Uuid::new_v4());
        Ok(EnvironmentCustomImageTerminalSessionToken {
            id: token_id.clone(),
            token: format!("mock_token_{}", base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(token_id.as_bytes())),
            expires_at: now + chrono::Duration::minutes(5),
            setup_session_id: session_id.to_string(),
            environment_id: Uuid::new_v4().to_string(),
            connection_type: "ssh".to_string(),
            websocket_path: format!("/ws/custom-image-terminal/{}", token_id),
        })
    }
}
