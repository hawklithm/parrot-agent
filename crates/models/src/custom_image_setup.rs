use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Custom image setup session status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupSessionStatus {
    Starting,
    WaitingForUser,
    Capturing,
    Completed,
    Failed,
    Cancelled,
}

/// Setup session connection type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Ssh,
}

/// Connection payload for setup session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPayload {
    pub r#type: ConnectionType,
    pub command: Option<String>,
    pub token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

/// Terminal session token response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionToken {
    pub id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub setup_session_id: Uuid,
    pub environment_id: Uuid,
    pub connection_type: ConnectionType,
    pub websocket_path: String,
}

/// Custom image setup session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomImageSetupSession {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub provider: String,
    pub status: SetupSessionStatus,
    pub started_by_user_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub connection_summary: Option<ConnectionPayload>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Setup session with connection payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupSessionResult {
    pub session: CustomImageSetupSession,
    pub connection_payload: Option<ConnectionPayload>,
}
