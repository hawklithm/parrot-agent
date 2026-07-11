use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Issue tree control mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeHold {
    pub id: Uuid,
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: IssueTreeHoldReleasePolicy,
    pub metadata: Option<serde_json::Value>,
    pub actor_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Issue tree hold member
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeHoldMember {
    pub id: Uuid,
    pub hold_id: Uuid,
    pub issue_id: Uuid,
    pub previous_status: String,
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
