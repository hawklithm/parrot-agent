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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolCatalogEntryKind {
    Tool,
    Prompt,
    Resource,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskLevel {
    Read,
    Write,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolCatalogStatus {
    Active,
    Deprecated,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolProfileStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolDefaultAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolProfileSelectorType {
    Application,
    Connection,
    CatalogEntry,
    ToolName,
    RiskLevel,
    Wildcard,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolProfileEffect {
    Include,
    Exclude,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolProfileBindingTargetType {
    Company,
    Agent,
    Project,
    Issue,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicyType {
    RateLimit,
    Authorization,
    Redaction,
    ApprovalRequired,
    Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolMcpGatewayStatus {
    Active,
    Disabled,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolMcpGatewayDefaultProfileMode {
    GatewayOnly,
    GatewayPlusAgent,
    AgentOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolMcpGatewayContextScopeType {
    None,
    Agent,
    Project,
    Issue,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolGatewaySubjectType {
    GatewayClient,
    Agent,
    Human,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolRuntimeKind {
    LocalStdio,
    RemoteMcp,
    RestApi,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolRuntimeSlotStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolRuntimeHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unchecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationPolicyDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationApprovalState {
    NotRequired,
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolInvocationStatus {
    Pending,
    Running,
    Success,
    Error,
    Denied,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolActionRequestStatus {
    Pending,
    Approved,
    Denied,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolCallEventType {
    PolicyEvaluated,
    ToolCalled,
    ToolCompleted,
    ToolFailed,
    ApprovalRequested,
    ApprovalDecided,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolCallEventOutcome {
    Pending,
    Success,
    Error,
    Denied,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConnectionTokenIssuancePath {
    Exchange,
    OauthAccess,
    Static,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ConnectionTokenIssuanceOutcome {
    Success,
    Denied,
    RateLimited,
    UseEnvLease,
    UpstreamError,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolRateLimitWindowKind {
    Sliding,
    Fixed,
    Daily,
    Hourly,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ToolStdioCommandStatus {
    Active,
    Disabled,
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
// Phase 2: Catalog & Profile Models
// ============================================================================

/// Discovered tool from MCP connection
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolCatalogEntry {
    pub id: Uuid,
    pub company_id: Uuid,
    pub application_id: Option<Uuid>,
    pub connection_id: Uuid,
    pub entry_kind: ToolCatalogEntryKind,
    pub name: String,
    pub tool_name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub annotations: serde_json::Value,
    pub risk_level: ToolRiskLevel,
    pub is_read_only: bool,
    pub is_write: bool,
    pub is_destructive: bool,
    pub status: ToolCatalogStatus,
    pub version: Option<String>,
    pub version_hash: String,
    pub schema_hash: Option<String>,
    pubst_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by_agent_id: Option<Uuid>,
    pub reviewed_by_user_id: Option<String>,
    pub quarantined_at: Option<DateTime<Utc>>,
    pub quarantine_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool permission profile
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolProfile {
    pub id: Uuid,
    pub company_id: Uuid,
    pub profile_key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: ToolProfileStatus,
    pub default_action: ToolDefaultAction,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub new_tools_reviewed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool profile entry (rule within a profile)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolProfileEntry {
    pub id: Uuid,
    pub company_id: Uuid,
    pub profile_id: Uuid,
    pub selector_type: ToolProfileSelectorType,
    pub effect: ToolProfileEffect,
    pub application_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub catalog_entry_id: Option<Uuid>,
    pub tool_name: Option<String>,
    pub risk_level: Option<ToolRiskLevel>,
    pub conditions: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool profile binding to workspace
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolProfileBinding {
    pub id: Uuid,
    pub company_id: Uuid,
    pub profile_id: Uuid,
    pub target_type: ToolProfileBindingTargetType,
    pub target_id: String,
    pub priority: i32,
    pub metadata: serde_json::Value,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool governance policy
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolPolicy {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: ToolPolicyType,
    pub priority: i32,
    pub enabled: bool,
    pub selectors: serde_json::Value,
    pub conditions: Option<serde_json::Value>,
    pub config: Option<serde_json::Value>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Stdio command template
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolStdioCommandTemplate {
    pub id: Uuid,
    pub company_id: Uuid,
    pub template_key: String,
    pub name: String,
    pub description: Option<String>,
    pub status: ToolStdioCommandStatus,
    pub command: String,
    pub args: serde_json::Value,
_keys: serde_json::Value,
    pub tools: serde_json::Value,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Phase 3: MCP Gateway Models
// ============================================================================

/// MCP gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolMcpGateway {
    pub id: Uuid,
    pub company_id: Uuid,
    pub gateway_public_id: String,
    pub name: String,
    pub slug: String,
    pub display_slug: String,
    pub description: Option<String>,
    pub status: ToolMcpGatewayStatus,
    pub profile_id: Uuid,
    pub default_profile_mode: ToolMcpGatewayDefaultProfileMode,
    pub context_scope_type: ToolMcpGatewayContextScopeType,
    pub context_scope_id: Option<String>,
    pub agent_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub approval_issue_id: Option<Uuid>,
    pub auth_config: serde_json::Value,
    pub header_policy: serde_json::Value,
    pub metadata_policy: serde_json::Value,
    pub on_demand_tools_config: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// MCP gateway access token
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolMcpGatewayToken {
    pub id: Uuid,
    pub company_id: Uuid,
    pub gateway_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub subject_type: ToolGatewaySubjectType,
    pub subject_id: Option<String>,
    pub client_label: String,
    pub owner_note: String,
    pub allowed_actions: serde_json::Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub expiry_override_reason: Option<String>,
    pub expiry_override_by_user_id: Option<String>,
    pub expiry_override_by_agent_id: Option<Uuid>,
    pub expiry_override_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Phase 4: Runtime Management Models
// ============================================================================

/// Tool runtime slot (running process)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolRuntimeSlot {
    pub id: Uuid,
    pub company_id: Uuid,
    pub application_id: Option<Uuid>,
    pub connection_id: Uuid,
    pub project_workspace_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub owner_scope_type: String,
    pub owner_scope_id: Option<String>,
    pub runtime_kind: ToolRuntimeKind,
    pub slot_key: String,
    pub status: ToolRuntimeSlotStatus,
    pub reuse_key: Option<String>,
    pub workspace_scope: Option<String>,
    pub credential_scope_hash: Option<String>,
    pub provider: Option<String>,
    pub provider_ref: Option<String>,
    pub process_id: Option<i32>,
    pub command_template_key: Option<String>,
    pub health_status: ToolRuntimeHealthStatus,
    pub health_message: Option<String>,
    pub last_health_check_at: Option<DateTime<Utc>>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub idle_exp: Option<DateTime<Utc>>,
    pub idle_deadline_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Gateway session (active MCP connection)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolGatewaySession {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub run_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub gateway_token_id: Option<Uuid>,
    pub gateway_public_id: Option<String>,
    pub client_subject_type: Option<ToolGatewaySubjectType>,
    pub client_subject_id: Option<String>,
    pub client_name: Option<String>,
    pub mcp_session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Gateway rate limit counter
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolGatewayRateLimitCounter {
    pub id: Uuid,
    pub company_id: Uuid,
    pub counter_key: String,
    pub window_start_at: DateTime<Utc>,
    pub window_ms: i32,
    pub limit_value: i32,
    pub count: i32,
    pub reset_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Phase 5: Invocation Tracking Models
// =========================================================================

/// Toocation record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolInvocation {
    pub id: Uuid,
    pub company_id: Uuid,
    pub idempotency_key: Option<String>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub agent_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub gateway_token_id: Option<Uuid>,
    pub gateway_public_id: Option<String>,
    pub client_subject_type: Option<ToolGatewaySubjectType>,
    pub client_subject_id: Option<String>,
    pub client_name: Option<String>,
    pub mcp_session_id: Option<String>,
  correlation_id: Option<String>,
    pub application_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub catalog_entry_id: Option<Uuid>,
    pub catalog_version_hash: Option<String>,
    pub catalog_schema_hash: Option<String>,
    pub provider_type: Option<String>,
    pub application_key: Option<String>,
    pub upstream_tool_name: Option<String>,
    pub risk_level: Option<ToolRiskLevel>,
    pub tool_name: String,
    pub arguments_hash: Option<String>,
    pub arguments_summary: Option<serde_json::Value>,
    pub policy_decision: Option<ToolInvocationPolicyDecision>,
    pub matched_policy_ids: serde_json::Value,
    pub policy_explanation: Option<serde_json::Value>,
    pub credential_scope_summary: Option<serde_json::Value>,
    pub header_policy_summary: Option<serde_json::Value>,
    pub approval_state: ToolInvocationApprovalState,
    pub status: ToolInvocationStatus,
    pub upstream_request_id: Option<String>,
    pub result_hash: Option<String>,
    pub result_summary: Option<serde_json::Value>,
    pub result_size_bytes: Option<i32>,
    pub result_artifact_id: Option<Uuid>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool action approval request
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolActionRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub invocation_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub interaction_id: Option<Uuid>,
    pub approval_id: Option<Uuid>,
    pub status: ToolActionRequestStatus,
    pub canonical_arguments_hash: String,
    pub canonical_arguments_summary: serde_json::Value,
    pub signed_arguments: Option<String>,
    pub preview_markdown: Option<String>,
    pub requested_by_agent_id: Option<Uuid>,
    pub requested_by_user_id: Option<String>,
    pub resolved_by_agent_id: Option<Uuid>,
    pub resolved_by_user_id: Option<String>,
    pub decided_by_agent_id: Option<Uuid>,
    pub decided_by_user_id: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool call audit event
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolCallEvent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub event_type: ToolCallEventType,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub gateway_id: Option<Uuid>,
    pub gateway_token_id: Option<Uuid>,
    pub gateway_public_id: Option<String>,
    puclient_subject_type: Option<ToolGatewaySubjectType>,
    pub client_subject_id: Option<String>,
    pub client_name: Option<String>,
    pub mcp_session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub application_id: Option<Uuid>,
    pub connection_id: Option<Uuid>,
    pub catalog_entry_id: Option<Uuid>,
    pub invocation_id: Option<Uuid>,
    pub action_request_id: Option<Uuid>,
    pub runtime_slot_id: Option<Uuid>,
    pub tool_name: Option<String>,
    pub decision: Option<ToolInvocationPolicyDecision>,
    pub matched_policy_ids: serde_json::Value,
    pub reason_code: Option<String>,
    pub policy_explanation: Option<serde_json::Value>,
    pub credential_scope_summary: Option<serde_json::Value>,
    pub header_policy_summary: Option<serde_json::Value>,
    pub outcome: ToolCallEventOutcome,
    pub latency_ms: Option<i32>,
    pub arguments_summary: Option<serde_json::Value>,
    pub request_hash: Option<String>,
    pub request_summary: Option<serde_json::Value>,
    pub result_hash: Option<String>,
    pub result_summary: Option<serde_json::Value>,
    pub result_size_bytes: Option<i32>,
    pub redaction_plan: Option<serde_json::Value>,
    pub rate_limit_state: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// OAuth token issuance tracking
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConnectionTokenIssuance {
    pub id: Uuid,
    pub company_id: Uuid,
    pub application_id: Option<Uuid>,
    pub connection_id: Uuid,
    pub agent_id: Uuid,
    pub run_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub responsible_user_id: Option<String>,
    pub path: ConnectionTokenIssuancePath,
    pub requested_scope: serde_json::Value,
    pub issued_scope: serde_json::Value,
    pub ttl_seconds: Option<i32>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_hash: Option<String>,
    pub outcome: ConnectionTokenIssuanceOutcome,
    pub error_code: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Tool rate limit counter
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolRateLimitCounter {
    pub id: Uuid,
    pub company_id: Uuid,
    pub policy_id: Uuid,
    pub counter_key: String,
    pub scope_type: String,
    pub scope_id: String,
    pub window_kind: ToolRateLimitWindowKind,
    pub window_start_at: DateTime<Utc>,
    pub limit_value: i32,
    pub remaining: i32,
    pub reset_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Phase 6: Metrics & Audit Models
// ============================================================================

/// Runtime metric counter
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolRuntimeMetricCounter {
    pub id: Uuid,
    pub company_id: Uuid,
    pub metric: String,
    pub bucket_start_at: DateTime<Utc>,
    pub count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tool access audit event
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolAccessAuditEvent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub gateway_id: Option<Uuid>,
    pub gateway_token_id: Option<Uuid>,
    pub gateway_public_id: Option<String>,
    pub client_name: Option<String>,
    pub correlation_id: Option<String>,
    pub connection_id: Option<Uuid>,
    pub catalog_entry_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub action: String,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub details: serde_json::Value,
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
