use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "environment_driver", rename_all = "snake_case")]
pub enum EnvironmentDriver {
    Local,
    Ssh,
    Sandbox,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "environment_status", rename_all = "snake_case")]
pub enum EnvironmentStatus {
    Active,
    InUse,
    Provisioning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "lease_status", rename_all = "snake_case")]
pub enum LeaseStatus {
    Active,
    Released,
    Expired,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalEnvironmentConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshEnvironmentConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_workspace_path: String,
    pub private_key: Option<String>,
    pub private_t_ref: Option<String>,
    pub known_hosts: Option<String>,
    pub strict_host_key_checking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEnvironmentConfig {
    pub provider: String,
    pub image: String,
    pub reuse_lease: bool,
    pub stream_run_logs: bool,
    pub timeout_ms: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub driver: EnvironmentDriver,
    pub status: EnvironmentStatus,
    pub config: JsonValue,
    pub env_vars: JsonValue,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EnvironmentLease {
    pub id: Uuid,
    pub company_id: Uuid,
    pub environment_id: Uuid,
    pub execution_workspace_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub status: LeaseStatus,
    pub lease_policy: JsonValue,
    pub provider: Option<String>,
    pub provider_lease_id: Option<String>,
    pub acquired_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub cleanup_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "workspace_mode", rename_all = "snake_case")]
pub enum WorkspaceMode {
    Ephemeral,
    Persistent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "workspace_strategy", rename_all = "snake_case")]
pub enum WorkspaceStrategy {
    Clone,
    Worktree,
    Existing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "workspace_status", rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Pending,
    Ready,
    InUse,
    Cleaning,
    Error,
    Archived,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ExecutionWorkspace {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub source_issue_id: Option<Uuid>,
    pub name: String,
    pub mode: WorkspaceMode,
    pub strategy_type: WorkspaceStrategy,
    pub status: WorkspaceStatus,
    pub cwd: Option<String>,
    pub provider_ref: Option<String>,
    pub base_ref: Option<String>,
    pub branch_name: Option<String>,
    pub repo_url: Option<String>,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeasePolicy {
    pub heartbeat_interval_ms: i64,
    pub max_ttl_ms: Option<i64>,
    pub auto_release_on_expire: bool,
}

impl Default for LeasePolicy {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 30000, // 30 seconds
            max_ttl_ms: Some(3600000),    // 1 hour
            auto_release_on_expire: true,
        }
    }
}
