use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Issue tree control mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum IssueTreeControlMode {
    Pause,
    Resume,
    Cancel,
    Restore,
}

/// Issue tree hold release policy strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueTreeHoldReleasePolicyStrategy {
    Manual,
    AllDone,
    FirstDone,
}

/// Issue tree hold release policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeHoldReleasePolicy {
    pub strategy: IssueTreeHoldReleasePolicyStrategy,
    pub note: Option<String>,
}

/// Issue tree hold
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeHold {
    pub id: Uuid,
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub status: IssueTreeHoldStatus,
    pub reason: Option<String>,
    pub release_policy: sqlx::types::Json<IssueTreeHoldReleasePolicy>,
    pub metadata: Option<serde_json::Value>,
    pub actor_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Issue tree hold status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum IssueTreeHoldStatus {
    Active,
    Released,
    Expired,
}

/// Issue tree hold member
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeHoldMember {
    pub id: Uuid,
    pub hold_id: Uuid,
    pub issue_id: Uuid,
    pub previous_status: String,
    pub company_id: Uuid,
    pub parent_issue_id: Option<Uuid>,
    pub issue_identifier: Option<String>,
    pub issue_title: String,
    pub issue_status: String,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub active_run_id: Option<Uuid>,
    pub active_run_status: Option<String>,
    pub depth: i32,
    pub skipped: bool,
    pub skip_reason: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Create issue tree hold input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIssueTreeHoldInput {
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: IssueTreeHoldReleasePolicy,
    pub metadata: Option<serde_json::Value>,
}

/// Affected issue in tree control preview
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedIssue {
    pub issue_id: Uuid,
    pub current_status: String,
    pub target_status: String,
}

/// Preview active run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewActiveRun {
    pub run_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub issue_id: Uuid,
}

/// Issue tree control preview
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeControlPreview {
    pub affected_issues: Vec<AffectedIssue>,
    pub active_runs: Vec<PreviewActiveRun>,
    pub status_changes: Vec<AffectedIssue>,
}

/// Issue tree preview issue (alias for AffectedIssue)
pub type IssueTreePreviewIssue = AffectedIssue;

/// Issue tree preview run (alias for PreviewActiveRun)
pub type IssueTreePreviewRun = PreviewActiveRun;

/// Issue tree preview warning
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreePreviewWarning {
    pub warning_type: String,
    pub message: String,
    pub issue_id: Option<Uuid>,
}

/// Hold release policy strategy (alias for backwards compatibility)
pub type HoldReleasePolicyStrategy = IssueTreeHoldReleasePolicyStrategy;

/// Active issue tree pause hold gate (for gate checking)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveIssueTreePauseHoldGate {
    pub hold_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub release_policy: IssueTreeHoldReleasePolicy,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
