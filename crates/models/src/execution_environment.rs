use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Environment Driver Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentDriver {
    Local,
    Ssh,
    Sandbox,
    Plugin,
}

/// Environment Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentStatus {
    Active,
    Archived,
}

/// Execution Environment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionEnvironment {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub driver: EnvironmentDriver,
    pub status: EnvironmentStatus,
    pub config: JsonValue, // JSONB - driver-specific configuration
    pub env_vars: JsonValue, // JSONB - environment variables
    pub metadata: Option<JsonValue>, // JSONB - additional metadata
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ExecutionEnvironment {
    pub fn is_active(&self) -> bool {
        self.status == EnvironmentStatus::Active
    }

    pub fn is_local(&self) -> bool {
        self.driver == EnvironmentDriver::Local
    }
}

/// Environment Lease Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentLeaseStatus {
    Active,
    Expired,
    Released,
    Failed,
}

/// Environment Lease Cleanup Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentLeaseCleanupStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// Environment Lease Policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentLeasePolicy {
    Ephemeral,     // Single-use, cleaned up after release
    Reusable,      // Can be reused across multiple runs
}

/// Runtime Lease
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RuntimeLease {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub status: EnvironmentLeaseStatus,
    pub policy: EnvironmentLeasePolicy,
    pub workspace_id: Option<String>, // External workspace identifier
    pub lease_metadata: Option<JsonValue>, // JSONB - lease-specific metadata
    pub cleanup_status: Option<EnvironmentLeaseCleanupStatus>,
    pub cleanup_error: Option<String>,
    pub acquired_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RuntimeLease {
    pub fn is_active(&self) -> bool {
        self.status == EnvironmentLeaseStatus::Active
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    pub fn is_ephemeral(&self) -> bool {
        self.policy == EnvironmentLeasePolicy::Ephemeral
    }

    pub fn is_reusable(&self) -> bool {
        self.policy == EnvironmentLeasePolicy::Reusable
    }
}

/// Create Environment Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEnvironmentInput {
    pub name: String,
    pub description: Option<String>,
    pub driver: EnvironmentDriver,
    pub status: Option<EnvironmentStatus>,
    pub config: Option<JsonValue>,
    pub env_vars: Option<JsonValue>,
    pub metadata: Option<JsonValue>,
}

/// Update Environment Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEnvironmentInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub driver: Option<EnvironmentDriver>,
    pub status: Option<EnvironmentStatus>,
    pub config: Option<JsonValue>,
    pub env_vars: Option<JsonValue>,
    pub metadata: Option<JsonValue>,
}

/// Create Runtime Lease Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRuntimeLeaseInput {
    pub environment_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub policy: EnvironmentLeasePolicy,
    pub workspace_id: Option<String>,
    pub lease_metadata: Option<JsonValue>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Update Runtime Lease Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRuntimeLeaseInput {
    pub status: Option<EnvironmentLeaseStatus>,
    pub cleanup_status: Option<EnvironmentLeaseCleanupStatus>,
    pub cleanup_error: Option<String>,
    pub released_at: Option<DateTime<Utc>>,
    pub lease_metadata: Option<JsonValue>,
}

/// Environment Probe Result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentProbeResult {
    pub ok: bool,
    pub driver: EnvironmentDriver,
    pub summary: String,
    pub details: Option<JsonValue>,
    pub error: Option<String>,
}

/// Environment Capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentCapabilities {
    pub drivers: Vec<EnvironmentDriver>,
    pub sandbox_providers: Vec<String>,
    pub supports_custom_images: bool,
    pub supports_interactive_setup: bool,
}

/// Environment Delete Blast Radius
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentDeleteBlastRadius {
    pub can_delete: bool,
    pub blocked_reasons: Vec<String>,
    pub affected_agents: i64,
    pub affected_issues: i64,
    pub active_leases: i64,
}

/// Workspace Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceMode {
    Ephemeral,    // Temporary, cleaned up after use
    Persistent,   // Long-lived, retained across runs
}

/// Workspace Strategy Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStrategyType {
    GitWorktree,  // Git worktree-based isolation
    SharedClone,  // Shared clone with branch switching
    Isolated,     // Fully isolated clone
}

/// Workspace Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceStatus {
    Provisioning, // Being created
    Ready,        // Available for use
    Running,      // Actively in use
    Teardown,     // Being cleaned up
    Error,        // Failed state
    Archived,     // Soft deleted
}

/// Execution Workspace
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExecutionWorkspace {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub source_issue_id: Option<Uuid>,
    pub name: String,
    pub mode: WorkspaceMode,
    pub strategy_type: WorkspaceStrategyType,
    pub status: WorkspaceStatus,
    pub cwd: Option<String>,              // Current working directory
    pub provider_ref: Option<String>,     // Provider-specific reference
    pub base_ref: Option<String>,         // Git base ref (branch/commit)
    pub branch_name: Option<String>,      // W branch name
    pub repo_url: Option<String>,         // Repository URL
    pub metadata: Option<JsonValue>,      // JSONB - additional metadata
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ExecutionWorkspace {
    pub fn is_ready(&self) -> bool {
        self.status == WorkspaceStatus::Ready
    }

    pub fn is_running(&self) -> bool {
        self.status == WorkspaceStatus::Running
    }

    pub fn is_ephemeral(&self) -> bool {
        self.mode == WorkspaceMode::Ephemeral
    }

    pub fn is_persistent(&self) -> bool {
        self.mode == WorkspaceMode::Persistent
    }
}

/// Create Execution Workspace Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExecutionWorkspaceInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub source_issue_id: Option<Uuid>,
    pub name: String,
    pub mode: WorkspaceMode,
    pub strategy_type: WorkspaceStrategyType,
    pub cwd: Option<String>,
    pub provider_ref: Option<String>,
    pub base_ref: Option<String>,
    pub branch_name: Option<String>,
    pub repo_url: Option<String>,
    pub metadata: Option<JsonValue>,
}

/// Update Execution Workspace Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateExecutionWorkspaceInput {
    pub name: Option<String>,
    pub status: Option<WorkspaceStatus>,
    pub cwd: Option<String>,
    pub provider_ref: Option<String>,
    pub base_ref: Option<String>,
    pub branch_name: Option<String>,
    pub metadata: Option<JsonValue>,
}
