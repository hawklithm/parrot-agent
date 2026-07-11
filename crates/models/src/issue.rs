use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Issue status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum IssueStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Blocked,
    Done,
    Cancelled,
}

/// Issue priority enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum IssuePriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Issue work mode enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum IssueWorkMode {
    Standard,
    Ask,
    Planning,
    SkillTest,
}

/// Issue monitor scheduled by
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum IssueMonitorScheduledBy {
    Assignee,
    Board,
}

/// Issue execution policy (JSONB field)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueExecutionPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_preference: Option<String>,
}

/// Issue execution state (JSONB field)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueExecutionState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Issue core structure
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: IssueStatus,
    pub work_mode: IssueWorkMode,
    pub priority: IssuePriority,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub checkout_run_id: Option<Uuid>,
    pub execution_run_id: Option<Uuid>,
    pub execution_agent_name_key: Option<String>,
    pub execution_locked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pubesponsible_user_id: Option<Uuid>,
    pub issue_number: Option<i32>,
    pub identifier: Option<String>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
    pub origin_run_id: Option<Uuid>,
    pub origin_fingerprint: Option<String>,
    pub request_depth: i32,
    pub billing_code: Option<String>,
    pub execution_policy: Option<sqlx::types::Json<IssueExecutionPolicy>>,
    pub execution_state: Option<sqlx::types::Json<IssueExecutionState>>,
    pub monitor_next_check_at: Option<chrono::DateTime<chrono::Utc>>,
    pub monitor_last_triggered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub monitor_attempt_count: Option<i32>,
    pub monitor_notes: Option<String>,
    pub monitor_scheduled_by: Option<IssueMonitorScheduledBy>,
    pub execution_workspace_id: Option<Uuid>,
    pub execution_workspace_preference: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub cancelled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub hidden_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create issue input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub parent_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub work_mode: Option<IssueWorkMode>,
    pub responsible_user_id: Option<Uuid>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
    pub origin_run_id: Option<Uuid>,
    pub request_depth: Option<i32>,
    pub billing_code: Option<String>,
    pub execution_workspace_id: Option<Uuid>,
    pub execution_workspace_preference: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub assignee_adapter_overrides: Option<serde_json::Value>,
}

/// Update issue input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIssueInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub work_mode: Option<IssueWorkMode>,
    pub responsible_user_id: Option<Uuid>,
    pub source_trust: Option<String>,
    pub monitor_scheduled_by: Option<IssueMonitorScheduledBy>,
    pub monitor_notes: Option<String>,
    pub hidden_at: Option<chrono::DateTime<chrono::Utc>>,
    pub execution_workspace_preference: Option<String>,
    pub execution_workspace_settings: Option<serde_json::Value>,
    pub execution_policy: Option<IssueExecutionPolicy>,
    pub execution_state: Option<IssueExecutionState>,
}

/// Create document input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDocumentInput {
    pub key: String,
    pub content: String,
    pub content_type: Option<String>,
}

/// Update document input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentInput {
    pub content: String,
    pub content_type: Option<String>,
}

/// Document lock information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLock {
    pub locked_by_agent_id: Option<Uuid>,
    pub locked_by_user_id: Option<Uuid>,
    pub locked_at: chrono::DateTime<chrono::Utc>,
    pub run_id: Option<Uuid>,
}

/// Lock document input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockDocumentInput {
    pub run_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub locked_by_type: String,
    pub locked_by_id: Uuid,
}

/// Add comment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCommentInput {
    pub body: String,
    pub reopen_requested: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

/// Comment actor type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum CommentActorType {
    Agent,
    User,
    Board,
    System,
}

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub cursor: Option<String>,
}

/// Issue query filter
#[derive(Debug, Clone, Default)]
pub struct IssueQueryFilter {
    pub status: Option<Vec<IssueStatus>>,
    pub priority: Option<Vec<IssuePriority>>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub work_mode: Option<IssueWorkMode>,
}
