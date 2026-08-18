// Tool Access System - Core Models (Phase 1)
// Generated from Paperclip schema: tool_access.ts

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolApplicationType {
    Builtin,
    Plugin,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolApplicationStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionKind {
    Managed,
    Delegated,
    SelfHosted,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionOwnership {
    PlatformShared,
    PlatformProvisioned,
    Customer,
    Dcr,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionTransport {
    McpRemote,
    RestApi,
    LocalStdio,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionAuthKind {
    Oauth,
    ApiKey,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionStatus {
    Draft,
    Active,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unchecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConnectionGrantKind {
    Workspace,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConnectionGrantStatus {
    Active,
    Revoked,
    Expired,
    NeedsReauthorization,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolConnectionInstallTargetType {
    Company,
    Agent,
}

// ============================================================================
// Models
// ============================================================================

/// Tool application definition (e.g. GitHub, Slack)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolApplication {
    pub id: Uuid,
    pub company_id: Uuid,
    pub application_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub application_type: ToolApplicationType,
    pub status: ToolApplicationStatus,
    pub plugin_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub owner_user_id: Option<String>,
    pub metadata: serde_json::Value,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool connection configuration with auth and transport
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolConnection {
    pub id: Uuid,
    pub company_id: Uuid,
    pub application_id: Uuid,
    pub name: String,
    pub uid: String,
    pub connection_kind: ToolConnectionKind,
    pub ownership: ToolConnectionOwnership,
    pub transport: ToolConnectionTransport,
    pub auth_kind: ToolConnectionAuthKind,
    pub status: ToolConnectionStatus,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub transport_config: serde_json::Value,
    pub credential_refs: serde_json::Value,
    pub credential_secret_refs: serde_json::Value,
    pub health_status: ToolConnectionHealthStatus,
    pub health_message: Option<String>,
    pub health_checked_at: Option<DateTime<Utc>>,
    pub last_healthy_at: Option<DateTime<Utc>>,
    pub last_catalog_refresh_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Connection authorization (user or workspace-level)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConnectionGrant {
    pub id: Uuid,
    pub company_id: Uuid,
    pub connection_id: Uuid,
    pub kind: ConnectionGrantKind,
    pub subject_user_id: Option<String>,
    pub provider_tenant: Option<serde_json::Value>,
    pub credential_secret_refs: serde_json::Value,
    pub status: ConnectionGrantStatus,
    pub is_default: bool,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by_agent_id: Option<Uuid>,
    pub revoked_by_user_id: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool installation record (to company or agent)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolConnectionInstall {
    pub id: Uuid,
    pub company_id: Uuid,
    pub connection_id: Uuid,
    pub target_type: ToolConnectionInstallTargetType,
    pub target_id: String,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// OAuth 2.0 state tracking with PKCE support
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolOauthState {
    pub state: String,
    pub company_id: Uuid,
    pub connection_id: Uuid,
    pub code_verifier: String,
    pub created_by_actor_type: Option<String>,
    pub created_by_actor_id: Option<String>,
    pub created_by_session_id: Option<String>,
    pub subject_user_id: Option<String>,
    pub requested_scopes: Option<serde_json::Value>,
    pub return_to: Option<String>,
    pub issue_id: Option<Uuid>,
    pub interaction_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// Input/Output DTOs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolApplicationInput {
    pub application_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub application_type: ToolApplicationType,
    pub plugin_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub owner_user_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateToolApplicationInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<ToolApplicationStatus>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolConnectionInput {
    pub application_id: Uuid,
    pub name: String,
    pub uid: String,
    pub connection_kind: Option<ToolConnectionKind>,
    pub ownership: Option<ToolConnectionOwnership>,
    pub transport: ToolConnectionTransport,
    pub auth_kind: Option<ToolConnectionAuthKind>,
    pub config: Option<serde_json::Value>,
    pub transport_config: Option<serde_json::Value>,
    pub credential_refs: Option<serde_json::Value>,
    pub credential_secret_refs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateToolConnectionInput {
    pub name: Option<String>,
    pub status: Option<ToolConnectionStatus>,
    pub enabled: Option<bool>,
    pub config: Option<serde_json::Value>,
    pub transport_config: Option<serde_json::Value>,
    pub credential_refs: Option<serde_json::Value>,
    pub credential_secret_refs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConnectionGrantInput {
    pub connection_id: Uuid,
    pub kind: ConnectionGrantKind,
    pub subject_user_id: Option<String>,
    pub provider_tenant: Option<serde_json::Value>,
    pub credential_secret_refs: Option<serde_json::Value>,
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolConnectionInstallInput {
    pub connection_id: Uuid,
    pub target_type: ToolConnectionInstallTargetType,
    pub target_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolOauthStateInput {
    pub state: String,
    pub connection_id: Uuid,
    pub code_verifier: String,
    pub created_by_actor_type: Option<String>,
    pub created_by_actor_id: Option<String>,
    pub created_by_session_id: Option<String>,
    pub subject_user_id: Option<String>,
    pub requested_scopes: Option<Vec<String>>,
    pub return_to: Option<String>,
    pub issue_id: Option<Uuid>,
    pub interaction_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

// ============================================================================
// Helper implementations
// ============================================================================

impl ToolApplication {
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

impl ToolConnection {
    pub fn is_healthy(&self) -> bool {
        self.health_status == ToolConnectionHealthStatus::Healthy
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.status == ToolConnectionStatus::Active
    }
}

impl ConnectionGrant {
    pub fn is_active(&self) -> bool {
        self.status == ConnectionGrantStatus::Active && self.revoked_at.is_none()
    }

    pub fn is_user_grant(&self) -> bool {
        self.kind == ConnectionGrantKind::User
    }

    pub fn is_workspace_grant(&self) -> bool {
        self.kind == ConnectionGrantKind::Workspace
    }
}

impl ToolOauthState {
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}
