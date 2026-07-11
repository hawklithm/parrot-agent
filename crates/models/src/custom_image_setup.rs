use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Setup session status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentCustomImageSetupSessionStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

/// Connection type for setup session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentCustomImageSetupConnectionType {
    Ssh,
}

/// Connection summary (redacted connection details)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCustomImageSetupConnectionSummary {
    #[serde(rename = "type")]
    pub connection_type: EnvironmentCustomImageSetupConnectionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default = "default_true")]
    pub host_redacted: bool,
    #[serde(default = "default_true")]
    pub port_redacted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Environment custom image setup session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCustomImageSetupSession {
    pub id: Uuid,
    pub environment_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_template_id: Option<Uuid>,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_lease_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_lease_id: Option<Uuid>,
    pub status: EnvironmentCustomImageSetupSessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_template_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_summary: Option<EnvironmentCustomImageSetupConnectionSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_secret_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Connection payload (full connection details including credentials)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCustomImageConnectionPayload {
    #[serde(rename = "type")]
    pub connection_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Setup session result (session + connection payload)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCustomImageSetupSessionResult {
    pub session: EnvironmentCustomImageSetupSession,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_payload: Option<EnvironmentCustomImageConnectionPayload>,
}

/// Terminal session token (WebSocket authentication)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCustomImageTerminalSessionToken {
    pub id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub setup_session_id: String,
    pub environment_id: String,
    pub connection_type: String,
    pub websocket_path: String,
}

/// Request to create terminal session token (empty body)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentCustomImageTerminalSessionTokenRequest {}
