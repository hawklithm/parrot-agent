use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// Issue Status Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Blocked,
    Done,
    Cancelled,
}

impl IssueStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, IssueStatus::Done | IssueStatus::Cancelled)
    }
}

// Issue Priority Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_priority", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum IssuePriority {
    Critical,
    High,
    Medium,
    Low,
}

// Issue Work Mode Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_work_mode", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum IssueWorkMode {
    Standard,
    Ask,
    Planning,
    SkillTest,
}

// Issue Origin Kind
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueOriginKind {
    UserRequest,
    AgentRequest,
    GoalDecomposition,
    WatchdogTrigger,
    MonitorTrigger,
    #[serde(untagged)]
    Plugin(String), // plugin:xxx format
}

// Issue Monitor Scheduled By
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "issue_monitor_scheduled_by", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum IssueMonitorScheduledBy {
    Assignee,
    Board,
}

// Issue Execution Policy (JSONB mapped)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueExecutionPolicy {
    #[serde(flatten)]
    pub config: JsonValue,
}

// Issue Execution State (JSONB mapped)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueExecutionState {
    #[serde(flatten)]
    pub state: JsonValue,
}

// Issue Assignee Adapter Overrides (JSONB mapped)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueAssigneeAdapterOverrides {
    #[serde(flatten)]
    pub overrides: JsonValue,
}

// Issue Execution Workspace Settings (JSONB mapped)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueExecutionWorkspaceSettings {
    #[serde(flatten)]
    pub settings: JsonValue,
}

// Issue Model
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    pub execution_locked_at: Option<DateTime<Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub responsible_user_id: Option<Uuid>,
    pub issue_number: Option<i32>,
    pub identifier: Option<String>,
    pub origin_kind: Option<String>, // Stored as text, deserialized to IssueOriginKind
    pub origin_id: Option<String>,
    pub origin_run_id: Option<Uuid>,
    pub origin_fingerprint: Option<String>,
    pub request_depth: i32,
    pub billing_code: Option<String>,
    pub assignee_adapter_overrides: Option<JsonValue>, // JSONB
    pub execution_policy: Option<JsonValue>,           // JSONB
    pub execution_state: Option<JsonValue>,            // JSONB
    pub monitor_next_check_at: Option<DateTime<Utc>>,
    pub monitor_last_triggered_at: Option<DateTime<Utc>>,
    pub monitor_attempt_count: Option<i32>,
    pub monitor_notes: Option<String>,
    pub monitor_scheduled_by: Option<IssueMonitorScheduledBy>,
    pub execution_workspace_id: Option<Uuid>,
    pub execution_workspace_preference: Option<String>,
    pub execution_workspace_settings: Option<JsonValue>, // JSONB
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub source_trust: Option<JsonValue>, // JSONB
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Issue {
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn is_checked_out(&self) -> bool {
        self.checkout_run_id.is_some()
    }

    pub fn is_blocked(&self) -> bool {
        self.status == IssueStatus::Blocked
    }

    pub fn is_execution_locked(&self) -> bool {
        self.execution_locked_at.is_some()
    }
}

// Create Issue Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIssueInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: IssueStatus,
    pub work_mode: Option<IssueWorkMode>,
    pub priority: Option<IssuePriority>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub responsible_user_id: Option<Uuid>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
    pub origin_run_id: Option<Uuid>,
    pub request_depth: Option<i32>,
    pub billing_code: Option<String>,
    pub assignee_adapter_overrides: Option<JsonValue>,
    pub execution_workspace_id: Option<Uuid>,
    pub execution_workspace_preference: Option<String>,
}

// Update Issue Input
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateIssueInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub work_mode: Option<IssueWorkMode>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub responsible_user_id: Option<Uuid>,
    pub execution_policy: Option<JsonValue>,
    pub execution_state: Option<JsonValue>,
    pub monitor_notes: Option<String>,
    pub monitor_scheduled_by: Option<IssueMonitorScheduledBy>,
    pub execution_workspace_preference: Option<String>,
    pub execution_workspace_settings: Option<JsonValue>,
    pub hidden_at: Option<DateTime<Utc>>,
    pub source_trust: Option<JsonValue>,
}

// Checkout Issue Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutIssueInput {
    pub agent_id: Uuid,
    pub expected_statuses: Vec<IssueStatus>,
    pub checkout_run_id: Option<Uuid>,
}

