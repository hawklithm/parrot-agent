use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// Issue Tree Control Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_tree_control_mode", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum IssueTreeControlMode {
    Pause,
    Resume,
    Cancel,
    Restore,
}

// Issue Tree Hold Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_tree_hold_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum IssueTreeHoldStatus {
    Active,
    Released,
}

// Issue Tree Hold Release Policy Strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "hold_release_strategy", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum HoldReleasePolicyStrategy {
    Manual,
    AllDone,
    FirstDone,
}

// Issue Tree Hold Release Policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreeHoldReleasePolicy {
    pub strategy: HoldReleasePolicyStrategy,
    pub note: Option<String>,
}

// Issue Tree Hold
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueTreeHold {
    pub id: Uuid,
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub status: IssueTreeHoldStatus,
    pub reason: Option<String>,
    pub release_policy: JsonValue, // JSONB mapped to IssueTreeHoldReleasePolicy
    pub metadata: Option<JsonValue>,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub released_at: Option<DateTime<Utc>>,
    pub released_by_type: Option<String>,
    pub released_by_id: Option<Uuid>,
}

// Issue Tree Hold Member
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueTreeHoldMember {
    pub id: Uuid,
    pub company_id: Uuid,
    pub hold_id: Uuid,
    pub issue_id: Uuid,
    pub parent_issue_id: Option<Uuid>,
    pub depth: i32,
    pub issue_identifier: Option<String>,
    pub issue_title: String,
    pub issue_status: String, // Stored as text, parsed to IssueStatus
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub active_run_id: Option<Uuid>,
    pub active_run_status: Option<String>,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Create Issue Tree Hold Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssueTreeHoldInput {
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: Option<IssueTreeHoldReleasePolicy>,
    pub metadata: Option<JsonValue>,
}

// Issue Tree Control Preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreeControlPreview {
    pub affected_issues: Vec<IssueTreePreviewIssue>,
    pub active_runs: Vec<IssueTreePreviewRun>,
    pub warnings: Vec<IssueTreePreviewWarning>,
}

// Issue Tree Preview Issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreePreviewIssue {
    pub id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub current_status: super::issue::IssueStatus,
    pub target_status: Option<super::issue::IssueStatus>,
    pub depth: i32,
    pub skipped: bool,
    pub skip_reason: Option<String>,
}

// Issue Tree Preview Run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreePreviewRun {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub agent_id: Uuid,
    pub status: String, // "queued", "running"
    pub started_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// Issue Tree Preview Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreePreviewAgent {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub affected_issue_count: i32,
    pub active_run_count: i32,
}

// Issue Tree Preview Warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTreePreviewWarning {
    pub kind: String,
    pub message: String,
    pub issue_id: Option<Uuid>,
}

// Active Pause Hold Gate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveIssueTreePauseHoldGate {
    pub hold_id: Uuid,
    pub root_issue_id: Uuid,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
