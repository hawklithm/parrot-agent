use async_trait::async_trait;
use models::{
    CreateEnvironmentCustomImageTerminalSessionTokenRequest,
    EnvironmentCustomImageSetupSession, EnvironmentCustomImageSetupSessionResult,
    EnvironmentCustomImageSetupSessionStatus, EnvironmentCustomImageTerminalSessionToken,
};
use base64::Engine;
use uuid::Uuid;

use crate::errors::ServiceResult;
use sqlx::{PgPool, Row};

pub struct PgCustomImageSetupService { pool: PgPool }
impl PgCustomImageSetupService { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[async_trait]
impl CustomImageSetupService for PgCustomImageSetupService {
    async fn get_session(&self, session_id: Uuid) -> ServiceResult<EnvironmentCustomImageSetupSessionResult> {
        let row = sqlx::query("SELECT id, environment_id, template_id, promoted_template_id, provider, provider_lease_id, environment_lease_id, status, started_by_user_id, started_by_agent_id, base_template_ref, expires_at, finished_at, failure_reason, connection_summary, connection_secret_ref, metadata, created_at, updated_at FROM environment_custom_image_setup_sessions WHERE id = $1").bind(session_id).fetch_optional(&self.pool).await?.ok_or_else(|| crate::errors::ServiceError::NotFound("custom image setup session not found".into()))?;
        let value = serde_json::json!({
            "id": row.get::<Uuid,_>("id"), "environmentId": row.get::<Uuid,_>("environment_id"),
            "templateId": row.get::<Option<Uuid>,_>("template_id"), "promotedTemplateId": row.get::<Option<Uuid>,_>("promoted_template_id"),
            "provider": row.get::<String,_>("provider"), "providerLeaseId": row.get::<Option<String>,_>("provider_lease_id"),
            "environmentLeaseId": row.get::<Option<Uuid>,_>("environment_lease_id"), "status": row.get::<String,_>("status"),
            "startedByUserId": row.get::<Option<String>,_>("started_by_user_id"), "startedByAgentId": row.get::<Option<Uuid>,_>("started_by_agent_id"),
            "baseTemplateRef": row.get::<Option<String>,_>("base_template_ref"), "expiresAt": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),
            "finishedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("finished_at"), "failureReason": row.get::<Option<String>,_>("failure_reason"),
            "connectionSummary": row.get::<Option<serde_json::Value>,_>("connection_summary"), "connectionSecretRef": row.get::<Option<String>,_>("connection_secret_ref"),
            "metadata": row.get::<Option<serde_json::Value>,_>("metadata"), "createdAt": row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"), "updatedAt": row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")
        });
        let session: EnvironmentCustomImageSetupSession = serde_json::from_value(value.clone()).map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        let payload = value.get("metadata").and_then(|m| m.get("connectionPayload")).cloned().map(|v| serde_json::from_value(v).map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))).transpose()?;
        Ok(EnvironmentCustomImageSetupSessionResult { session, connection_payload: payload })
    }
    async fn create_terminal_session_token(&self, session_id: Uuid, _request: CreateEnvironmentCustomImageTerminalSessionTokenRequest) -> ServiceResult<EnvironmentCustomImageTerminalSessionToken> {
        let result = self.get_session(session_id).await?;
        if !matches!(result.session.status, EnvironmentCustomImageSetupSessionStatus::Running) { return Err(crate::errors::ServiceError::InvalidState("setup session is not running".into())); }
        let now = chrono::Utc::now();
        let token_id = Uuid::new_v4().to_string();
        Ok(EnvironmentCustomImageTerminalSessionToken { id: token_id.clone(), token: format!("parrot_{token_id}"), expires_at: result.session.expires_at.unwrap_or(now + chrono::Duration::minutes(5)).min(now + chrono::Duration::minutes(5)), setup_session_id: session_id.to_string(), environment_id: result.session.environment_id.to_string(), connection_type: "ssh".into(), websocket_path: format!("/api/environment-custom-image-setup-sessions/{session_id}/terminal/ws") })
    }
}

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