// Release Issue Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseIssueInput {
    pub release_run_id: Option<Uuid>,
    pub result: Option<String>,
    pub target_status: Option<IssueStatus>,
}

// Force Release Input (Admin operation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceReleaseIssueInput {
    pub reason: String,
}

// Issue Query Filter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

// Pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub cursor: Option<String>,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
            cursor: None,
        }
    }
}

// Issue State Machine
pub struct IssueStateMachine {
    transitions: Vec<IssueStateTransition>,
}

#[derive(Debug, Clone)]
pub struct IssueStateTransition {
    pub from: IssueStatus,
    pub to: IssueStatus,
    pub trigger: IssueTransitionTrigger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueTransitionTrigger {
    UserAction,
    AgentAction,
    Checkout,
    Release,
    ApprovalApproved,
    TreeControl,
    Monitor,
}

impl IssueStateMachine {
    pub fn new() -> Self {
        let transitions = vec![
            // Backlog transitions
            IssueStateTransition {
                from: IssueStatus::Backlog,
                to: IssueStatus::Todo,
                trigger: IssueTransitionTrigger::UserAction,
            },
            IssueStateTransition {
                from: IssueStatus::Backlog,
                to: IssueStatus::InProgress,
                trigger: IssueTransitionTrigger::Checkout,
            },
            // Todo transitions
            IssueStateTransition {
                from: IssueStatus::Todo,
                to: IssueStatus::InProgress,
                trigger: IssueTransitionTrigger::Checkout,
            },
            IssueStateTransition {
                from: IssueStatus::Todo,
                to: IssueStatus::Backlog,
                trigger: IssueTransitionTrigger::UserAction,
            },
            // InProgress transitions
            IssueStateTransition {
                from: IssueStatus::InProgress,
                to: IssueStatus::InReview,
                trigger: IssueTransitionTrigger::Release,
            },
            IssueStateTransition {
                from: IssueStatus::InProgress,
                to: IssueStatus::Done,
                trigger: IssueTransitionTrigger::Release,
            },
            IssueStateTransition {
                from: IssueStatus::InProgress,
                to: IssueStatus::Blocked,
                trigger: IssueTransitionTrigger::AgentAction,
            },
            // InReview transitions
            IssueStateTransition {
                from: IssueStatus::InReview,
                to: IssueStatus::Done,
                trigger: IssueTransitionTrigger::UserAction,
            },
            IssueStateTransition {
                from: IssueStatus::InReview,
                to: IssueStatus::InProgress,
                trigger: IssueTransitionTrigger::UserAction,
            },
            // Blocked transitions
            IssueStateTransition {
                from: IssueStatus::Blocked,
                to: IssueStatus::InProgress,
                trigger: IssueTransitionTrigger::ApprovalApproved,
            },
            IssueStateTransition {
                from: IssueStatus::Blocked,
                to: IssueStatus::Todo,
                trigger: IssueTransitionTrigger::UserAction,
            },
            // Any status can be cancelled
            IssueStateTransition {
                from: IssueStatus::Backlog,
                to: IssueStatus::Cancelled,
                trigger: IssueTransitionTrigger::TreeControl,
            },
            IssueStateTransition {
                from: IssueStatus::Todo,
                to: IssueStatus::Cancelled,
                trigger: IssueTransitionTrigger::TreeControl,
            },
            IssueStateTransition {
                from: IssueStatus::InProgress,
                to: IssueStatus::Cancelled,
                trigger: IssueTransitionTrigger::TreeControl,
            },
            IssueStateTransition {
                from: IssueStatus::InReview,
                to: IssueStatus::Cancelled,
                trigger: IssueTransitionTrigger::TreeControl,
            },
            IssueStateTransition {
                from: IssueStatus::Blocked,
                to: IssueStatus::Cancelled,
                trigger: IssueTransitionTrigger::TreeControl,
            },
        ];
        Self { transitions }
    }

    pub fn validate_transition(&self, from: IssueStatus, to: IssueStatus) -> bool {
        if from == to {
            return true;
        }
        self.transitions.iter().any(|t| t.from == from && t.to == to)
    }

    pub fn can_transition_with_trigger(
        &self,
        from: IssueStatus,
        to: IssueStatus,
        trigger: IssueTransitionTrigger,
    ) -> bool {
        if from == to {
            return true;
        }
        self.transitions
            .iter()
            .any(|t| t.from == from && t.to == to && t.trigger == trigger)
    }
}

impl Default for IssueStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
