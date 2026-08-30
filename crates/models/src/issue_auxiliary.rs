use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Work product
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkProduct {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
    pub runtime_service_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
    #[serde(rename = "type")]
    pub work_product_type: String,
    pub provider: String,
    pub external_id: Option<String>,
    pub title: String,
    pub url: Option<String>,
    pub status: String,
    pub review_state: String,
    pub is_primary: bool,
    pub health_status: String,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_trust: Option<serde_json::Value>,
    pub created_by_run_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create work product input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkProductInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
    pub project_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
    pub runtime_service_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub work_product_type: Option<String>,
    pub provider: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub status: Option<String>,
    pub review_state: Option<String>,
    pub is_primary: Option<bool>,
    pub health_status: Option<String>,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_trust: Option<serde_json::Value>,
    pub created_by_run_id: Option<Uuid>,
}

/// Update work product input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkProductInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
    pub project_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
    pub runtime_service_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub work_product_type: Option<String>,
    pub provider: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub status: Option<String>,
    pub review_state: Option<String>,
    pub is_primary: Option<bool>,
    pub health_status: Option<String>,
    pub summary: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub source_trust: Option<serde_json::Value>,
    pub created_by_run_id: Option<Uuid>,
}

/// Attachment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: Uuid,
    pub parent_type: String, // "issue" | "case"
    pub parent_id: Uuid,
    pub company_id: Uuid,
    pub asset_id: Option<Uuid>,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Upload attachment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAttachmentInput {
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub content: Vec<u8>,
}

// ─── Issue Read Status ──────────────────────────────────────────

/// Issue read status (tracks which users have read which issues)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueReadStatus {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub user_id: Uuid,
    pub read_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Mark issue as read input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkIssueReadInput {
    pub user_id: Uuid,
}

// ─── Issue Inbox Archive ───────────────────────────────────────

/// Issue inbox archive (tracks which users have archived which issues)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueInboxArchive {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub user_id: Uuid,
    pub archived_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Archive issue input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveIssueInput {
    pub user_id: Uuid,
}

// ─── Feedback Vote ─────────────────────────────────────────────

/// Feedback vote direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoteDirection {
    Up,
    Down,
}

impl std::fmt::Display for VoteDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoteDirection::Up => write!(f, "up"),
            VoteDirection::Down => write!(f, "down"),
        }
    }
}

/// Feedback vote
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackVote {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub voter_id: Uuid,
    pub voter_type: String,
    pub vote: String,
    pub reason: Option<String>,
    pub shared_with_labs: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create feedback vote input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFeedbackVoteInput {
    pub voter_id: Uuid,
    pub voter_type: Option<String>,
    pub vote: VoteDirection,
    pub reason: Option<String>,
    pub shared_with_labs: Option<bool>,
}

/// Feedback trace
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackTrace {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub vote_id: Uuid,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub status: String,
    pub failure_reason: Option<String>,
    pub shared_with_labs: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Feedback trace bundle (trace + related data)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackTraceBundle {
    pub trace: FeedbackTrace,
    pub vote: Option<FeedbackVote>,
    pub issue_title: Option<String>,
    pub issue_identifier: Option<String>,
}

// ─── Recovery Action ───────────────────────────────────────────

/// Recovery action status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryActionStatus {
    Active,
    Escalated,
    Resolved,
    Cancelled,
}

impl std::fmt::Display for RecoveryActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryActionStatus::Active => write!(f, "active"),
            RecoveryActionStatus::Escalated => write!(f, "escalated"),
            RecoveryActionStatus::Resolved => write!(f, "resolved"),
            RecoveryActionStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Recovery action
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryAction {
    pub id: Uuid,
    pub company_id: Uuid,
    pub source_issue_id: Uuid,
    pub recovery_issue_id: Option<Uuid>,
    pub kind: String,
    pub status: String,
    pub owner_type: String,
    pub owner_agent_id: Option<Uuid>,
    pub owner_user_id: Option<String>,
    pub previous_owner_agent_id: Option<Uuid>,
    pub return_owner_agent_id: Option<Uuid>,
    pub cause: String,
    pub fingerprint: String,
    pub evidence: serde_json::Value,
    pub next_action: String,
    pub wake_policy: Option<serde_json::Value>,
    pub monitor_policy: Option<serde_json::Value>,
    pub attempt_count: i32,
    pub max_attempts: Option<i32>,
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    pub outcome: Option<String>,
    pub resolution_note: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create recovery action input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecoveryActionInput {
    pub recovery_issue_id: Option<Uuid>,
    pub kind: String,
    pub owner_type: Option<String>,
    pub owner_agent_id: Option<Uuid>,
    pub owner_user_id: Option<String>,
    pub previous_owner_agent_id: Option<Uuid>,
    pub return_owner_agent_id: Option<Uuid>,
    pub cause: String,
    pub fingerprint: String,
    pub evidence: Option<serde_json::Value>,
    pub next_action: String,
    pub wake_policy: Option<serde_json::Value>,
    pub monitor_policy: Option<serde_json::Value>,
    pub max_attempts: Option<i32>,
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Resolve recovery action input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRecoveryActionInput {
    pub status: String,
    pub outcome: String,
    pub resolution_note: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ─── Plan Decomposition ────────────────────────────────────────

/// Plan decomposition (accepted plan that decomposes an issue into sub-issues)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PlanDecomposition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub plan: serde_json::Value,
    pub accepted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub accepted_by_type: Option<String>,
    pub accepted_by_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create plan decomposition input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanDecompositionInput {
    pub plan: serde_json::Value,
}

/// Accept plan decomposition input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptPlanDecompositionInput {
    pub accepted_by_type: String,
    pub accepted_by_id: Uuid,
}
