use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Issue status enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
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

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueStatus::Backlog => write!(f, "backlog"),
            IssueStatus::Todo => write!(f, "todo"),
            IssueStatus::InProgress => write!(f, "in_progress"),
            IssueStatus::InReview => write!(f, "in_review"),
            IssueStatus::Blocked => write!(f, "blocked"),
            IssueStatus::Done => write!(f, "done"),
            IssueStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Issue state machine for status transitions
#[derive(Debug, Clone)]
pub struct IssueStateMachine {
    current_status: IssueStatus,
}

/// Transition error for issue state machine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueTransitionError {
    pub from: IssueStatus,
    pub to: IssueStatus,
    pub reason: String,
}

impl fmt::Display for IssueTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid issue transition from {:?} to {:?}: {}",
            self.from, self.to, self.reason
        )
    }
}

impl std::error::Error for IssueTransitionError {}

impl IssueStateMachine {
    /// Create a new state machine with initial status
    pub fn new(initial_status: IssueStatus) -> Self {
        Self {
            current_status: initial_status,
        }
    }

    /// Get current status
    pub fn current(&self) -> IssueStatus {
        self.current_status
    }

    /// Check if transition is valid
    pub fn can_transition_to(&self, target: IssueStatus) -> bool {
        let cur = self.current_status;
        match (cur, target) {
            // From Backlog
            (IssueStatus::Backlog, IssueStatus::Todo) => true,
            (IssueStatus::Backlog, IssueStatus::Cancelled) => true,

            // From Todo
            (IssueStatus::Todo, IssueStatus::InProgress) => true,
            (IssueStatus::Todo, IssueStatus::Blocked) => true,
            (IssueStatus::Todo, IssueStatus::Cancelled) => true,

            // From InProgress
            (IssueStatus::InProgress, IssueStatus::InReview) => true,
            (IssueStatus::InProgress, IssueStatus::Blocked) => true,
            (IssueStatus::InProgress, IssueStatus::Done) => true,
            (IssueStatus::InProgress, IssueStatus::Cancelled) => true,
            (IssueStatus::InProgress, IssueStatus::Todo) => true, // un-start

            // From InReview
            (IssueStatus::InReview, IssueStatus::Done) => true,
            (IssueStatus::InReview, IssueStatus::InProgress) => true, // re-open
            (IssueStatus::InReview, IssueStatus::Blocked) => true,
            (IssueStatus::InReview, IssueStatus::Cancelled) => true,

            // From Blocked
            (IssueStatus::Blocked, IssueStatus::InProgress) => true,
            (IssueStatus::Blocked, IssueStatus::InReview) => true,
            (IssueStatus::Blocked, IssueStatus::Cancelled) => true,

            // From Done
            (IssueStatus::Done, IssueStatus::InProgress) => true, // re-open
            (IssueStatus::Done, IssueStatus::Todo) => true, // re-open as todo

            // From Cancelled - no forward transitions
            (IssueStatus::Cancelled, IssueStatus::Todo) => true, // re-open

            // Same state is always allowed
            (a, b) if a == b => true,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Attempt to transition to a new status
    pub fn transition_to(&mut self, target: IssueStatus) -> Result<IssueStatus, IssueTransitionError> {
        if !self.can_transition_to(target) {
            return Err(IssueTransitionError {
                from: self.current_status,
                to: target,
                reason: format!(
                    "Transition from {:?} to {:?} is not allowed",
                    self.current_status, target
                ),
            });
        }

        let old_status = self.current_status;
        self.current_status = target;
        Ok(old_status)
    }

    /// Set status directly (for initialization, skips validation)
    pub fn set_status(&mut self, status: IssueStatus) {
        self.current_status = status;
    }

    /// Get all valid next states from current status
    pub fn valid_next_states(&self) -> Vec<IssueStatus> {
        let cur = self.current_status;
        match cur {
            IssueStatus::Backlog => vec![IssueStatus::Todo, IssueStatus::Cancelled],
            IssueStatus::Todo => vec![
                IssueStatus::InProgress,
                IssueStatus::Blocked,
                IssueStatus::Cancelled,
            ],
            IssueStatus::InProgress => vec![
                IssueStatus::InReview,
                IssueStatus::Blocked,
                IssueStatus::Done,
                IssueStatus::Cancelled,
                IssueStatus::Todo,
            ],
            IssueStatus::InReview => vec![
                IssueStatus::Done,
                IssueStatus::InProgress,
                IssueStatus::Blocked,
                IssueStatus::Cancelled,
            ],
            IssueStatus::Blocked => vec![
                IssueStatus::InProgress,
                IssueStatus::InReview,
                IssueStatus::Cancelled,
            ],
            IssueStatus::Done => vec![IssueStatus::InProgress, IssueStatus::Todo],
            IssueStatus::Cancelled => vec![IssueStatus::Todo],
        }
    }

    /// Check if issue is in a terminal state
    pub fn is_terminal(&self) -> bool {
        let cur = self.current_status;
        matches!(
            cur,
            IssueStatus::Done | IssueStatus::Cancelled
        )
    }

    /// Check if issue can be worked on
    pub fn can_work(&self) -> bool {
        let cur = self.current_status;
        matches!(
            cur,
            IssueStatus::Todo | IssueStatus::InProgress | IssueStatus::InReview
        )
    }

    /// Check if the issue is active (not terminal)
    pub fn is_active(&self) -> bool {
        !self.is_terminal()
    }

    /// Inherit status: when parent is cancelled, children should also be cancelled
    pub fn inherited_status(parent_status: IssueStatus) -> Option<IssueStatus> {
        match parent_status {
            IssueStatus::Cancelled => Some(IssueStatus::Cancelled),
            IssueStatus::Done => Some(IssueStatus::Done),
            _ => None,
        }
    }
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
    pub assigned_to: Option<Uuid>,
    pub checkout_run_id: Option<Uuid>,
    pub execution_run_id: Option<Uuid>,
    pub execution_agent_name_key: Option<String>,
    pub execution_locked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub responsible_user_id: Option<Uuid>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_state_machine_todo_to_in_progress() {
        let mut sm = IssueStateMachine::new(IssueStatus::Todo);
        assert!(sm.can_transition_to(IssueStatus::InProgress));
        assert!(sm.transition_to(IssueStatus::InProgress).is_ok());
        assert_eq!(sm.current(), IssueStatus::InProgress);
    }

    #[test]
    fn test_issue_state_machine_backlog_to_todo() {
        let mut sm = IssueStateMachine::new(IssueStatus::Backlog);
        assert!(sm.can_transition_to(IssueStatus::Todo));
        assert!(sm.transition_to(IssueStatus::Todo).is_ok());
        assert_eq!(sm.current(), IssueStatus::Todo);
    }

    #[test]
    fn test_issue_state_machine_full_flow() {
        let mut sm = IssueStateMachine::new(IssueStatus::Todo);
        assert!(sm.transition_to(IssueStatus::InProgress).is_ok());
        assert!(sm.transition_to(IssueStatus::InReview).is_ok());
        assert!(sm.transition_to(IssueStatus::Done).is_ok());
        assert_eq!(sm.current(), IssueStatus::Done);
    }

    #[test]
    fn test_issue_state_machine_done_to_in_progress_reopen() {
        let mut sm = IssueStateMachine::new(IssueStatus::Done);
        assert!(sm.can_transition_to(IssueStatus::InProgress));
        assert!(sm.transition_to(IssueStatus::InProgress).is_ok());
        assert_eq!(sm.current(), IssueStatus::InProgress);
    }

    #[test]
    fn test_issue_state_machine_invalid_transition() {
        let mut sm = IssueStateMachine::new(IssueStatus::Backlog);
        assert!(!sm.can_transition_to(IssueStatus::Done));
        assert!(sm.transition_to(IssueStatus::Done).is_err());

        let mut sm2 = IssueStateMachine::new(IssueStatus::Cancelled);
        assert!(!sm2.can_transition_to(IssueStatus::Done));
        assert!(sm2.transition_to(IssueStatus::Done).is_err());
    }

    #[test]
    fn test_issue_state_machine_valid_next_states() {
        let sm = IssueStateMachine::new(IssueStatus::InProgress);
        let next = sm.valid_next_states();
        assert!(next.contains(&IssueStatus::InReview));
        assert!(next.contains(&IssueStatus::Blocked));
        assert!(next.contains(&IssueStatus::Done));
        assert!(next.contains(&IssueStatus::Cancelled));
        assert!(next.contains(&IssueStatus::Todo));
    }

    #[test]
    fn test_issue_state_machine_terminal_and_can_work() {
        let done_sm = IssueStateMachine::new(IssueStatus::Done);
        assert!(done_sm.is_terminal());
        assert!(!done_sm.can_work());

        let todo_sm = IssueStateMachine::new(IssueStatus::Todo);
        assert!(!todo_sm.is_terminal());
        assert!(todo_sm.can_work());
    }

    #[test]
    fn test_issue_state_machine_inherited_status() {
        assert_eq!(
            IssueStateMachine::inherited_status(IssueStatus::Cancelled),
            Some(IssueStatus::Cancelled)
        );
        assert_eq!(
            IssueStateMachine::inherited_status(IssueStatus::Done),
            Some(IssueStatus::Done)
        );
        assert_eq!(
            IssueStateMachine::inherited_status(IssueStatus::InProgress),
            None
        );
    }

    #[test]
    fn test_issue_state_machine_cancelled_reopen() {
        let mut sm = IssueStateMachine::new(IssueStatus::Cancelled);
        assert!(sm.can_transition_to(IssueStatus::Todo));
        assert!(sm.transition_to(IssueStatus::Todo).is_ok());
        assert_eq!(sm.current(), IssueStatus::Todo);
    }

    #[test]
    fn test_issue_state_machine_same_state() {
        let mut sm = IssueStateMachine::new(IssueStatus::InProgress);
        assert!(sm.can_transition_to(IssueStatus::InProgress));
        assert!(sm.transition_to(IssueStatus::InProgress).is_ok());
        assert_eq!(sm.current(), IssueStatus::InProgress);
    }
}
