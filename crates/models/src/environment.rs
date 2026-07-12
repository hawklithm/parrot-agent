use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Environment driver type
#[derive(Debug, Clone, Hash, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum EnvironmentDriver {
    Local,
    Ssh,
    Sandbox,
    Plugin,
}

/// Environment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum EnvironmentStatus {
    Active,
    InUse,
    Provisioning,
    Error,
    Archived,
}

/// Lease status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LeaseStatus {
    Active,
    Released,
    Expired,
    Failed,
}

/// Local environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEnvironmentConfig {
    // Empty placeholder for local environment
}

/// SSH environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshEnvironmentConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_workspace_path: String,
    pub private_key: Option<String>,
    pub private_key_secret_ref: Option<String>,
    pub known_hosts: Option<String>,
    pub strict_host_key_checking: bool,
}

/// Sandbox environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxEnvironmentConfig {
    pub provider: String,
    pub image: String,
    pub reuse_lease: bool,
    pub stream_run_logs: bool,
    pub timeout_ms: Option<i64>,
}

/// Environment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub driver: EnvironmentDriver,
    pub status: EnvironmentStatus,
    pub config: serde_json::Value, // JSONB: driver-specific config
    pub env_vars: serde_json::Value, // JSONB: environment variables
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Environment lease
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentLease {
    pub id: Uuid,
    pub company_id: Uuid,
    pub environment_id: Uuid,
    pub execution_workspace_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub status: LeaseStatus,
    pub lease_policy: Option<serde_json::Value>,
    pub provider: Option<String>,
    pub provider_lease_id: Option<String>,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
    pub failure_reason: Option<String>,
    pub cleanup_status: Option<String>,
}

/// Execution workspace mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum ExecutionWorkspaceMode {
    Ephemeral,
    Persistent,
}

/// Execution workspace strategy type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ExecutionWorkspaceStrategyType {
    CloneAndCheckout,
    ReuseExisting,
    Worktree,
}

/// Execution workspace status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ExecutionWorkspaceStatus {
    Provisioning,
    Ready,
    InUse,
    ConflictRequiresResolution,
    Cleanup,
    Disposed,
}

/// Execution workspace
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionWorkspace {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub source_issue_id: Option<Uuid>,
    pub name: String,
    pub mode: ExecutionWorkspaceMode,
    pub strategy_type: ExecutionWorkspaceStrategyType,
    pub status: ExecutionWorkspaceStatus,
    pub cwd: Option<String>,
    pub provider_ref: Option<String>,
    pub base_ref: Option<String>,
    pub branch_name: Option<String>,
    pub repo_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create environment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentInput {
    pub name: String,
    pub description: Option<String>,
    pub driver: EnvironmentDriver,
    pub status: Option<EnvironmentStatus>,
    pub config: serde_json::Value,
    pub env_vars: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
}

/// Update environment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironmentInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub driver: Option<EnvironmentDriver>,
    pub status: Option<EnvironmentStatus>,
    pub config: Option<serde_json::Value>,
    pub env_vars: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
}
