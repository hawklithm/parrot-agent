use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Environment probe result (diagnostic check)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProbeResult {
    pub ok: bool,
    pub driver: String, // "local" | "ssh" | "sandbox" | "plugin"
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Environment delete blocked reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentDeleteBlockedReason {
    ManagedLocal,
    InstanceDefault,
}

/// Static references summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentStaticReferences {
    pub is_managed_local: bool,
    pub is_instance_default: bool,
    pub agent_default_count: i32,
    pub execution_workspace_selection_count: i32,
    pub issue_selection_count: i32,
    pub project_selection_count: i32,
    pub secret_binding_count: i32,
}

/// Active runtime use summary
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentActiveRuntimeUse {
    pub active_lease_count: i32,
    pub active_custom_image_setup_session_count: i32,
    pub has_active_runtime_use: bool,
}

/// Environment delete blast radius analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentDeleteBlastRadius {
    pub environment_id: Uuid,
    pub can_delete: bool,
    pub delete_blocked_reasons: Vec<EnvironmentDeleteBlockedReason>,
    pub static_references: EnvironmentStaticReferences,
    pub active_runtime_use: EnvironmentActiveRuntimeUse,
}

/// Environment lease status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentLeaseStatus {
    Acquired,
    Active,
    Released,
    Expired,
    Failed,
}

/// Environment lease policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentLeasePolicy {
    Reuse,
    Ephemeral,
}

/// Environment lease cleanup status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentLeaseCleanupStatus {
    Pending,
    Succeeded,
    Failed,
}

/// Environment lease (runtime access token)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentLease {
    pub id: Uuid,
    pub company_id: Uuid,
    pub environment_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_run_id: Option<Uuid>,
    pub status: EnvironmentLeaseStatus,
    pub lease_policy: EnvironmentLeasePolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_lease_id: Option<String>,
    pub acquired_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_status: Option<EnvironmentLeaseCleanupStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to acquire an environment lease
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireEnvironmentLeaseRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_run_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_workspace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_workspace_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_type: Option<String>,
    #[serde(default)]
    pub apply_custom_image_template: bool,
}
