//! Persistent, resumable task-driven team orchestration.
//!
//! Each root issue routed to a CEO owns one `task_orchestrations` row (a
//! versioned, company-scoped state machine) plus its team plan revisions,
//! subtasks, deliverables, acceptance decisions, durable side-effect outbox
//! and append-only audit events.
//!
//! Design boundaries:
//!
//! * This service is the **single writer** for orchestration state. Every
//!   mutation re-reads the row under a transaction-scoped advisory lock derived
//!   from the root issue, validates the event against the current stage, applies
//!   it and appends an audit event in one transaction.
//! * Model output (the CEO decision contract) is untrusted input. Structural
//!   validation lives in [`validate_decision_contract`]; company scope, agent
//!   executability, reporting cycles and approvals are enforced here against
//!   the database.
//! * Side effects that must survive a crash (waking an assignee or the CEO,
//!   cancelling child issues) are written to `task_orchestration_actions` with
//!   an idempotency key and applied by a claim/replay loop.
//! * Completing an orchestrated task is driven by persisted acceptance and a
//!   persisted CEO report — never by a run's exit status.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::agent_service::{AgentService, CreateAgentInput, ServiceError};
use crate::approval_service::{ApprovalService, CreateApprovalInput};
use crate::heartbeat_service::{
    HeartbeatError, HeartbeatService, HeartbeatWakeupOptions, TaskBudgetResumeReference,
};
use crate::issue_service::IssueService;
use models::{AgentStatus, ApprovalType, CreateIssueInput, Issue, IssueStatus};

// ─────────────────────────────── stages ───────────────────────────────

/// Workflow stages for one root-task orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStage {
    Assessing,
    TeamPlanning,
    AwaitingTeamApproval,
    Clarifying,
    Planning,
    Executing,
    Reviewing,
    Reporting,
    Completed,
    Failed,
    Cancelled,
}

impl OrchestrationStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assessing => "assessing",
            Self::TeamPlanning => "team_planning",
            Self::AwaitingTeamApproval => "awaiting_team_approval",
            Self::Clarifying => "clarifying",
            Self::Planning => "planning",
            Self::Executing => "executing",
            Self::Reviewing => "reviewing",
            Self::Reporting => "reporting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Stages that still accept new clarification questions.
    pub const fn can_receive_clarification(self) -> bool {
        matches!(
            self,
            Self::Assessing
                | Self::TeamPlanning
                | Self::AwaitingTeamApproval
                | Self::Clarifying
                | Self::Planning
                | Self::Executing
                | Self::Reviewing
        )
    }

    /// Stages a failure or cancellation can interrupt.
    pub const fn can_be_interrupted(self) -> bool {
        matches!(
            self,
            Self::Assessing
                | Self::TeamPlanning
                | Self::AwaitingTeamApproval
                | Self::Clarifying
                | Self::Planning
                | Self::Executing
                | Self::Reviewing
                | Self::Reporting
        )
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl std::str::FromStr for OrchestrationStage {
    type Err = OrchestrationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "assessing" => Self::Assessing,
            "team_planning" => Self::TeamPlanning,
            "awaiting_team_approval" => Self::AwaitingTeamApproval,
            "clarifying" => Self::Clarifying,
            "planning" => Self::Planning,
            "executing" => Self::Executing,
            "reviewing" => Self::Reviewing,
            "reporting" => Self::Reporting,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            other => {
                return Err(OrchestrationError::Storage(format!(
                    "unknown persisted orchestration stage {other}"
                )))
            }
        })
    }
}

// ─────────────────────────────── events ───────────────────────────────

/// Events that drive the orchestration forward. Each is validated against the
/// current stage; an illegal event is rejected without mutating state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationEvent {
    /// The CEO finished reading context and assessing the request.
    AssessmentCompleted,
    /// A team plan revision was persisted. `approval_required` is true when the
    /// revision hires agents in a company that requires board approval.
    TeamPlanPrepared { approval_required: bool },
    /// The board approved the pending plan.
    TeamPlanApproved,
    /// The board rejected the pending plan; the CEO replans.
    TeamPlanRejected,
    /// A clarification question became pending.
    ClarificationRequested { interaction_id: Uuid },
    /// The user answered a clarification question.
    UserClarificationAnswered { interaction_id: Uuid },
    /// A required subtask issue was created and recorded.
    RequiredSubtaskPlanned { issue_id: Uuid },
    /// All planning is done; execution may start.
    PlanningCompleted,
    /// An executor submitted a deliverable.
    ExecutionSubmitted { issue_id: Uuid },
    /// A deliverable was accepted. `issue_id` is the subtask issue, or the root
    /// issue for the final summary.
    AcceptancePassed { issue_id: Uuid },
    /// A deliverable was rejected with feedback.
    AcceptanceRejected { issue_id: Uuid },
    /// The CEO persisted the final user-facing report.
    FinalReportRecorded,
    /// An unrecoverable failure.
    FailureRecorded { reason: String },
    /// Retry after a recorded failure, returning to the recovery stage.
    RetryAfterFailure,
    /// The orchestration was cancelled.
    CancellationRequested,
    /// Resume a cancelled orchestration at its recovery stage.
    ResumeAfterCancellation,
}

impl OrchestrationEvent {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::AssessmentCompleted => "assessment_completed",
            Self::TeamPlanPrepared { .. } => "team_plan_prepared",
            Self::TeamPlanApproved => "team_plan_approved",
            Self::TeamPlanRejected => "team_plan_rejected",
            Self::ClarificationRequested { .. } => "clarification_requested",
            Self::UserClarificationAnswered { .. } => "user_clarification_answered",
            Self::RequiredSubtaskPlanned { .. } => "required_subtask_planned",
            Self::PlanningCompleted => "planning_completed",
            Self::ExecutionSubmitted { .. } => "execution_submitted",
            Self::AcceptancePassed { .. } => "acceptance_passed",
            Self::AcceptanceRejected { .. } => "acceptance_rejected",
            Self::FinalReportRecorded => "final_report_recorded",
            Self::FailureRecorded { .. } => "failure_recorded",
            Self::RetryAfterFailure => "retry_after_failure",
            Self::CancellationRequested => "cancellation_requested",
            Self::ResumeAfterCancellation => "resume_after_cancellation",
        }
    }
}

// ─────────────────────────────── errors ───────────────────────────────

/// Why an orchestration operation was rejected. State is unchanged on every
/// variant unless the doc says otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationError {
    NotOrchestrated {
        issue_id: Uuid,
    },
    InvalidTransition {
        stage: OrchestrationStage,
        event: String,
    },
    UnansweredQuestionsRemain {
        count: usize,
    },
    RequiredSubtasksNotAccepted {
        count: usize,
    },
    DependenciesNotSatisfied {
        issue_ids: Vec<Uuid>,
    },
    InteractionNotFound(Uuid),
    SubtaskNotFound(Uuid),
    SubtaskAlreadyPlanned(Uuid),
    PlanNotFound(Uuid),
    AgentNotInCompany(Uuid),
    AgentNotExecutable(Uuid),
    ReportingCycleDetected,
    DependencyCycleDetected,
    CrossCompanyAccess,
    SelfReviewBlocked,
    NotDirectReviewer,
    NotSubtaskOwner,
    ContractInvalid {
        field: String,
        message: String,
    },
    Conflict(String),
    TerminalState,
    Storage(String),
    Service(String),
}

impl std::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOrchestrated { issue_id } => {
                write!(f, "issue {issue_id} is not under team orchestration")
            }
            Self::InvalidTransition { stage, event } => {
                write!(f, "event {event} is not legal in stage {}", stage.as_str())
            }
            Self::UnansweredQuestionsRemain { count } => {
                write!(f, "{count} clarification question(s) are still unanswered")
            }
            Self::RequiredSubtasksNotAccepted { count } => {
                write!(f, "{count} required subtask(s) are not accepted yet")
            }
            Self::DependenciesNotSatisfied { issue_ids } => {
                write!(
                    f,
                    "blocking issue(s) are not done: {}",
                    issue_ids
                        .iter()
                        .map(Uuid::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Self::InteractionNotFound(id) => {
                write!(f, "interaction {id} does not belong to this task")
            }
            Self::SubtaskNotFound(id) => write!(f, "subtask {id} is not part of this task"),
            Self::SubtaskAlreadyPlanned(id) => write!(f, "subtask {id} is already planned"),
            Self::PlanNotFound(id) => write!(f, "team plan {id} was not found"),
            Self::AgentNotInCompany(id) => write!(f, "agent {id} does not belong to this company"),
            Self::AgentNotExecutable(id) => {
                write!(f, "agent {id} is not an executable member of this company")
            }
            Self::ReportingCycleDetected => write!(f, "task reporting graph contains a cycle"),
            Self::DependencyCycleDetected => write!(f, "subtask dependency graph contains a cycle"),
            Self::CrossCompanyAccess => write!(f, "resource is outside the company scope"),
            Self::SelfReviewBlocked => write!(f, "an agent cannot review its own deliverable"),
            Self::NotDirectReviewer => {
                write!(
                    f,
                    "only the task's direct superior may review this deliverable"
                )
            }
            Self::NotSubtaskOwner => {
                write!(f, "only the subtask owner may submit this deliverable")
            }
            Self::ContractInvalid { field, message } => {
                write!(f, "invalid decision contract at {field}: {message}")
            }
            Self::Conflict(message) => write!(f, "{message}"),
            Self::TerminalState => write!(f, "orchestration is already terminal"),
            Self::Storage(error) => write!(f, "orchestration storage error: {error}"),
            Self::Service(error) => write!(f, "orchestration service error: {error}"),
        }
    }
}

impl std::error::Error for OrchestrationError {}

impl From<sqlx::Error> for OrchestrationError {
    fn from(error: sqlx::Error) -> Self {
        OrchestrationError::Storage(error.to_string())
    }
}

// ─────────────────────────────── read models ───────────────────────────────

/// A pending clarification question surfaced by the CEO.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingClarification {
    pub interaction_id: Uuid,
    pub prompt: String,
    pub asked_at: DateTime<Utc>,
}

/// A required subtask of the orchestration, with its live execution state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationSubtask {
    pub issue_id: Uuid,
    pub parent_issue_id: Option<Uuid>,
    pub owner_agent_id: Option<Uuid>,
    pub reviewer_agent_id: Option<Uuid>,
    pub acceptance_criteria: Option<String>,
    pub status: String,
    pub required: bool,
}

/// One member decision inside a team plan revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanMember {
    pub agent_id: Option<Uuid>,
    pub action: String,
    pub role: Option<String>,
    pub responsibilities: Option<String>,
    pub reason: Option<String>,
    pub reports_to_agent_id: Option<Uuid>,
    pub approval_id: Option<Uuid>,
    pub hire_status: Option<String>,
    pub hired_agent_id: Option<Uuid>,
    pub exited_at: Option<DateTime<Utc>>,
    pub exit_reason: Option<String>,
}

/// A team plan revision.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamPlan {
    pub plan_id: Uuid,
    pub revision: i32,
    pub status: String,
    pub members: Vec<PlanMember>,
    pub proposed_at: DateTime<Utc>,
}

/// One deliverable version plus its acceptance verdict, if any.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliverableView {
    pub deliverable_id: Uuid,
    pub subtask_issue_id: Option<Uuid>,
    pub version: i32,
    pub submitted_by_agent_id: Option<Uuid>,
    pub body: Option<String>,
    pub evidence: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub verdict: Option<String>,
    pub feedback: Option<String>,
    pub reviewer_agent_id: Option<Uuid>,
}

/// Full orchestration read model for the task-detail UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationView {
    pub orchestration_id: Uuid,
    pub root_issue_id: Uuid,
    pub ceo_agent_id: Uuid,
    pub stage: OrchestrationStage,
    pub stage_version: i64,
    pub decision_contract_version: i32,
    pub plan: Option<TeamPlan>,
    pub pending_clarifications: Vec<PendingClarification>,
    pub subtasks: Vec<OrchestrationSubtask>,
    pub deliverables: Vec<DeliverableView>,
    pub final_report: Option<String>,
    pub error_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub task_budget_cents: Option<i64>,
    pub task_spent_cents: i64,
    pub task_remaining_cents: Option<i64>,
    pub task_budget_exhausted: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskBudgetRoot {
    pub root_issue_id: Uuid,
    pub budget_cents: Option<i64>,
    pub budget_revision: i64,
    pub title: String,
    pub orchestration_required: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct RequiredOrchestrationRecoveryCandidate {
    issue_id: Uuid,
    company_id: Uuid,
    ceo_agent_id: Uuid,
    orchestration_id: Option<Uuid>,
}

/// Resolve the task's root and configured lifetime cap without crossing the
/// tenant boundary. A missing root (including a malformed/cross-company parent
/// chain) returns no row, which callers enforcing a budget treat as a failure.
pub async fn task_budget_root(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
) -> Result<Option<TaskBudgetRoot>, sqlx::Error> {
    sqlx::query_as::<_, TaskBudgetRoot>(
        r#"
        WITH RECURSIVE ancestors(id, parent_id) AS (
            SELECT id, parent_id
              FROM issues
             WHERE company_id = $1 AND id = $2
            UNION
            SELECT parent.id, parent.parent_id
              FROM issues parent
              JOIN ancestors child ON child.parent_id = parent.id
             WHERE parent.company_id = $1
        )
        SELECT root.id AS root_issue_id,
               root.task_budget_cents AS budget_cents,
               root.task_budget_revision AS budget_revision,
               root.title,
               root.task_orchestration_required AS orchestration_required
          FROM ancestors
          JOIN issues root ON root.id = ancestors.id AND root.company_id = $1
         WHERE ancestors.parent_id IS NULL
         LIMIT 1
        "#,
    )
    .bind(company_id)
    .bind(issue_id)
    .fetch_optional(pool)
    .await
}

/// Sum all settled cost events on the root issue and its full descendant tree.
/// Hidden and harness descendants are included so their recorded spend cannot
/// be omitted from a task cap. Both the issue and cost event side are scoped to
/// the requested company.
pub async fn task_tree_spend_cents(
    pool: &PgPool,
    company_id: Uuid,
    root_issue_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let amount: Option<i64> = sqlx::query_scalar(
        r#"
        WITH RECURSIVE root(id) AS (
            SELECT id FROM issues
             WHERE company_id = $1 AND id = $2 AND parent_id IS NULL
        ), issue_tree(id) AS (
            SELECT id FROM root
            UNION
            SELECT child.id
              FROM issues child
              JOIN issue_tree parent ON child.parent_id = parent.id
             WHERE child.company_id = $1
        )
        SELECT COALESCE(SUM(event.cost_cents), 0)::bigint
          FROM root
          JOIN issue_tree tree ON true
          LEFT JOIN cost_events event
            ON event.company_id = $1 AND event.issue_id = tree.id
        "#,
    )
    .bind(company_id)
    .bind(root_issue_id)
    .fetch_optional(pool)
    .await?;
    amount.ok_or(sqlx::Error::RowNotFound)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBudgetUpdate {
    pub issue_id: Uuid,
    pub budget_cents: Option<i64>,
    pub spent_cents: i64,
    pub remaining_cents: Option<i64>,
    pub exhausted: bool,
    pub revision: i64,
    pub resumed_wake_count: usize,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskBudgetWakeResume {
    id: Uuid,
    company_id: Uuid,
    root_issue_id: Option<Uuid>,
    issue_id: Uuid,
    agent_id: Uuid,
    requested_by_actor_type: Option<String>,
    requested_by_actor_id: Option<Uuid>,
    resume_source: Option<String>,
    resume_trigger_detail: Option<String>,
    resume_reason: Option<String>,
    resume_idempotency_key: String,
    resume_kind: String,
    source_task_action_id: Option<Uuid>,
    resume_payload: Option<serde_json::Value>,
    resume_context_snapshot: Option<serde_json::Value>,
    retry_of_run_id: Option<Uuid>,
    scheduled_retry_attempt: Option<i32>,
    attempts: i32,
    claim_token: Uuid,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskBudgetWakeResumeState {
    issue_status: String,
    issue_cancelled_at: Option<DateTime<Utc>>,
    issue_hidden_at: Option<DateTime<Utc>>,
    issue_assignee_agent_id: Option<Uuid>,
    root_status: String,
    root_cancelled_at: Option<DateTime<Utc>>,
    root_hidden_at: Option<DateTime<Utc>>,
    budget_cents: Option<i64>,
    orchestration_stage: Option<String>,
}

/// Only a narrow, non-sensitive assignment context is safe to persist and
/// replay. Any unknown caller-provided text or JSON shape is recorded as an
/// unsupported intent instead of being reissued as a generic assignment.
pub(crate) fn task_budget_resume_options_supported(
    issue_id: Uuid,
    options: &HeartbeatWakeupOptions,
) -> Result<(), &'static str> {
    if let Some(reference) = options.task_budget_resume_reference {
        let reason = reference.kind();
        if options.source.as_deref() != Some("automation")
            || options.trigger_detail.as_deref() != Some("task_orchestration")
            || options.reason.as_deref() != Some(reason)
            || options.requested_by_actor_type.as_deref() != Some("system")
            || options.requested_by_actor_id.is_some()
        {
            return Err("task orchestration wake metadata is not in the typed replay allowlist");
        }
        if options.retry_of_run_id.is_some() || options.scheduled_retry_attempt.is_some() {
            return Err("task orchestration wake cannot carry scheduled retry metadata");
        }

        let Some(payload) = options.payload.as_ref().and_then(serde_json::Value::as_object) else {
            return Err("task orchestration wake payload is not an object");
        };
        if payload.len() != 3
            || payload.keys().any(|key| !matches!(key.as_str(), "issueId" | "agentId" | "reason"))
            || payload
                .get("issueId")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                != Some(issue_id)
            || payload
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .is_none()
            || payload.get("reason").and_then(serde_json::Value::as_str) != Some(reason)
        {
            return Err("task orchestration wake payload does not match its typed event");
        }

        let Some(context) = options
            .context_snapshot
            .as_ref()
            .and_then(serde_json::Value::as_object)
        else {
            return Err("task orchestration wake context is not an object");
        };
        if context.len() != 4
            || context
                .keys()
                .any(|key| !matches!(key.as_str(), "issueId" | "taskId" | "wakeReason" | "source"))
            || ["issueId", "taskId"].iter().any(|key| {
                context
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    != Some(issue_id)
            })
            || context.get("wakeReason").and_then(serde_json::Value::as_str) != Some(reason)
            || context.get("source").and_then(serde_json::Value::as_str)
                != Some("task_orchestration")
        {
            return Err("task orchestration wake context does not match its typed event");
        }

        let expected_key = options
            .task_budget_resume_intent_id
            .map(|intent_id| format!("task-budget-wake-resume:{intent_id}"))
            .unwrap_or_else(|| {
                format!("task-orchestration-action:{}", reference.action_id())
            });
        if options.idempotency_key.as_deref() != Some(expected_key.as_str()) {
            return Err("task orchestration wake idempotency key is not tied to its source action");
        }
        return Ok(());
    }

    if options.source.as_deref().is_some_and(|source| {
        !crate::heartbeat_service::HEARTBEAT_INVOCATION_SOURCES.contains(&source)
    }) {
        return Err("invocation source is not a supported enum value");
    }
    if options
        .trigger_detail
        .as_deref()
        .is_some_and(|detail| detail != "system")
    {
        return Err("trigger detail is not in the replay allowlist");
    }
    if options.reason.as_deref().is_some_and(|reason| {
        !matches!(
            reason,
            "issue_assigned" | "issue_checked_out" | "issue_blockers_resolved"
        )
    }) {
        return Err("wake reason is not in the replay allowlist");
    }
    if options
        .requested_by_actor_type
        .as_deref()
        .is_some_and(|actor_type| {
            !matches!(actor_type, "board" | "agent" | "user" | "system" | "none")
        })
    {
        return Err("request actor type is not in the replay allowlist");
    }

    if let Some(payload) = options.payload.as_ref() {
        let Some(object) = payload.as_object() else {
            return Err("wake payload is not an object");
        };
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "issueId" | "mutation"))
        {
            return Err("wake payload contains an unknown field");
        }
        if object.get("issueId").is_some_and(|value| {
            value.as_str().and_then(|value| Uuid::parse_str(value).ok()) != Some(issue_id)
        }) {
            return Err("wake payload issue id is not a matching UUID");
        }
        if object.get("mutation").is_some_and(|value| {
            !matches!(
                value.as_str(),
                Some(
                    "create"
                        | "create_replay"
                        | "create_recovery"
                        | "checkout"
                        | "blockers_resolved"
                )
            )
        }) {
            return Err("wake payload mutation is not in the replay allowlist");
        }
    }

    if let Some(context) = options.context_snapshot.as_ref() {
        let Some(object) = context.as_object() else {
            return Err("wake context is not an object");
        };
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "issueId" | "source"))
        {
            return Err("wake context contains an unknown field");
        }
        if object.get("issueId").is_some_and(|value| {
            value.as_str().and_then(|value| Uuid::parse_str(value).ok()) != Some(issue_id)
        }) {
            return Err("wake context issue id is not a matching UUID");
        }
        if object.get("source").is_some_and(|value| {
            !matches!(
                value.as_str(),
                Some(
                    "issue.create"
                        | "issue.create.recovery"
                        | "issue.create.replay"
                        | "issue.child_create"
                        | "issue.checkout"
                        | "issue.release"
                )
            )
        }) {
            return Err("wake context source is not in the replay allowlist");
        }
    }

    let is_recovery_payload = options
        .payload
        .as_ref()
        .and_then(|payload| payload.get("mutation"))
        .and_then(serde_json::Value::as_str)
        == Some("create_recovery");
    let is_recovery_context = options
        .context_snapshot
        .as_ref()
        .and_then(|context| context.get("source"))
        .and_then(serde_json::Value::as_str)
        == Some("issue.create.recovery");
    if is_recovery_payload != is_recovery_context {
        return Err("recovery mutation and context source must be preserved together");
    }
    if is_recovery_payload {
        let expected_key = options
            .task_budget_resume_intent_id
            .map(|intent_id| format!("task-budget-wake-resume:{intent_id}"))
            .unwrap_or_else(|| format!("task-orchestration-initial:{issue_id}"));
        let payload_has_issue = options
            .payload
            .as_ref()
            .and_then(|payload| payload.get("issueId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            == Some(issue_id);
        let context_has_issue = options
            .context_snapshot
            .as_ref()
            .and_then(|context| context.get("issueId"))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            == Some(issue_id);
        if options.source.as_deref() != Some("assignment")
            || options.trigger_detail.as_deref() != Some("system")
            || options.reason.as_deref() != Some("issue_assigned")
            || options.requested_by_actor_type.as_deref() != Some("system")
            || options.requested_by_actor_id.is_some()
            || options.idempotency_key.as_deref() != Some(expected_key.as_str())
            || !payload_has_issue
            || !context_has_issue
        {
            return Err("recovery wake is not the durable initial assignment shape");
        }
    }

    if options.retry_of_run_id.is_some() != options.scheduled_retry_attempt.is_some()
        || options
            .scheduled_retry_attempt
            .is_some_and(|attempt| attempt < 0)
    {
        return Err("scheduled retry metadata cannot be preserved safely");
    }

    if let Some(key) = options.idempotency_key.as_deref() {
        let safe_key = if let Some(intent_id) = options.task_budget_resume_intent_id {
            key.strip_prefix("task-budget-wake-resume:")
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(intent_id)
        } else {
            key.strip_prefix("task-orchestration-initial:")
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(issue_id)
                || key
                    .strip_prefix("issue_blockers_resolved:")
                    .and_then(|value| value.split_once(':'))
                    .is_some_and(|(dependent, blocker)| {
                        Uuid::parse_str(dependent).ok() == Some(issue_id)
                            && Uuid::parse_str(blocker).is_ok()
                    })
        };
        if !safe_key {
            return Err("idempotency key is not in the replay allowlist");
        }
    }

    Ok(())
}

/// Confirm that a typed resume reference still points to the exact durable
/// task action and live task state. This validates the authoritative rows,
/// rather than trusting the wake's copied JSON shape.
pub(crate) async fn task_budget_resume_reference_is_live<'e, E>(
    executor: E,
    company_id: Uuid,
    root_issue_id: Uuid,
    issue_id: Uuid,
    agent_id: Uuid,
    reference: TaskBudgetResumeReference,
    allow_running_action: bool,
) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let live = match reference {
        TaskBudgetResumeReference::SubtaskAssigned { action_id } => {
            sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                      FROM task_orchestration_actions action
                      JOIN task_orchestrations orchestration
                        ON orchestration.id = action.orchestration_id
                       AND orchestration.company_id = action.company_id
                      JOIN companies company ON company.id = action.company_id
                      JOIN issues root
                        ON root.id = orchestration.root_issue_id
                       AND root.company_id = orchestration.company_id
                      JOIN task_orchestration_subtasks subtask
                        ON subtask.orchestration_id = orchestration.id
                       AND subtask.company_id = orchestration.company_id
                      JOIN issues child
                        ON child.id = subtask.issue_id
                       AND child.company_id = subtask.company_id
                      JOIN agents agent
                        ON agent.id = $5 AND agent.company_id = action.company_id
                     WHERE action.id = $1
                       AND action.company_id = $2
                       AND action.action_type = 'wake_assignee'
                       AND (action.status = 'applied' OR ($6 AND action.status = 'running'))
                       AND action.idempotency_key = 'subtask-assigned:' || child.id::text
                       AND action.payload = jsonb_build_object(
                           'issueId', child.id, 'agentId', agent.id,
                           'reason', 'subtask_assigned')
                       AND orchestration.root_issue_id = $3
                       AND orchestration.stage IN ('executing', 'reviewing')
                       AND root.task_orchestration_required
                       AND root.parent_id IS NULL
                       AND root.status::text NOT IN ('done', 'cancelled')
                       AND root.cancelled_at IS NULL AND root.hidden_at IS NULL
                       AND child.id = $4
                       AND child.assignee_agent_id = agent.id
                       AND child.status::text NOT IN ('done', 'cancelled')
                       AND child.cancelled_at IS NULL AND child.hidden_at IS NULL
                       AND subtask.owner_agent_id = agent.id
                       AND subtask.status = 'pending'
                       AND company.status::text = 'active'
                       AND EXISTS (
                           SELECT 1
                             FROM task_orchestration_events event
                            WHERE event.orchestration_id = orchestration.id
                              AND event.company_id = action.company_id
                              AND event.event_type = 'required_subtask_planned'
                              AND event.payload->>'issueId' = child.id::text
                       )
                       AND EXISTS (
                           SELECT 1
                             FROM task_team_plans plan
                             JOIN task_team_members member
                               ON member.plan_id = plan.id
                              AND member.company_id = plan.company_id
                            WHERE plan.orchestration_id = orchestration.id
                              AND plan.company_id = action.company_id
                              AND plan.status = 'approved'
                              AND member.exited_at IS NULL
                              AND ((member.action = 'reuse' AND member.agent_id = agent.id)
                                OR (member.action = 'hire'
                                    AND member.hire_status = 'hired'
                                    AND (member.agent_id = agent.id
                                      OR member.hired_agent_id = agent.id)))
                       )
                )
                "#,
            )
            .bind(action_id)
            .bind(company_id)
            .bind(root_issue_id)
            .bind(issue_id)
            .bind(agent_id)
            .bind(allow_running_action)
            .fetch_one(executor)
            .await?
        }
        TaskBudgetResumeReference::SubtaskAccepted { action_id } => {
            sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                      FROM task_orchestration_actions action
                      JOIN task_orchestrations orchestration
                        ON orchestration.id = action.orchestration_id
                       AND orchestration.company_id = action.company_id
                      JOIN companies company ON company.id = action.company_id
                      JOIN issues root
                        ON root.id = orchestration.root_issue_id
                       AND root.company_id = orchestration.company_id
                      JOIN agents agent
                        ON agent.id = $5 AND agent.company_id = action.company_id
                      JOIN task_deliverables deliverable
                        ON deliverable.orchestration_id = orchestration.id
                       AND deliverable.company_id = action.company_id
                       AND deliverable.kind = 'subtask'
                      JOIN task_orchestration_subtasks subtask
                        ON subtask.id = deliverable.subtask_id
                       AND subtask.orchestration_id = orchestration.id
                       AND subtask.company_id = action.company_id
                      JOIN issues child
                        ON child.id = subtask.issue_id
                       AND child.company_id = subtask.company_id
                      JOIN task_acceptance_decisions decision
                        ON decision.deliverable_id = deliverable.id
                       AND decision.orchestration_id = orchestration.id
                       AND decision.company_id = action.company_id
                     WHERE action.id = $1
                       AND action.company_id = $2
                       AND action.action_type = 'wake_assignee'
                       AND (action.status = 'applied' OR ($6 AND action.status = 'running'))
                       AND action.idempotency_key =
                           'subtask-accepted:' || child.id::text || ':' || deliverable.id::text
                       AND action.payload = jsonb_build_object(
                           'issueId', root.id, 'agentId', agent.id,
                           'reason', 'subtask_accepted')
                       AND orchestration.root_issue_id = $3
                       AND orchestration.stage IN ('executing', 'reviewing')
                       AND root.task_orchestration_required
                       AND root.parent_id IS NULL
                       AND root.status::text NOT IN ('done', 'cancelled')
                       AND root.cancelled_at IS NULL AND root.hidden_at IS NULL
                       AND root.id = $4
                       AND subtask.status = 'accepted'
                       AND child.status::text = 'done'
                       AND child.cancelled_at IS NULL AND child.hidden_at IS NULL
                       AND decision.verdict = 'accepted'
                       AND decision.reviewer_agent_id = agent.id
                       AND EXISTS (
                           SELECT 1
                             FROM task_orchestration_events event
                            WHERE event.orchestration_id = orchestration.id
                              AND event.company_id = action.company_id
                              AND event.event_type = 'acceptance_passed'
                              AND event.actor_agent_id = agent.id
                              AND event.payload->>'issueId' = child.id::text
                       )
                       AND company.status::text = 'active'
                       AND (agent.id = orchestration.ceo_agent_id OR EXISTS (
                           SELECT 1
                             FROM task_team_plans plan
                             JOIN task_team_members member
                               ON member.plan_id = plan.id
                              AND member.company_id = plan.company_id
                            WHERE plan.orchestration_id = orchestration.id
                              AND plan.company_id = action.company_id
                              AND plan.status = 'approved'
                              AND member.exited_at IS NULL
                              AND ((member.action = 'reuse' AND member.agent_id = agent.id)
                                OR (member.action = 'hire'
                                    AND member.hire_status = 'hired'
                                    AND (member.agent_id = agent.id
                                      OR member.hired_agent_id = agent.id)))
                       ))
                )
                "#,
            )
            .bind(action_id)
            .bind(company_id)
            .bind(root_issue_id)
            .bind(issue_id)
            .bind(agent_id)
            .bind(allow_running_action)
            .fetch_one(executor)
            .await?
        }
    };
    Ok(live)
}

const TASK_BUDGET_BLOCKED_RETRY_SECONDS: i64 = 5 * 60;

fn task_budget_resume_backoff_seconds(attempts: i32) -> i64 {
    let exponent = attempts.saturating_sub(1).clamp(0, 6) as u32;
    (30_i64.saturating_mul(1_i64 << exponent)).min(30 * 60)
}

// ─────────────────────────────── raw row ───────────────────────────────

/// Raw orchestration row.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OrchRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub ceo_agent_id: Uuid,
    pub stage: String,
    pub stage_version: i64,
    pub recovery_stage: Option<String>,
    pub decision_contract_version: i32,
    pub final_report: Option<String>,
    pub error_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl OrchRow {
    pub fn parsed_stage(&self) -> Result<OrchestrationStage, OrchestrationError> {
        self.stage.parse()
    }
}

const ORCH_COLUMNS: &str = "id, company_id, root_issue_id, ceo_agent_id, stage, stage_version,
     recovery_stage, decision_contract_version, final_report, error_reason,
     created_at, updated_at, completed_at";

// ─────────────────────────── decision contract ───────────────────────────

/// Contract version accepted by [`parse_decision_contract`]. Bump only with a
/// matching migration of persisted `decision_contract` payloads.
pub const DECISION_CONTRACT_VERSION: i32 = 1;
/// Upper bounds so a hostile or runaway model cannot exhaust storage.
pub const MAX_CONTRACT_MEMBERS: usize = 32;
pub const MAX_CONTRACT_SUBTASKS: usize = 64;
pub const MAX_CONTRACT_QUESTIONS: usize = 10;
pub const MAX_CONTRACT_TEXT_CHARS: usize = 4_000;

/// One member in the CEO's proposed task team.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractMember {
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    pub action: String,
    pub role: String,
    pub responsibilities: String,
    pub reason: String,
    #[serde(default)]
    pub reports_to_agent_id: Option<Uuid>,
    #[serde(default)]
    pub hire: Option<ContractHire>,
}

/// A new-agent request. Only fields the existing hire approval understands are
/// accepted; no free-form command, endpoint, or secret is permitted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractHire {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    pub adapter_type: String,
    #[serde(default)]
    pub budget_monthly_cents: Option<i32>,
}

/// One planned subtask. `key` is a contract-local reference used by
/// `blockedBy` and `parentKey`; the server never trusts it as an issue id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractSubtask {
    pub key: String,
    pub title: String,
    pub description: String,
    pub owner_agent_id: Uuid,
    #[serde(default)]
    pub reviewer_agent_id: Option<Uuid>,
    pub acceptance_criteria: String,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub parent_key: Option<String>,
}

/// One clarification question asked of the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractQuestion {
    pub question: String,
    pub why_it_blocks: String,
}

/// The full, versioned CEO decision for one task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecisionContract {
    pub contract_version: i32,
    pub assessment: String,
    /// `simple` or `complex`.
    pub complexity: String,
    pub rationale: String,
    #[serde(default)]
    pub members: Vec<ContractMember>,
    #[serde(default)]
    pub subtasks: Vec<ContractSubtask>,
    #[serde(default)]
    pub questions: Vec<ContractQuestion>,
    /// True when the CEO will execute the task itself.
    #[serde(default)]
    pub ceo_executes: bool,
}

/// One CEO-owned membership change after the plan has been approved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeamAdjustment {
    pub agent_id: Uuid,
    /// `reuse` (add or update the member) or `exit` (end task membership only).
    pub action: String,
    pub role: String,
    pub responsibilities: String,
    pub reason: String,
    #[serde(default)]
    pub reports_to_agent_id: Option<Uuid>,
}

/// Reject a decision contract with a machine-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ContractValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

fn contract_error(field: &str, message: &str) -> ContractValidationError {
    ContractValidationError {
        field: field.to_string(),
        message: message.to_string(),
    }
}

/// Structural validation of a CEO decision contract. This is the untrusted
/// boundary: anything the model wrote is data, and only these shapes are
/// accepted. Company/ownership checks happen in [`TaskOrchestrationService`]
/// where the database is available.
pub fn validate_decision_contract(
    contract: &DecisionContract,
) -> Result<(), ContractValidationError> {
    if contract.contract_version != DECISION_CONTRACT_VERSION {
        return Err(contract_error(
            "contractVersion",
            "unsupported decision contract version",
        ));
    }
    for (field, value) in [
        ("assessment", &contract.assessment),
        ("rationale", &contract.rationale),
    ] {
        if value.trim().is_empty() {
            return Err(contract_error(field, "must not be empty"));
        }
        if value.chars().count() > MAX_CONTRACT_TEXT_CHARS {
            return Err(contract_error(field, "exceeds the maximum length"));
        }
    }
    if !matches!(contract.complexity.as_str(), "simple" | "complex") {
        return Err(contract_error("complexity", "must be simple or complex"));
    }
    if contract.members.len() > MAX_CONTRACT_MEMBERS {
        return Err(contract_error("members", "too many members"));
    }
    if contract.subtasks.len() > MAX_CONTRACT_SUBTASKS {
        return Err(contract_error("subtasks", "too many subtasks"));
    }
    if contract.questions.len() > MAX_CONTRACT_QUESTIONS {
        return Err(contract_error("questions", "too many questions"));
    }

    let mut seen_agents = BTreeSet::new();
    let mut hire_names = BTreeSet::new();
    let mut reused_agents = BTreeSet::new();
    let mut exited_agents = BTreeSet::new();
    for (index, member) in contract.members.iter().enumerate() {
        let at = format!("members[{index}]");
        match member.action.as_str() {
            "reuse" | "exit" => {
                let Some(agent_id) = member.agent_id else {
                    return Err(contract_error(&at, "agentId is required for reuse/exit"));
                };
                if member.hire.is_some() {
                    return Err(contract_error(&at, "hire is only valid for a hire action"));
                }
                if member.action == "reuse" {
                    reused_agents.insert(agent_id);
                } else {
                    exited_agents.insert(agent_id);
                }
            }
            "hire" => {
                if member.agent_id.is_some() {
                    return Err(contract_error(
                        &at,
                        "a hire must not name an existing agent",
                    ));
                }
                let Some(hire) = member.hire.as_ref() else {
                    return Err(contract_error(&at, "hire details are required"));
                };
                if hire.name.trim().is_empty() {
                    return Err(contract_error(
                        &format!("{at}.hire.name"),
                        "must not be empty",
                    ));
                }
                if hire.adapter_type.trim().is_empty() {
                    return Err(contract_error(
                        &format!("{at}.hire.adapterType"),
                        "must not be empty",
                    ));
                }
                if let Some(budget) = hire.budget_monthly_cents {
                    if budget < 0 {
                        return Err(contract_error(
                            &format!("{at}.hire.budgetMonthlyCents"),
                            "must not be negative",
                        ));
                    }
                }
                if !hire_names.insert(hire.name.trim().to_ascii_lowercase()) {
                    return Err(contract_error(&at, "duplicate hire name"));
                }
            }
            _ => return Err(contract_error(&at, "action must be reuse, hire, or exit")),
        }
        for text in [&member.role, &member.responsibilities, &member.reason] {
            if text.trim().is_empty() {
                return Err(contract_error(
                    &at,
                    "role/responsibilities/reason must not be empty",
                ));
            }
            if text.chars().count() > MAX_CONTRACT_TEXT_CHARS {
                return Err(contract_error(
                    &at,
                    "role/responsibilities/reason exceeds the maximum length",
                ));
            }
        }
        if let Some(agent_id) = member.agent_id {
            if !seen_agents.insert(agent_id) {
                return Err(contract_error(&at, "agent appears more than once"));
            }
        }
        if member.agent_id.is_some() && member.agent_id == member.reports_to_agent_id {
            return Err(contract_error(&at, "an agent cannot report to itself"));
        }
        if member.action == "exit" && member.reports_to_agent_id.is_some() {
            return Err(contract_error(
                &at,
                "an exiting member must not declare a superior",
            ));
        }
    }
    if reused_agents
        .iter()
        .any(|agent| exited_agents.contains(agent))
    {
        return Err(contract_error(
            "members",
            "an agent cannot be both reused and exited",
        ));
    }

    // A simple task must not manufacture subtasks or a hierarchy.
    if contract.complexity == "simple" && !contract.subtasks.is_empty() {
        return Err(contract_error(
            "subtasks",
            "a simple task must not declare subtasks",
        ));
    }
    // Open questions gate the plan; work must not be dispatched before the
    // answers exist.
    if !contract.questions.is_empty() && !contract.subtasks.is_empty() {
        return Err(contract_error(
            "subtasks",
            "subtasks must not be planned while clarification questions are open",
        ));
    }

    let mut keys = BTreeSet::new();
    for (index, subtask) in contract.subtasks.iter().enumerate() {
        let at = format!("subtasks[{index}]");
        if subtask.key.trim().is_empty() {
            return Err(contract_error(&format!("{at}.key"), "must not be empty"));
        }
        if !keys.insert(subtask.key.clone()) {
            return Err(contract_error(&at, "duplicate subtask key"));
        }
        for (field, value) in [
            ("title", &subtask.title),
            ("description", &subtask.description),
            ("acceptanceCriteria", &subtask.acceptance_criteria),
        ] {
            if value.trim().is_empty() {
                return Err(contract_error(
                    &format!("{at}.{field}"),
                    "must not be empty",
                ));
            }
            if value.chars().count() > MAX_CONTRACT_TEXT_CHARS {
                return Err(contract_error(
                    &format!("{at}.{field}"),
                    "exceeds the maximum length",
                ));
            }
        }
        if subtask.reviewer_agent_id == Some(subtask.owner_agent_id) {
            return Err(contract_error(&at, "an owner cannot review its own work"));
        }
        if exited_agents.contains(&subtask.owner_agent_id) {
            return Err(contract_error(
                &format!("{at}.ownerAgentId"),
                "an exiting member cannot own a subtask",
            ));
        }
        if !reused_agents.contains(&subtask.owner_agent_id) {
            // Owners must be existing, reusable members. A pending hire has no
            // agent id yet and therefore cannot be assigned work.
            return Err(contract_error(
                &format!("{at}.ownerAgentId"),
                "the owner must be a reused member of the task team",
            ));
        }
        if let Some(reviewer) = subtask.reviewer_agent_id {
            if !reused_agents.contains(&reviewer) {
                return Err(contract_error(
                    &format!("{at}.reviewerAgentId"),
                    "the reviewer must be a reused member of the task team",
                ));
            }
        }
        if subtask.blocked_by.len() > MAX_CONTRACT_SUBTASKS {
            return Err(contract_error(&at, "too many dependencies"));
        }
        let mut seen = BTreeSet::new();
        for dependency in &subtask.blocked_by {
            if !seen.insert(dependency.clone()) {
                return Err(contract_error(&at, "duplicate dependency"));
            }
            if dependency == &subtask.key {
                return Err(contract_error(
                    &format!("{at}.blockedBy"),
                    "a subtask cannot depend on itself",
                ));
            }
        }
    }

    // Every referenced key must exist within the contract.
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut dependencies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (index, subtask) in contract.subtasks.iter().enumerate() {
        let at = format!("subtasks[{index}]");
        if let Some(parent) = subtask.parent_key.as_ref() {
            if parent == &subtask.key {
                return Err(contract_error(
                    &format!("{at}.parentKey"),
                    "a subtask cannot parent itself",
                ));
            }
            if !keys.contains(parent) {
                return Err(contract_error(
                    &format!("{at}.parentKey"),
                    "parentKey must reference another subtask in this contract",
                ));
            }
            parents.insert(subtask.key.clone(), parent.clone());
        }
        let mut edges = BTreeSet::new();
        for dependency in &subtask.blocked_by {
            if !keys.contains(dependency) {
                return Err(contract_error(
                    &format!("{at}.blockedBy"),
                    "blockedBy must reference another subtask in this contract",
                ));
            }
            edges.insert(dependency.clone());
        }
        dependencies.insert(subtask.key.clone(), edges);
    }

    // Parent cycles.
    for start in parents.keys() {
        let mut seen = BTreeSet::new();
        let mut cursor = Some(start.clone());
        while let Some(current) = cursor {
            if !seen.insert(current.clone()) {
                return Err(contract_error("subtasks", "parent chain contains a cycle"));
            }
            cursor = parents.get(&current).cloned();
        }
    }
    // Dependency cycles.
    for start in dependencies.keys() {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                return Err(contract_error(
                    "subtasks",
                    "dependency graph contains a cycle",
                ));
            }
            if let Some(edges) = dependencies.get(&current) {
                stack.extend(edges.iter().cloned());
            }
        }
    }

    for (index, question) in contract.questions.iter().enumerate() {
        let at = format!("questions[{index}]");
        for (field, value) in [
            ("question", &question.question),
            ("whyItBlocks", &question.why_it_blocks),
        ] {
            if value.trim().is_empty() {
                return Err(contract_error(
                    &format!("{at}.{field}"),
                    "must not be empty",
                ));
            }
            if value.chars().count() > MAX_CONTRACT_TEXT_CHARS {
                return Err(contract_error(
                    &format!("{at}.{field}"),
                    "exceeds the maximum length",
                ));
            }
        }
    }

    // A task needs exactly one owner path: the CEO itself, or at least one
    // reviewable subtask. Two revisions are exempt: asking-only (the CEO plans
    // after the answers) and hire-only (the CEO plans after the hire lands).
    let is_bridge_revision = !contract.questions.is_empty()
        || contract
            .members
            .iter()
            .any(|member| member.action == "hire");
    if !contract.ceo_executes && contract.subtasks.is_empty() && !is_bridge_revision {
        return Err(contract_error(
            "subtasks",
            "declare at least one subtask or set ceoExecutes",
        ));
    }

    Ok(())
}

/// Parse and structurally validate a raw JSON decision contract. The returned
/// contract is still untrusted; the service re-validates company scope and
/// reporting edges before persisting anything.
pub fn parse_decision_contract(
    raw: &serde_json::Value,
) -> Result<DecisionContract, ContractValidationError> {
    let contract: DecisionContract = serde_json::from_value(raw.clone())
        .map_err(|error| contract_error("contract", &error.to_string()))?;
    validate_decision_contract(&contract)?;
    Ok(contract)
}

// ─────────────────────────────── service ───────────────────────────────

/// The persistent orchestration service. Cheap to clone; shared across HTTP
/// routes, heartbeat hooks and the recovery worker.
#[derive(Clone)]
pub struct TaskOrchestrationService {
    pool: PgPool,
    heartbeat: Arc<dyn HeartbeatService>,
    agent_service: Option<Arc<dyn AgentService>>,
    approval_service: Option<Arc<dyn ApprovalService>>,
    issue_service: Option<Arc<dyn IssueService>>,
}

impl TaskOrchestrationService {
    /// Service with the full hire/issue toolchain (routes and workers).
    pub fn new(
        pool: PgPool,
        heartbeat: Arc<dyn HeartbeatService>,
        agent_service: Arc<dyn AgentService>,
        approval_service: Arc<dyn ApprovalService>,
        issue_service: Arc<dyn IssueService>,
    ) -> Self {
        Self {
            pool,
            heartbeat,
            agent_service: Some(agent_service),
            approval_service: Some(approval_service),
            issue_service: Some(issue_service),
        }
    }

    /// Read/maintenance-only variant (heartbeat hooks, recovery jobs). Any
    /// operation that needs to hire or create issues returns
    /// [`OrchestrationError::Service`].
    pub fn read_only(pool: PgPool, heartbeat: Arc<dyn HeartbeatService>) -> Self {
        Self {
            pool,
            heartbeat,
            agent_service: None,
            approval_service: None,
            issue_service: None,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn heartbeat(&self) -> Arc<dyn HeartbeatService> {
        self.heartbeat.clone()
    }

    fn require_agent_service(&self) -> Result<&Arc<dyn AgentService>, OrchestrationError> {
        self.agent_service
            .as_ref()
            .ok_or_else(|| OrchestrationError::Service("agent service is not configured".into()))
    }

    fn require_approval_service(&self) -> Result<&Arc<dyn ApprovalService>, OrchestrationError> {
        self.approval_service
            .as_ref()
            .ok_or_else(|| OrchestrationError::Service("approval service is not configured".into()))
    }

    fn require_issue_service(&self) -> Result<&Arc<dyn IssueService>, OrchestrationError> {
        self.issue_service
            .as_ref()
            .ok_or_else(|| OrchestrationError::Service("issue service is not configured".into()))
    }

    // ─────────────────────── lifecycle ───────────────────────

    /// Create the orchestration for a root issue, idempotently.
    ///
    /// Re-invoking for the same root issue returns the existing instance, so a
    /// retried request or a replayed wake cannot fork a second workflow. The
    /// CEO must be an active, invokable CEO agent of the same company.
    pub async fn start(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        ceo_agent_id: Uuid,
    ) -> Result<OrchRow, OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;

        let issue_company: Option<Uuid> =
            sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
                .bind(root_issue_id)
                .fetch_optional(&mut *tx)
                .await?;
        if issue_company != Some(company_id) {
            tx.rollback().await?;
            return Err(OrchestrationError::CrossCompanyAccess);
        }

        // Existing orchestrations are idempotent even if the CEO was later
        // paused or terminated. Validate executability only when creating the
        // first row, otherwise a retry can incorrectly break Board replay.
        if let Some(existing) = sqlx::query_as::<_, OrchRow>(&format!(
            "SELECT {ORCH_COLUMNS} FROM task_orchestrations \
             WHERE company_id = $1 AND root_issue_id = $2"
        ))
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(existing);
        }

        let ceo_ok: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents
              WHERE id = $1 AND company_id = $2 AND paused_at IS NULL
                AND status IN ('idle', 'running') AND role = 'ceo'",
        )
        .bind(ceo_agent_id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await?;
        if ceo_ok.is_none() {
            tx.rollback().await?;
            return Err(OrchestrationError::AgentNotInCompany(ceo_agent_id));
        }

        let row: OrchRow = sqlx::query_as(&format!(
            "INSERT INTO task_orchestrations (company_id, root_issue_id, ceo_agent_id)
             VALUES ($1, $2, $3) RETURNING {ORCH_COLUMNS}"
        ))
        .bind(company_id)
        .bind(root_issue_id)
        .bind(ceo_agent_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO task_orchestration_events
                (orchestration_id, company_id, event_type, actor_agent_id, stage_after, payload)
             VALUES ($1, $2, 'orchestration_started', $3, 'assessing', $4)",
        )
        .bind(row.id)
        .bind(company_id)
        .bind(ceo_agent_id)
        .bind(serde_json::json!({ "rootIssueId": root_issue_id }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    /// Load the orchestration for a root issue, if any.
    pub async fn find(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<Option<OrchRow>, OrchestrationError> {
        let row = sqlx::query_as::<_, OrchRow>(&format!(
            "SELECT {ORCH_COLUMNS} FROM task_orchestrations \
             WHERE root_issue_id = $1 AND company_id = $2"
        ))
        .bind(root_issue_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Update or clear the Board-owned lifetime cap on one root task, audit it
    /// in the same transaction, then explicitly replay task-budget-skipped
    /// assignment wakes when the new cap permits work again.
    pub async fn update_task_budget(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        actor_user_id: Uuid,
        budget_cents: Option<i64>,
    ) -> Result<TaskBudgetUpdate, OrchestrationError> {
        if budget_cents.is_some_and(|amount| amount < 0) {
            return Err(OrchestrationError::ContractInvalid {
                field: "budgetCents".to_string(),
                message: "must be a non-negative integer or null".to_string(),
            });
        }

        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let issue = sqlx::query(
            "SELECT parent_id, task_budget_cents, task_budget_revision
               FROM issues WHERE id = $1 AND company_id = $2 FOR UPDATE",
        )
        .bind(root_issue_id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(OrchestrationError::CrossCompanyAccess)?;
        if issue.try_get::<Option<Uuid>, _>("parent_id")?.is_some() {
            tx.rollback().await?;
            return Err(OrchestrationError::ContractInvalid {
                field: "issueId".to_string(),
                message: "task budget can only be set on a root issue".to_string(),
            });
        }
        let old_budget: Option<i64> = issue.try_get("task_budget_cents")?;
        let mut revision: i64 = issue.try_get("task_budget_revision")?;
        let changed = old_budget != budget_cents;
        if changed {
            revision = sqlx::query_scalar(
                "UPDATE issues
                    SET task_budget_cents = $3,
                        task_budget_revision = task_budget_revision + 1,
                        updated_at = NOW()
                  WHERE id = $1 AND company_id = $2
                  RETURNING task_budget_revision",
            )
            .bind(root_issue_id)
            .bind(company_id)
            .bind(budget_cents)
            .fetch_one(&mut *tx)
            .await?;

            let payload = serde_json::json!({
                "oldBudgetCents": old_budget,
                "budgetCents": budget_cents,
                "revision": revision,
            });
            sqlx::query(
                "INSERT INTO activity_logs
                    (id, company_id, event_type, actor_type, actor_id,
                     resource_type, resource_id, metadata, created_at)
                 VALUES ($1, $2, 'issue.task_budget_updated', 'user', $3,
                         'issue', $4, $5, NOW())",
            )
            .bind(Uuid::new_v4())
            .bind(company_id)
            .bind(actor_user_id)
            .bind(root_issue_id)
            .bind(&payload)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO task_orchestration_events
                    (orchestration_id, company_id, event_type, actor_user_id, payload)
                 SELECT id, company_id, 'task_budget_updated', $3, $4
                   FROM task_orchestrations
                  WHERE company_id = $1 AND root_issue_id = $2",
            )
            .bind(company_id)
            .bind(root_issue_id)
            .bind(actor_user_id)
            .bind(&payload)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE task_budget_wake_resumes
                    SET next_attempt_at = NOW(), updated_at = NOW()
                  WHERE company_id = $1 AND root_issue_id = $2 AND status = 'pending'",
            )
            .bind(company_id)
            .bind(root_issue_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        if changed {
            crate::live_events::publish_live_event(
                company_id,
                "task_orchestration.updated",
                serde_json::json!({
                    "issueId": root_issue_id,
                    "action": "budget",
                    "budgetCents": budget_cents,
                    "revision": revision,
                }),
            )
            .await;
        }

        let spent_cents = task_tree_spend_cents(&self.pool, company_id, root_issue_id).await?;
        let resumed_wake_count = self
            .recover_task_budget_wake_resumes_for_root(company_id, root_issue_id, 128)
            .await?;

        Ok(TaskBudgetUpdate {
            issue_id: root_issue_id,
            budget_cents,
            spent_cents,
            remaining_cents: budget_cents.map(|amount| amount.saturating_sub(spent_cents)),
            exhausted: budget_cents.is_some_and(|amount| spent_cents >= amount),
            revision,
            resumed_wake_count,
        })
    }

    /// Claim and replay durable task-budget wake intents. Claims are short
    /// leases committed before calling Heartbeat, so a process crash releases
    /// work after the lease expires and a second worker cannot dispatch the
    /// same intent concurrently.
    pub async fn recover_pending_task_budget_wake_resumes(
        &self,
        limit: usize,
    ) -> Result<usize, OrchestrationError> {
        self.recover_task_budget_wake_resumes(limit, None).await
    }

    async fn recover_task_budget_wake_resumes_for_root(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        limit: usize,
    ) -> Result<usize, OrchestrationError> {
        self.recover_task_budget_wake_resumes(limit, Some((company_id, root_issue_id)))
            .await
    }

    async fn recover_task_budget_wake_resumes(
        &self,
        limit: usize,
        scope: Option<(Uuid, Uuid)>,
    ) -> Result<usize, OrchestrationError> {
        if limit == 0 {
            return Ok(0);
        }
        let claim_token = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;
        let batch_limit = limit.min(32) as i64;
        let wakes: Vec<TaskBudgetWakeResume> = sqlx::query_as(
            r#"
            WITH due AS (
                SELECT id
                  FROM task_budget_wake_resumes
                 WHERE ((status = 'pending' AND next_attempt_at <= NOW())
                    OR (status = 'processing' AND claim_expires_at <= NOW()))
                   AND ($2::uuid IS NULL OR company_id = $2)
                   AND ($3::uuid IS NULL OR root_issue_id = $3)
                 ORDER BY COALESCE(claim_expires_at, next_attempt_at), created_at
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED
            )
            UPDATE task_budget_wake_resumes AS resume
               SET status = 'processing',
                   claim_token = $4,
                   claim_expires_at = NOW() + INTERVAL '2 minutes',
                   attempts = resume.attempts + 1,
                   updated_at = NOW()
              FROM due
             WHERE resume.id = due.id
            RETURNING resume.id, resume.company_id, resume.root_issue_id,
                      resume.issue_id, resume.agent_id,
                      resume.requested_by_actor_type, resume.requested_by_actor_id,
                      resume.resume_source, resume.resume_trigger_detail,
                      resume.resume_reason, resume.resume_idempotency_key, resume.resume_kind,
                      resume.source_task_action_id, resume.resume_payload,
                      resume.resume_context_snapshot, resume.retry_of_run_id,
                      resume.scheduled_retry_attempt, resume.attempts, resume.claim_token
            "#,
        )
        .bind(batch_limit)
        .bind(scope.map(|(company_id, _)| company_id))
        .bind(scope.map(|(_, root_issue_id)| root_issue_id))
        .bind(claim_token)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let mut applied = 0;
        for wake in wakes {
            match self.replay_task_budget_wake_resume(&wake).await {
                Ok(true) => applied += 1,
                Ok(false) => {}
                Err(error) => {
                    self.finish_task_budget_wake_resume(
                        &wake,
                        "pending",
                        None,
                        Some("wake replay will retry after a transient service error"),
                        Some(task_budget_resume_backoff_seconds(wake.attempts)),
                    )
                    .await?;
                    tracing::warn!(
                        company_id = %wake.company_id,
                        issue_id = %wake.issue_id,
                        intent_id = %wake.id,
                        error = %error,
                        "task-budget wake resume deferred"
                    );
                }
            }
        }
        Ok(applied)
    }

    async fn replay_task_budget_wake_resume(
        &self,
        wake: &TaskBudgetWakeResume,
    ) -> Result<bool, OrchestrationError> {
        let Some(expected_root_id) = wake.root_issue_id else {
            self.finish_task_budget_wake_resume(
                wake,
                "unsupported",
                None,
                Some("task root was not resolved when the source wake was skipped"),
                None,
            )
            .await?;
            return Ok(false);
        };

        let resume_reference = match (wake.resume_kind.as_str(), wake.source_task_action_id) {
            ("generic", None) => None,
            ("subtask_assigned", Some(action_id)) => {
                Some(TaskBudgetResumeReference::SubtaskAssigned { action_id })
            }
            ("subtask_accepted", Some(action_id)) => {
                Some(TaskBudgetResumeReference::SubtaskAccepted { action_id })
            }
            _ => {
                self.finish_task_budget_wake_resume(
                    wake,
                    "unsupported",
                    None,
                    Some("stored task resume reference is malformed"),
                    None,
                )
                .await?;
                return Ok(false);
            }
        };

        let Some(root) = task_budget_root(&self.pool, wake.company_id, wake.issue_id).await? else {
            self.finish_task_budget_wake_resume(
                wake,
                "cancelled",
                None,
                Some("issue no longer resolves to a same-company task root"),
                None,
            )
            .await?;
            return Ok(false);
        };
        if root.root_issue_id != expected_root_id {
            self.finish_task_budget_wake_resume(
                wake,
                "cancelled",
                None,
                Some("issue moved to a different task root after the source wake"),
                None,
            )
            .await?;
            return Ok(false);
        }

        let state: Option<TaskBudgetWakeResumeState> = sqlx::query_as(
            r#"
            SELECT issue.status::text AS issue_status,
                   issue.cancelled_at AS issue_cancelled_at,
                   issue.hidden_at AS issue_hidden_at,
                   issue.assignee_agent_id AS issue_assignee_agent_id,
                   root.status::text AS root_status,
                   root.cancelled_at AS root_cancelled_at,
                   root.hidden_at AS root_hidden_at,
                   root.task_budget_cents AS budget_cents,
                   orchestration.stage AS orchestration_stage
              FROM issues issue
              JOIN issues root
                ON root.id = $3 AND root.company_id = $1 AND root.parent_id IS NULL
              JOIN agents agent
                ON agent.id = $4 AND agent.company_id = $1
              LEFT JOIN task_orchestrations orchestration
                ON orchestration.company_id = $1 AND orchestration.root_issue_id = root.id
             WHERE issue.id = $2 AND issue.company_id = $1
            "#,
        )
        .bind(wake.company_id)
        .bind(wake.issue_id)
        .bind(expected_root_id)
        .bind(wake.agent_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(state) = state else {
            self.finish_task_budget_wake_resume(
                wake,
                "cancelled",
                None,
                Some("issue, root, or agent is no longer in the source company"),
                None,
            )
            .await?;
            return Ok(false);
        };

        if resume_reference.is_none()
            && state.issue_assignee_agent_id != Some(wake.agent_id)
        {
            self.finish_task_budget_wake_resume(
                wake,
                "cancelled",
                None,
                Some("issue assignee changed after the source wake"),
                None,
            )
            .await?;
            return Ok(false);
        }
        if let Some(reference) = resume_reference {
            let live = task_budget_resume_reference_is_live(
                &self.pool,
                wake.company_id,
                expected_root_id,
                wake.issue_id,
                wake.agent_id,
                reference,
                true,
            )
            .await?;
            if !live {
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    None,
                    Some("source task action, assignment, reviewer, or deliverable changed"),
                    None,
                )
                .await?;
                return Ok(false);
            }
        }
        let terminal_issue = |status: &str,
                              cancelled_at: Option<DateTime<Utc>>,
                              hidden_at: Option<DateTime<Utc>>| {
            matches!(status, "done" | "cancelled") || cancelled_at.is_some() || hidden_at.is_some()
        };
        if terminal_issue(
            &state.issue_status,
            state.issue_cancelled_at,
            state.issue_hidden_at,
        ) || terminal_issue(
            &state.root_status,
            state.root_cancelled_at,
            state.root_hidden_at,
        ) || state
            .orchestration_stage
            .as_deref()
            .is_some_and(|stage| matches!(stage, "completed" | "failed" | "cancelled"))
        {
            self.finish_task_budget_wake_resume(
                wake,
                "cancelled",
                None,
                Some("issue or orchestration became terminal or cancelled"),
                None,
            )
            .await?;
            return Ok(false);
        }

        if !matches!(
            state.issue_status.as_str(),
            "todo" | "in_progress" | "in_review"
        ) || !matches!(
            state.root_status.as_str(),
            "todo" | "in_progress" | "in_review"
        ) {
            self.finish_task_budget_wake_resume(
                wake,
                "pending",
                None,
                Some("issue is not currently in a wakeable status"),
                Some(TASK_BUDGET_BLOCKED_RETRY_SECONDS),
            )
            .await?;
            return Ok(false);
        }

        if let Some(budget_cents) = state.budget_cents {
            let spent_cents =
                task_tree_spend_cents(&self.pool, wake.company_id, expected_root_id).await?;
            if spent_cents >= budget_cents {
                self.finish_task_budget_wake_resume(
                    wake,
                    "pending",
                    None,
                    Some("task budget remains exhausted"),
                    Some(TASK_BUDGET_BLOCKED_RETRY_SECONDS),
                )
                .await?;
                return Ok(false);
            }
        }

        let options = if let Some(reference) = resume_reference {
            let reason = reference.kind();
            HeartbeatWakeupOptions {
                source: Some("automation".to_string()),
                trigger_detail: Some("task_orchestration".to_string()),
                reason: Some(reason.to_string()),
                requested_by_actor_type: Some("system".to_string()),
                requested_by_actor_id: None,
                idempotency_key: Some(wake.resume_idempotency_key.clone()),
                payload: Some(serde_json::json!({
                    "issueId": wake.issue_id,
                    "agentId": wake.agent_id,
                    "reason": reason,
                })),
                context_snapshot: Some(serde_json::json!({
                    "issueId": wake.issue_id,
                    "taskId": wake.issue_id,
                    "wakeReason": reason,
                    "source": "task_orchestration",
                })),
                task_budget_resume_intent_id: Some(wake.id),
                task_budget_resume_reference: Some(reference),
                ..Default::default()
            }
        } else {
            HeartbeatWakeupOptions {
                source: wake.resume_source.clone(),
                trigger_detail: wake.resume_trigger_detail.clone(),
                reason: wake.resume_reason.clone(),
                requested_by_actor_type: wake.requested_by_actor_type.clone(),
                requested_by_actor_id: wake.requested_by_actor_id,
                idempotency_key: Some(wake.resume_idempotency_key.clone()),
                payload: wake.resume_payload.clone(),
                context_snapshot: wake.resume_context_snapshot.clone(),
                retry_of_run_id: wake.retry_of_run_id,
                scheduled_retry_attempt: wake.scheduled_retry_attempt,
                task_budget_resume_intent_id: Some(wake.id),
                ..Default::default()
            }
        };
        if task_budget_resume_options_supported(wake.issue_id, &options).is_err() {
            self.finish_task_budget_wake_resume(
                wake,
                "unsupported",
                None,
                Some("stored wake options no longer satisfy the replay allowlist"),
                None,
            )
            .await?;
            return Ok(false);
        }
        if let Some(retry_run_id) = wake.retry_of_run_id {
            let same_scope: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM heartbeat_runs
                      WHERE id = $1 AND company_id = $2 AND agent_id = $3
                        AND (context_snapshot->>'issueId' = $4
                             OR context_snapshot->>'taskId' = $4)
                 )",
            )
            .bind(retry_run_id)
            .bind(wake.company_id)
            .bind(wake.agent_id)
            .bind(wake.issue_id.to_string())
            .fetch_one(&self.pool)
            .await?;
            if !same_scope {
                self.finish_task_budget_wake_resume(
                    wake,
                    "unsupported",
                    None,
                    Some("retry run is no longer in the same company, agent, and issue scope"),
                    None,
                )
                .await?;
                return Ok(false);
            }
        }

        let existing_request = self.load_task_budget_resume_request(wake).await?;
        if let Some((request_id, _status, Some(_run_id))) = existing_request.as_ref() {
            return self
                .finish_task_budget_wake_resume(wake, "applied", Some(*request_id), None, None)
                .await;
        }
        if let Some((request_id, status, None)) = existing_request.as_ref() {
            if status == "cancelled" {
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    Some(*request_id),
                    Some("replay wake request was cancelled"),
                    None,
                )
                .await?;
                return Ok(false);
            }
        }

        let wake_result = if let Some(reference) = resume_reference {
            // Serialize the final live-state check and dispatch against task
            // cancellation and budget changes. This is the same advisory root
            // lock used by `cancel` and `update_task_budget`; no issue row lock
            // is held while Heartbeat uses another pool connection.
            let mut root_lock = self.pool.begin().await?;
            Self::lock_issue(&mut root_lock, expected_root_id).await?;

            let current_root = task_budget_root(&self.pool, wake.company_id, wake.issue_id).await?;
            if current_root
                .as_ref()
                .is_none_or(|root| root.root_issue_id != expected_root_id)
            {
                root_lock.rollback().await?;
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    None,
                    Some("task root changed before replay dispatch"),
                    None,
                )
                .await?;
                return Ok(false);
            }

            let current_state: Option<TaskBudgetWakeResumeState> = sqlx::query_as(
                r#"
                SELECT issue.status::text AS issue_status,
                       issue.cancelled_at AS issue_cancelled_at,
                       issue.hidden_at AS issue_hidden_at,
                       issue.assignee_agent_id AS issue_assignee_agent_id,
                       root.status::text AS root_status,
                       root.cancelled_at AS root_cancelled_at,
                       root.hidden_at AS root_hidden_at,
                       root.task_budget_cents AS budget_cents,
                       orchestration.stage AS orchestration_stage
                  FROM issues issue
                  JOIN issues root
                    ON root.id = $3 AND root.company_id = $1 AND root.parent_id IS NULL
                  JOIN agents agent
                    ON agent.id = $4 AND agent.company_id = $1
                  LEFT JOIN task_orchestrations orchestration
                    ON orchestration.company_id = $1 AND orchestration.root_issue_id = root.id
                 WHERE issue.id = $2 AND issue.company_id = $1
                "#,
            )
            .bind(wake.company_id)
            .bind(wake.issue_id)
            .bind(expected_root_id)
            .bind(wake.agent_id)
            .fetch_optional(&self.pool)
            .await?;
            let Some(current_state) = current_state else {
                root_lock.rollback().await?;
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    None,
                    Some("task issue, root, or agent disappeared before replay dispatch"),
                    None,
                )
                .await?;
                return Ok(false);
            };
            let is_terminal = |status: &str,
                               cancelled_at: Option<DateTime<Utc>>,
                               hidden_at: Option<DateTime<Utc>>| {
                matches!(status, "done" | "cancelled")
                    || cancelled_at.is_some()
                    || hidden_at.is_some()
            };
            if is_terminal(
                &current_state.issue_status,
                current_state.issue_cancelled_at,
                current_state.issue_hidden_at,
            ) || is_terminal(
                &current_state.root_status,
                current_state.root_cancelled_at,
                current_state.root_hidden_at,
            ) || current_state
                .orchestration_stage
                .as_deref()
                .is_some_and(|stage| matches!(stage, "completed" | "failed" | "cancelled"))
            {
                root_lock.rollback().await?;
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    None,
                    Some("task became terminal or cancelled before replay dispatch"),
                    None,
                )
                .await?;
                return Ok(false);
            }
            if !matches!(
                current_state.issue_status.as_str(),
                "todo" | "in_progress" | "in_review"
            ) || !matches!(
                current_state.root_status.as_str(),
                "todo" | "in_progress" | "in_review"
            ) {
                root_lock.rollback().await?;
                self.finish_task_budget_wake_resume(
                    wake,
                    "pending",
                    None,
                    Some("task is not currently wakeable"),
                    Some(TASK_BUDGET_BLOCKED_RETRY_SECONDS),
                )
                .await?;
                return Ok(false);
            }

            if !task_budget_resume_reference_is_live(
                &self.pool,
                wake.company_id,
                expected_root_id,
                wake.issue_id,
                wake.agent_id,
                reference,
                true,
            )
            .await?
            {
                root_lock.rollback().await?;
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    None,
                    Some("source task action changed before replay dispatch"),
                    None,
                )
                .await?;
                return Ok(false);
            }

            if let Some(budget_cents) = current_root.and_then(|root| root.budget_cents) {
                let spent_cents =
                    task_tree_spend_cents(&self.pool, wake.company_id, expected_root_id).await?;
                if spent_cents >= budget_cents {
                    root_lock.rollback().await?;
                    self.finish_task_budget_wake_resume(
                        wake,
                        "pending",
                        None,
                        Some("task budget became exhausted before replay dispatch"),
                        Some(TASK_BUDGET_BLOCKED_RETRY_SECONDS),
                    )
                    .await?;
                    return Ok(false);
                }
            }

            let result = self
                .heartbeat
                .wakeup_with_options(wake.agent_id, wake.issue_id, wake.company_id, options)
                .await;
            root_lock.commit().await?;
            result
        } else {
            self.heartbeat
                .wakeup_with_options(wake.agent_id, wake.issue_id, wake.company_id, options)
                .await
        };

        if let Err(error) = wake_result {
            self.finish_task_budget_wake_resume(
                wake,
                "pending",
                None,
                Some("heartbeat wake returned a transient error"),
                Some(task_budget_resume_backoff_seconds(wake.attempts)),
            )
            .await?;
            tracing::warn!(
                company_id = %wake.company_id,
                issue_id = %wake.issue_id,
                intent_id = %wake.id,
                error = %error,
                "task-budget wake replay will retry"
            );
            return Ok(false);
        }

        match self.load_task_budget_resume_request(wake).await? {
            Some((request_id, _status, Some(_run_id))) => {
                self.finish_task_budget_wake_resume(wake, "applied", Some(request_id), None, None)
                    .await
            }
            Some((request_id, status, None)) if status == "cancelled" => {
                self.finish_task_budget_wake_resume(
                    wake,
                    "cancelled",
                    Some(request_id),
                    Some("heartbeat cancelled the replay request before dispatch"),
                    None,
                )
                .await
            }
            Some((request_id, status, None)) => {
                let delay = if status == "skipped" {
                    TASK_BUDGET_BLOCKED_RETRY_SECONDS
                } else {
                    task_budget_resume_backoff_seconds(wake.attempts)
                };
                self.finish_task_budget_wake_resume(
                    wake,
                    "pending",
                    Some(request_id),
                    Some("heartbeat accepted the call without a durable run link"),
                    Some(delay),
                )
                .await
            }
            None => {
                self.finish_task_budget_wake_resume(
                    wake,
                    "pending",
                    None,
                    Some("heartbeat returned without a persisted replay request"),
                    Some(task_budget_resume_backoff_seconds(wake.attempts)),
                )
                .await
            }
        }
    }

    async fn load_task_budget_resume_request(
        &self,
        wake: &TaskBudgetWakeResume,
    ) -> Result<Option<(Uuid, String, Option<Uuid>)>, OrchestrationError> {
        Ok(sqlx::query_as(
            "SELECT id, status::text, run_id FROM agent_wakeup_requests
              WHERE company_id = $1 AND agent_id = $2 AND idempotency_key = $3",
        )
        .bind(wake.company_id)
        .bind(wake.agent_id)
        .bind(&wake.resume_idempotency_key)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn finish_task_budget_wake_resume(
        &self,
        wake: &TaskBudgetWakeResume,
        status: &str,
        replay_request_id: Option<Uuid>,
        error: Option<&str>,
        retry_after_seconds: Option<i64>,
    ) -> Result<bool, OrchestrationError> {
        let updated: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE task_budget_wake_resumes
               SET status = $3,
                   claim_token = NULL,
                   claim_expires_at = NULL,
                   resume_wakeup_request_id = COALESCE($4, resume_wakeup_request_id),
                   last_error = $5,
                   next_attempt_at = CASE WHEN $3 = 'pending'
                       THEN NOW() + ($6 * INTERVAL '1 second')
                       ELSE next_attempt_at END,
                   finished_at = CASE WHEN $3 IN ('applied', 'cancelled', 'unsupported')
                       THEN NOW() ELSE NULL END,
                   updated_at = NOW()
             WHERE id = $1 AND status = 'processing' AND claim_token = $2
            RETURNING id
            "#,
        )
        .bind(wake.id)
        .bind(wake.claim_token)
        .bind(status)
        .bind(replay_request_id)
        .bind(error)
        .bind(retry_after_seconds.unwrap_or(0))
        .fetch_optional(&self.pool)
        .await?;
        Ok(updated.is_some())
    }

    async fn lock_issue(
        tx: &mut Transaction<'_, Postgres>,
        root_issue_id: Uuid,
    ) -> Result<(), OrchestrationError> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
            .bind(root_issue_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn load_for_update(
        tx: &mut Transaction<'_, Postgres>,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<OrchRow, OrchestrationError> {
        sqlx::query_as::<_, OrchRow>(&format!(
            "SELECT {ORCH_COLUMNS} FROM task_orchestrations \
             WHERE root_issue_id = $1 AND company_id = $2 FOR UPDATE"
        ))
        .bind(root_issue_id)
        .bind(company_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(OrchestrationError::NotOrchestrated {
            issue_id: root_issue_id,
        })
    }

    async fn pending_question_count(
        tx: &mut Transaction<'_, Postgres>,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<usize, OrchestrationError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issue_thread_interactions
              WHERE company_id = $1 AND issue_id = $2
                AND kind = 'ask_user_questions' AND status = 'pending'",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_one(&mut **tx)
        .await?;
        Ok(count as usize)
    }

    async fn pending_subtask_count(
        tx: &mut Transaction<'_, Postgres>,
        orchestration_id: Uuid,
        company_id: Uuid,
    ) -> Result<usize, OrchestrationError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2
                AND required AND status <> 'accepted'",
        )
        .bind(orchestration_id)
        .bind(company_id)
        .fetch_one(&mut **tx)
        .await?;
        Ok(count as usize)
    }

    // ─────────────────────── event application ───────────────────────

    /// Apply one event atomically and return the updated row.
    pub async fn apply_event(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        event: &OrchestrationEvent,
        actor_agent_id: Option<Uuid>,
    ) -> Result<OrchRow, OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let current = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        let row = Self::apply_event_in_tx(&mut tx, &current, event, actor_agent_id).await?;
        tx.commit().await?;
        Ok(row)
    }

    async fn apply_event_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        current: &OrchRow,
        event: &OrchestrationEvent,
        actor_agent_id: Option<Uuid>,
    ) -> Result<OrchRow, OrchestrationError> {
        let stage = current.parsed_stage()?;
        let recovers_terminal = matches!(
            (stage, event),
            (
                OrchestrationStage::Failed,
                OrchestrationEvent::RetryAfterFailure
            ) | (
                OrchestrationStage::Cancelled,
                OrchestrationEvent::ResumeAfterCancellation
            )
        );
        if stage.is_terminal() && !recovers_terminal {
            return Err(OrchestrationError::TerminalState);
        }
        let pending_questions =
            Self::pending_question_count(tx, current.company_id, current.root_issue_id).await?;
        let pending_subtasks =
            Self::pending_subtask_count(tx, current.id, current.company_id).await?;

        let outcome = Self::next_stage(stage, current, event, pending_questions, pending_subtasks)?;

        let new_version = if outcome.advanced {
            current.stage_version + 1
        } else {
            current.stage_version
        };
        let completed_at = if outcome.stage == OrchestrationStage::Completed {
            Some(Utc::now())
        } else {
            None
        };
        let cancelled_at = if outcome.stage == OrchestrationStage::Cancelled {
            Some(Utc::now())
        } else {
            None
        };
        let error_reason = match event {
            OrchestrationEvent::FailureRecorded { reason } => Some(reason.clone()),
            _ => current.error_reason.clone(),
        };

        sqlx::query(
            "UPDATE task_orchestrations
                SET stage = $3, stage_version = $4, recovery_stage = $5,
                    error_reason = $6,
                    completed_at = COALESCE($7, completed_at),
                    cancelled_at = COALESCE($8, cancelled_at),
                    updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(current.id)
        .bind(current.company_id)
        .bind(outcome.stage.as_str())
        .bind(new_version)
        .bind(outcome.recovery_stage.as_deref())
        .bind(error_reason.as_deref())
        .bind(completed_at)
        .bind(cancelled_at)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            "INSERT INTO task_orchestration_events
                (orchestration_id, company_id, event_type, actor_agent_id,
                 stage_before, stage_after, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(current.id)
        .bind(current.company_id)
        .bind(event.name())
        .bind(actor_agent_id)
        .bind(stage.as_str())
        .bind(outcome.stage.as_str())
        .bind(event_payload(event))
        .execute(&mut **tx)
        .await?;

        Ok(sqlx::query_as::<_, OrchRow>(&format!(
            "SELECT {ORCH_COLUMNS} FROM task_orchestrations WHERE id = $1"
        ))
        .bind(current.id)
        .fetch_one(&mut **tx)
        .await?)
    }

    fn next_stage(
        stage: OrchestrationStage,
        current: &OrchRow,
        event: &OrchestrationEvent,
        pending_questions: usize,
        pending_subtasks: usize,
    ) -> Result<StageOutcome, OrchestrationError> {
        let recovery = current.recovery_stage.as_deref();
        let retake =
            |next: OrchestrationStage, recovery: Option<String>, advanced: bool| StageOutcome {
                stage: next,
                recovery_stage: recovery,
                advanced,
            };
        let restore = |fallback: OrchestrationStage| -> OrchestrationStage {
            recovery
                .and_then(|value| value.parse::<OrchestrationStage>().ok())
                .filter(|parsed| !parsed.is_terminal())
                .unwrap_or(fallback)
        };

        match (stage, event) {
            (OrchestrationStage::Assessing, OrchestrationEvent::AssessmentCompleted) => {
                // Clarification is entered when the plan is prepared, not here:
                // the plan revision must be recorded before the workflow
                // declares what it is waiting on.
                Ok(retake(OrchestrationStage::TeamPlanning, None, true))
            }
            (
                OrchestrationStage::TeamPlanning,
                OrchestrationEvent::TeamPlanPrepared { approval_required },
            ) => {
                let target = if *approval_required {
                    OrchestrationStage::AwaitingTeamApproval
                } else {
                    OrchestrationStage::Planning
                };
                if pending_questions > 0 {
                    Ok(retake(
                        OrchestrationStage::Clarifying,
                        Some(target.as_str().to_string()),
                        true,
                    ))
                } else {
                    Ok(retake(target, None, true))
                }
            }
            (OrchestrationStage::AwaitingTeamApproval, OrchestrationEvent::TeamPlanApproved) => {
                if pending_questions > 0 {
                    Ok(retake(
                        OrchestrationStage::Clarifying,
                        Some(OrchestrationStage::Planning.as_str().to_string()),
                        true,
                    ))
                } else {
                    Ok(retake(OrchestrationStage::Planning, None, true))
                }
            }
            (OrchestrationStage::AwaitingTeamApproval, OrchestrationEvent::TeamPlanRejected) => {
                Ok(retake(OrchestrationStage::TeamPlanning, None, true))
            }
            // A CEO may replace its own plan revision before any subtask exists
            // (for example after answers arrive). Planning stays the stage unless
            // the new revision opens a question or an approval gate.
            (
                OrchestrationStage::Planning,
                OrchestrationEvent::TeamPlanPrepared { approval_required },
            ) => {
                let target = if *approval_required {
                    OrchestrationStage::AwaitingTeamApproval
                } else {
                    OrchestrationStage::Planning
                };
                if pending_questions > 0 {
                    Ok(retake(
                        OrchestrationStage::Clarifying,
                        Some(target.as_str().to_string()),
                        true,
                    ))
                } else {
                    Ok(retake(target, None, target != stage))
                }
            }
            (s, OrchestrationEvent::ClarificationRequested { .. })
                if s.can_receive_clarification() =>
            {
                if s == OrchestrationStage::Clarifying {
                    Ok(retake(OrchestrationStage::Clarifying, None, false))
                } else {
                    Ok(retake(
                        OrchestrationStage::Clarifying,
                        Some(stage.as_str().to_string()),
                        true,
                    ))
                }
            }
            (
                OrchestrationStage::Clarifying,
                OrchestrationEvent::UserClarificationAnswered { .. },
            ) => {
                if pending_questions > 0 {
                    Ok(retake(OrchestrationStage::Clarifying, None, false))
                } else {
                    Ok(retake(restore(OrchestrationStage::Planning), None, true))
                }
            }
            (OrchestrationStage::Planning, OrchestrationEvent::RequiredSubtaskPlanned { .. }) => {
                Ok(retake(OrchestrationStage::Planning, None, false))
            }
            (OrchestrationStage::Planning, OrchestrationEvent::PlanningCompleted) => {
                if pending_questions > 0 {
                    return Err(OrchestrationError::UnansweredQuestionsRemain {
                        count: pending_questions,
                    });
                }
                Ok(retake(OrchestrationStage::Executing, None, true))
            }
            (
                OrchestrationStage::Executing | OrchestrationStage::Reviewing,
                OrchestrationEvent::ExecutionSubmitted { .. },
            ) => {
                if pending_questions > 0 {
                    return Err(OrchestrationError::UnansweredQuestionsRemain {
                        count: pending_questions,
                    });
                }
                Ok(retake(
                    OrchestrationStage::Reviewing,
                    None,
                    stage != OrchestrationStage::Reviewing,
                ))
            }
            (
                OrchestrationStage::Executing | OrchestrationStage::Reviewing,
                OrchestrationEvent::AcceptancePassed { issue_id },
            ) => {
                if *issue_id == current.root_issue_id {
                    if pending_subtasks > 0 {
                        return Err(OrchestrationError::RequiredSubtasksNotAccepted {
                            count: pending_subtasks,
                        });
                    }
                    if pending_questions > 0 {
                        return Err(OrchestrationError::UnansweredQuestionsRemain {
                            count: pending_questions,
                        });
                    }
                    Ok(retake(OrchestrationStage::Reporting, None, true))
                } else {
                    Ok(retake(stage, None, false))
                }
            }
            (
                OrchestrationStage::Executing | OrchestrationStage::Reviewing,
                OrchestrationEvent::AcceptanceRejected { issue_id },
            ) => {
                if *issue_id == current.root_issue_id {
                    Ok(retake(OrchestrationStage::Executing, None, true))
                } else if stage == OrchestrationStage::Reviewing {
                    Ok(retake(OrchestrationStage::Executing, None, true))
                } else {
                    Ok(retake(stage, None, false))
                }
            }
            (OrchestrationStage::Reporting, OrchestrationEvent::FinalReportRecorded) => {
                Ok(retake(OrchestrationStage::Completed, None, true))
            }
            (s, OrchestrationEvent::FailureRecorded { .. }) if s.can_be_interrupted() => {
                Ok(retake(
                    OrchestrationStage::Failed,
                    Some(stage.as_str().to_string()),
                    true,
                ))
            }
            (OrchestrationStage::Failed, OrchestrationEvent::RetryAfterFailure) => Ok(retake(
                restore(OrchestrationStage::TeamPlanning),
                None,
                true,
            )),
            (s, OrchestrationEvent::CancellationRequested) if s.can_be_interrupted() => Ok(retake(
                OrchestrationStage::Cancelled,
                Some(stage.as_str().to_string()),
                true,
            )),
            (OrchestrationStage::Cancelled, OrchestrationEvent::ResumeAfterCancellation) => Ok(
                retake(restore(OrchestrationStage::TeamPlanning), None, true),
            ),
            _ => Err(OrchestrationError::InvalidTransition {
                stage,
                event: event.name().to_string(),
            }),
        }
    }
}

struct StageOutcome {
    stage: OrchestrationStage,
    recovery_stage: Option<String>,
    advanced: bool,
}

/// Audit payload for an applied event. Never carries secret material.
fn event_payload(event: &OrchestrationEvent) -> serde_json::Value {
    match event {
        OrchestrationEvent::TeamPlanPrepared { approval_required } => {
            serde_json::json!({ "approvalRequired": approval_required })
        }
        OrchestrationEvent::ClarificationRequested { interaction_id }
        | OrchestrationEvent::UserClarificationAnswered { interaction_id } => {
            serde_json::json!({ "interactionId": interaction_id })
        }
        OrchestrationEvent::RequiredSubtaskPlanned { issue_id }
        | OrchestrationEvent::ExecutionSubmitted { issue_id }
        | OrchestrationEvent::AcceptancePassed { issue_id }
        | OrchestrationEvent::AcceptanceRejected { issue_id } => {
            serde_json::json!({ "issueId": issue_id })
        }
        OrchestrationEvent::FailureRecorded { reason } => {
            serde_json::json!({ "reason": reason })
        }
        _ => serde_json::json!({}),
    }
}

// ────────────────────────── decision submission ──────────────────────────

/// Result of applying a CEO decision contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionOutcome {
    pub plan_id: Uuid,
    pub revision: i32,
    pub approval_required: bool,
    pub approval_ids: Vec<Uuid>,
    pub subtask_issue_ids: Vec<Uuid>,
    pub question_interaction_ids: Vec<Uuid>,
    pub orchestration: OrchRow,
}

impl TaskOrchestrationService {
    /// Validate and apply a CEO decision contract.
    ///
    /// Steps, in order: structural validation, company scope, agent
    /// executability, plan reporting acyclicity, then (for hires) pending
    /// agents plus hire approvals, then subtask issues, then one transaction
    /// that persists the plan revision, member rows, subtask rows and stage
    /// events. Nothing is dispatched to an executor unless approval was not
    /// required and the plan is complete.
    pub async fn submit_decision(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        author_agent_id: Uuid,
        raw: &serde_json::Value,
    ) -> Result<DecisionOutcome, OrchestrationError> {
        let contract =
            parse_decision_contract(raw).map_err(|error| OrchestrationError::ContractInvalid {
                field: error.field,
                message: error.message,
            })?;

        let orchestration = self.find(company_id, root_issue_id).await?.ok_or(
            OrchestrationError::NotOrchestrated {
                issue_id: root_issue_id,
            },
        )?;
        if author_agent_id != orchestration.ceo_agent_id {
            return Err(OrchestrationError::NotDirectReviewer);
        }
        let stage = orchestration.parsed_stage()?;
        if !matches!(
            stage,
            OrchestrationStage::Assessing
                | OrchestrationStage::TeamPlanning
                | OrchestrationStage::AwaitingTeamApproval
                | OrchestrationStage::Planning
        ) {
            return Err(OrchestrationError::InvalidTransition {
                stage,
                event: "submit_decision".to_string(),
            });
        }
        let existing_subtasks: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .fetch_one(&self.pool)
        .await?;
        if stage == OrchestrationStage::AwaitingTeamApproval {
            return Err(OrchestrationError::InvalidTransition {
                stage,
                event: "submit_decision".to_string(),
            });
        }
        if stage == OrchestrationStage::Planning && existing_subtasks > 0 {
            return Err(OrchestrationError::InvalidTransition {
                stage,
                event: "submit_decision".to_string(),
            });
        }

        self.validate_contract_scope(company_id, orchestration.ceo_agent_id, &contract)
            .await?;

        // Ensure the root issue exists in this company.
        let root_issue: Issue =
            sqlx::query_as("SELECT * FROM issues WHERE id = $1 AND company_id = $2")
                .bind(root_issue_id)
                .bind(company_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(OrchestrationError::CrossCompanyAccess)?;

        let approval_required = contract
            .members
            .iter()
            .any(|member| member.action == "hire")
            && self.company_requires_agent_approval(company_id).await?;
        let has_hire = contract
            .members
            .iter()
            .any(|member| member.action == "hire");
        // A hire that needs approval keeps the plan in `awaiting_team_approval`,
        // where work planning is illegal. Reject the combination up front so the
        // CEO replans after the hire exists instead of failing mid-transaction.
        if has_hire && !contract.subtasks.is_empty() {
            return Err(OrchestrationError::ContractInvalid {
                field: "subtasks".to_string(),
                message: "a plan with hires must be approved before subtasks are planned"
                    .to_string(),
            });
        }

        let revision: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM task_team_plans WHERE orchestration_id = $1",
        )
        .bind(orchestration.id)
        .fetch_one(&self.pool)
        .await?;
        let plan_id = Uuid::new_v4();

        // Hires first: they are inert (`pending_approval`) until a board
        // decision, so creating them cannot dispatch work.
        let mut hired_members: HashMap<usize, (Uuid, Option<Uuid>, String)> = HashMap::new();
        let mut approval_ids: Vec<Uuid> = Vec::new();
        for (index, member) in contract.members.iter().enumerate() {
            if member.action != "hire" {
                continue;
            }
            let Some(hire) = member.hire.as_ref() else {
                continue;
            };
            let agent_service = self.require_agent_service()?.clone();
            let created = agent_service
                .create(CreateAgentInput {
                    company_id,
                    name: hire.name.clone(),
                    role: models::AgentRole::General,
                    status: Some(if approval_required {
                        AgentStatus::PendingApproval
                    } else {
                        AgentStatus::Idle
                    }),
                    adapter_type: hire.adapter_type.clone(),
                    adapter_config: serde_json::json!({}),
                    instructions_bundle: None,
                    runtime_config: None,
                    permissions: None,
                    budget_monthly_cents: hire.budget_monthly_cents,
                    reports_to: member
                        .reports_to_agent_id
                        .or(Some(orchestration.ceo_agent_id)),
                })
                .await
                .map_err(|error: ServiceError| OrchestrationError::Service(error.to_string()))?;

            let mut approval_id = None;
            let hire_status = if approval_required {
                let approval_service = self.require_approval_service()?.clone();
                let approval = approval_service
                    .create(CreateApprovalInput {
                        company_id,
                        approval_type: ApprovalType::HireAgent,
                        requested_by_agent_id: Some(author_agent_id),
                        requested_by_user_id: None,
                        payload: serde_json::json!({
                            "agent_name": created.name,
                            "agent_role": "general",
                            "agent_id": created.id,
                            "name": created.name,
                            "role": "general",
                            "title": hire.title,
                            "reportsTo": member.reports_to_agent_id.or(Some(orchestration.ceo_agent_id)),
                            "adapterType": created.adapter_type,
                            "adapterConfig": created.adapter_config.0,
                            "budgetMonthlyCents": created.budget_monthly_cents,
                            "metadata": {
                                "orchestrationId": orchestration.id,
                                "planId": plan_id,
                                "taskRole": member.role,
                            },
                            "orchestrationId": orchestration.id,
                            "planId": plan_id,
                        }),
                        linked_issue_ids: vec![root_issue_id],
                        validate_payload: true,
                    })
                    .await
                    .map_err(|error: ServiceError| OrchestrationError::Service(error.to_string()))?;
                approval_ids.push(approval.id);
                approval_id = Some(approval.id);
                "pending_approval"
            } else {
                "hired"
            };
            hired_members.insert(index, (created.id, approval_id, hire_status.to_string()));
        }

        // Subtasks: create inert issues first (no assignee), so a later
        // transaction failure cannot leave assignable work behind. Issue
        // creation is idempotent by `originId`.
        let mut issues_by_key: HashMap<String, Uuid> = HashMap::new();
        let mut subtask_issue_ids: Vec<Uuid> = Vec::new();
        for subtask in self.ordered_subtasks(&contract)? {
            let origin_id = format!("orchestration:{}:{}", orchestration.id, subtask.key);
            let existing: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM issues WHERE company_id = $1 AND origin_id = $2",
            )
            .bind(company_id)
            .bind(&origin_id)
            .fetch_optional(&self.pool)
            .await?;
            let issue_id = if let Some(existing) = existing {
                existing
            } else {
                let blocker_ids: Vec<Uuid> = subtask
                    .blocked_by
                    .iter()
                    .filter_map(|key| issues_by_key.get(key).copied())
                    .collect();
                let parent_issue_id = subtask
                    .parent_key
                    .as_ref()
                    .and_then(|key| issues_by_key.get(key).copied())
                    .or(Some(root_issue.id));
                let issue_service = self.require_issue_service()?.clone();
                let created = issue_service
                    .create(CreateIssueInput {
                        id: None,
                        company_id,
                        project_id: root_issue.project_id,
                        project_workspace_id: None,
                        goal_id: root_issue.goal_id,
                        title: subtask.title.clone(),
                        idempotency_key: Some(origin_id.clone()),
                        allow_duplicate: false,
                        description: Some(format!(
                            "{}\n\nAcceptance criteria:\n{}",
                            subtask.description, subtask.acceptance_criteria
                        )),
                        status: Some(if blocker_ids.is_empty() {
                            IssueStatus::Todo
                        } else {
                            IssueStatus::Blocked
                        }),
                        priority: None,
                        parent_id: parent_issue_id,
                        inherit_execution_workspace_from_issue_id: None,
                        assignee_agent_id: None,
                        assignee_user_id: None,
                        work_mode: None,
                        harness_kind: None,
                        responsible_user_id: root_issue.responsible_user_id,
                        origin_kind: Some("manual".to_string()),
                        origin_id: Some(origin_id.clone()),
                        origin_run_id: None,
                        origin_fingerprint: None,
                        request_depth: None,
                        billing_code: None,
                        task_budget_cents: None,
                        task_orchestration_required: false,
                        execution_workspace_id: None,
                        execution_workspace_preference: None,
                        execution_policy: None,
                        execution_workspace_settings: None,
                        created_by_agent_id: Some(author_agent_id),
                        created_by_user_id: None,
                        assignee_adapter_overrides: None,
                        label_ids: Vec::new(),
                        blocked_by_issue_ids: blocker_ids,
                        watchdog: None,
                        watchdog_discovery: None,
                        watchdog_created_by_run_id: None,
                        watchdog_discovery_audit: None,
                    })
                    .await
                    .map_err(|error: String| OrchestrationError::Service(error))?;
                created.issue.id
            };
            issues_by_key.insert(subtask.key.clone(), issue_id);
            subtask_issue_ids.push(issue_id);
        }

        // Clarification interactions (idempotent per orchestration + revision +
        // index). They gate every subsequent dispatch event.
        let mut question_interaction_ids: Vec<Uuid> = Vec::new();
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;

        // Persist the plan header. A revision with no approval gate is approved
        // immediately (the CEO owns the decision); a revision that hires agents
        // stays proposed until the board decides.
        sqlx::query(
            "UPDATE task_team_plans SET status = 'superseded'
              WHERE orchestration_id = $1 AND status IN ('proposed', 'approved')",
        )
        .bind(orchestration.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO task_team_plans
                (id, orchestration_id, company_id, revision, status, created_by_agent_id,
                 decided_at, payload)
             VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $5 = 'approved' THEN NOW() END, $7)",
        )
        .bind(plan_id)
        .bind(orchestration.id)
        .bind(company_id)
        .bind(revision)
        .bind(if approval_required {
            "proposed"
        } else {
            "approved"
        })
        .bind(author_agent_id)
        .bind(serde_json::json!({
            "approvalRequired": approval_required,
            "complexity": contract.complexity,
            "assessment": contract.assessment,
            "rationale": contract.rationale,
            "ceoExecutes": contract.ceo_executes,
            "memberCount": contract.members.len(),
        }))
        .execute(&mut *tx)
        .await?;

        // Persist member decisions.
        for (index, member) in contract.members.iter().enumerate() {
            let hired = hired_members.get(&index);
            let agent_id = member.agent_id.or_else(|| hired.map(|h| h.0));
            let approval_id = hired.and_then(|h| h.1);
            let hire_status = hired.map(|h| h.2.clone());
            let terminated_at = if member.action == "exit" {
                Some(Utc::now())
            } else {
                None
            };
            sqlx::query(
                "INSERT INTO task_team_members
                    (plan_id, company_id, agent_id, action, role, responsibilities, reason,
                     reports_to_agent_id, approval_id, hire_status, hired_agent_id,
                     exited_at, exit_reason)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(plan_id)
            .bind(company_id)
            .bind(agent_id)
            .bind(&member.action)
            .bind(Some(&member.role))
            .bind(Some(&member.responsibilities))
            .bind(Some(&member.reason))
            .bind(
                member
                    .reports_to_agent_id
                    .or(Some(orchestration.ceo_agent_id)),
            )
            .bind(approval_id)
            .bind(hire_status.as_deref())
            .bind(hired.map(|h| h.0))
            .bind(terminated_at)
            .bind(if member.action == "exit" {
                Some(member.reason.as_str())
            } else {
                None
            })
            .execute(&mut *tx)
            .await?;
        }

        // Persist required subtasks and assign owners. Assigning an owner is
        // the dispatch trigger, so it happens only in this transaction and
        // only for subtasks whose blockers are already done.
        for subtask in self.ordered_subtasks(&contract)? {
            let Some(issue_id) = issues_by_key.get(&subtask.key).copied() else {
                continue;
            };
            // The reviewer is the explicit one when given; otherwise the task-
            // internal direct superior: the parent subtask's owner for a nested
            // item, or the owner's plan superior (else the CEO) at the top level.
            let reviewer = subtask
                .reviewer_agent_id
                .or_else(|| {
                    if let Some(parent_key) = subtask.parent_key.as_ref() {
                        contract
                            .subtasks
                            .iter()
                            .find(|candidate| &candidate.key == parent_key)
                            .map(|candidate| candidate.owner_agent_id)
                    } else {
                        contract
                            .members
                            .iter()
                            .find(|member| {
                                member.action == "reuse"
                                    && member.agent_id == Some(subtask.owner_agent_id)
                            })
                            .and_then(|member| member.reports_to_agent_id)
                    }
                })
                .or(Some(orchestration.ceo_agent_id));
            let inserted = sqlx::query(
                "INSERT INTO task_orchestration_subtasks
                    (orchestration_id, company_id, issue_id, owner_agent_id, reviewer_agent_id,
                     acceptance_criteria, required)
                 VALUES ($1, $2, $3, $4, $5, $6, TRUE)
                 ON CONFLICT (orchestration_id, issue_id) DO NOTHING",
            )
            .bind(orchestration.id)
            .bind(company_id)
            .bind(issue_id)
            .bind(subtask.owner_agent_id)
            .bind(reviewer)
            .bind(&subtask.acceptance_criteria)
            .execute(&mut *tx)
            .await?;
            if inserted.rows_affected() == 0 {
                continue;
            }
            sqlx::query(
                "UPDATE issues SET assignee_agent_id = $3, updated_at = NOW()
                  WHERE id = $1 AND company_id = $2",
            )
            .bind(issue_id)
            .bind(company_id)
            .bind(subtask.owner_agent_id)
            .execute(&mut *tx)
            .await?;
        }

        // Create clarification interactions inside the transaction-equivalent
        // service call before emitting the events that read them. The plan
        // revision, member rows and (inert, unassigned) subtask rows are
        // committed first so the interaction service never contends with this
        // transaction's advisory lock.
        tx.commit().await?;
        for (index, question) in contract.questions.iter().enumerate() {
            let interaction_service =
                crate::issue_thread_interaction_service::IssueThreadInteractionService::new(
                    self.pool.clone(),
                );
            let interaction = interaction_service
                .create(
                    &root_issue,
                    models::CreateThreadInteractionInput {
                        kind: "ask_user_questions".to_string(),
                        payload: serde_json::json!({
                            "questions": [{
                                "id": format!("q-{}", index + 1),
                                "prompt": question.question,
                                "required": true,
                                "whyItBlocks": question.why_it_blocks,
                            }]
                        }),
                        title: Some("CEO needs an answer before planning".to_string()),
                        summary: Some(question.question.clone()),
                        continuation_policy: "wake_assignee".to_string(),
                        resolver_policy: None,
                        idempotency_key: Some(format!(
                            "orchestration:{}:revision:{}:question:{}",
                            orchestration.id,
                            revision,
                            index + 1
                        )),
                        addressee_agent_id: None,
                        source_run_id: None,
                        source_comment_id: None,
                    },
                    crate::issue_thread_interaction_service::InteractionCreator {
                        agent_id: Some(author_agent_id),
                        user_id: None,
                    },
                )
                .await
                .map_err(OrchestrationError::Service)?;
            if interaction.status == "pending" {
                question_interaction_ids.push(interaction.id);
            }
        }

        // Single transaction for all stage events.
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let mut row = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;

        if row.parsed_stage()? == OrchestrationStage::Assessing {
            row = Self::apply_event_in_tx(
                &mut tx,
                &row,
                &OrchestrationEvent::AssessmentCompleted,
                Some(author_agent_id),
            )
            .await?;
        }
        row = Self::apply_event_in_tx(
            &mut tx,
            &row,
            &OrchestrationEvent::TeamPlanPrepared { approval_required },
            Some(author_agent_id),
        )
        .await?;

        for interaction_id in &question_interaction_ids {
            row = Self::apply_event_in_tx(
                &mut tx,
                &row,
                &OrchestrationEvent::ClarificationRequested {
                    interaction_id: *interaction_id,
                },
                Some(author_agent_id),
            )
            .await?;
        }

        for subtask in self.ordered_subtasks(&contract)? {
            let Some(issue_id) = issues_by_key.get(&subtask.key).copied() else {
                continue;
            };
            row = Self::apply_event_in_tx(
                &mut tx,
                &row,
                &OrchestrationEvent::RequiredSubtaskPlanned { issue_id },
                Some(author_agent_id),
            )
            .await?;
        }

        // Planning is complete when no clarification remains and no board
        // approval gate is open. Subtask owners are woken below.
        let ready_to_execute = !approval_required
            && contract.questions.is_empty()
            && (!contract.subtasks.is_empty() || contract.ceo_executes);
        if ready_to_execute && row.parsed_stage()? == OrchestrationStage::Planning {
            row = Self::apply_event_in_tx(
                &mut tx,
                &row,
                &OrchestrationEvent::PlanningCompleted,
                Some(author_agent_id),
            )
            .await?;
        }

        // Durable wakes: only subtasks with no open blocker, once per
        // orchestration + subtask.
        if ready_to_execute {
            for subtask in contract.subtasks.iter() {
                let Some(issue_id) = issues_by_key.get(&subtask.key).copied() else {
                    continue;
                };
                let blocked: bool = sqlx::query_scalar(
                    "SELECT EXISTS (
                         SELECT 1 FROM issue_relations relation
                         JOIN issues blocker ON blocker.id = relation.issue_id
                         WHERE relation.company_id = $1
                           AND relation.related_issue_id = $2
                           AND relation.type = 'blocks'
                           AND blocker.status <> 'done')",
                )
                .bind(company_id)
                .bind(issue_id)
                .fetch_one(&mut *tx)
                .await?;
                if blocked {
                    continue;
                }
                Self::enqueue_action_in_tx(
                    &mut tx,
                    orchestration.id,
                    company_id,
                    "wake_assignee",
                    serde_json::json!({
                        "issueId": issue_id,
                        "agentId": subtask.owner_agent_id,
                        "reason": "subtask_assigned",
                    }),
                    Some(&format!("subtask-assigned:{issue_id}")),
                )
                .await?;
            }
        }
        if matches!(
            row.parsed_stage()?,
            OrchestrationStage::AwaitingTeamApproval | OrchestrationStage::Clarifying
        ) {
            Self::enqueue_action_in_tx(
                &mut tx,
                orchestration.id,
                company_id,
                "wake_assignee",
                serde_json::json!({
                    "issueId": root_issue_id,
                    "reason": "orchestration_waiting",
                }),
                Some(&format!("orchestration-waiting:{plan_id}")),
            )
            .await?;
        }

        // Persist the latest decision contract on the orchestration row.
        sqlx::query(
            "UPDATE task_orchestrations
                SET decision_contract = $3, decision_contract_version = $4, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(raw)
        .bind(revision)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let _ = self.drain_actions(orchestration.id).await;

        Ok(DecisionOutcome {
            plan_id,
            revision,
            approval_required,
            approval_ids,
            subtask_issue_ids,
            question_interaction_ids,
            orchestration: row,
        })
    }

    fn ordered_subtasks<'a>(
        &self,
        contract: &'a DecisionContract,
    ) -> Result<Vec<&'a ContractSubtask>, OrchestrationError> {
        let mut by_key: HashMap<&str, &ContractSubtask> = HashMap::new();
        for subtask in &contract.subtasks {
            by_key.insert(subtask.key.as_str(), subtask);
        }
        let mut pending: Vec<&ContractSubtask> = contract.subtasks.iter().collect();
        let mut emitted = BTreeSet::new();
        let mut ordered = Vec::with_capacity(pending.len());
        while !pending.is_empty() {
            let mut progressed = false;
            let mut still_pending = Vec::new();
            for subtask in pending {
                let parent_ready = subtask
                    .parent_key
                    .as_deref()
                    .map(|key| emitted.contains(key))
                    .unwrap_or(true);
                let blockers_ready = subtask
                    .blocked_by
                    .iter()
                    .all(|key| emitted.contains(key.as_str()));
                if parent_ready && blockers_ready {
                    emitted.insert(subtask.key.as_str());
                    ordered.push(subtask);
                    progressed = true;
                } else {
                    still_pending.push(subtask);
                }
            }
            if !progressed {
                return Err(OrchestrationError::DependencyCycleDetected);
            }
            pending = still_pending;
        }
        Ok(ordered)
    }

    /// Validate company scope, agent executability, ownership and the
    /// task-scope reporting graph. The CEO itself must be the author.
    async fn validate_contract_scope(
        &self,
        company_id: Uuid,
        ceo_agent_id: Uuid,
        contract: &DecisionContract,
    ) -> Result<(), OrchestrationError> {
        let mut referenced: BTreeSet<Uuid> = BTreeSet::new();
        referenced.insert(ceo_agent_id);
        for member in &contract.members {
            if let Some(agent) = member.agent_id {
                referenced.insert(agent);
            }
            if let Some(agent) = member.reports_to_agent_id {
                referenced.insert(agent);
            }
        }
        for subtask in &contract.subtasks {
            referenced.insert(subtask.owner_agent_id);
            if let Some(reviewer) = subtask.reviewer_agent_id {
                referenced.insert(reviewer);
            }
        }
        for agent in &referenced {
            let executable: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM agents
                      WHERE id = $1 AND company_id = $2
                        AND status IN ('idle', 'running')
                        AND paused_at IS NULL)",
            )
            .bind(agent)
            .bind(company_id)
            .fetch_one(&self.pool)
            .await?;
            if !executable {
                let known: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1 AND company_id = $2)",
                )
                .bind(agent)
                .bind(company_id)
                .fetch_one(&self.pool)
                .await?;
                return Err(if known {
                    OrchestrationError::AgentNotExecutable(*agent)
                } else {
                    OrchestrationError::AgentNotInCompany(*agent)
                });
            }
        }

        // Each reused member's task-scope superior must be a member of the
        // plan or the CEO; the graph must be acyclic and rooted at the CEO.
        let mut parents: HashMap<Uuid, Uuid> = HashMap::new();
        for member in &contract.members {
            let Some(agent) = member.agent_id else {
                continue;
            };
            if member.action == "exit" {
                continue;
            }
            parents.insert(agent, member.reports_to_agent_id.unwrap_or(ceo_agent_id));
        }
        for (child, parent) in &parents {
            if child == parent {
                return Err(OrchestrationError::ReportingCycleDetected);
            }
            if *parent != ceo_agent_id && !parents.contains_key(parent) {
                return Err(OrchestrationError::ContractInvalid {
                    field: "members".to_string(),
                    message: "reportsToAgentId must be a member of this task team or the CEO"
                        .to_string(),
                });
            }
        }
        for start in parents.keys() {
            let mut seen = BTreeSet::new();
            let mut cursor = Some(*start);
            while let Some(current) = cursor {
                if !seen.insert(current) {
                    return Err(OrchestrationError::ReportingCycleDetected);
                }
                if current == ceo_agent_id {
                    break;
                }
                cursor = parents.get(&current).copied();
            }
        }
        Ok(())
    }

    async fn company_requires_agent_approval(
        &self,
        company_id: Uuid,
    ) -> Result<bool, OrchestrationError> {
        let required: Option<bool> = sqlx::query_scalar(
            "SELECT require_board_approval_for_new_agents FROM companies WHERE id = $1",
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(required.unwrap_or(true))
    }

    /// Board decision on the pending team plan.
    pub async fn decide_team_plan(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        approved: bool,
        note: Option<&str>,
    ) -> Result<OrchRow, OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let current = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        if current.parsed_stage()? != OrchestrationStage::AwaitingTeamApproval {
            return Err(OrchestrationError::InvalidTransition {
                stage: current.parsed_stage()?,
                event: "decide_team_plan".to_string(),
            });
        }

        let plan_id: Option<Uuid> = sqlx::query_scalar(
            "UPDATE task_team_plans
                SET status = $2, decided_at = NOW(), decision_note = $3
              WHERE orchestration_id = $1 AND status = 'proposed'
              RETURNING id",
        )
        .bind(current.id)
        .bind(if approved { "approved" } else { "rejected" })
        .bind(note)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(_) = plan_id else {
            return Err(OrchestrationError::PlanNotFound(current.id));
        };

        let event = if approved {
            OrchestrationEvent::TeamPlanApproved
        } else {
            OrchestrationEvent::TeamPlanRejected
        };
        let row = Self::apply_event_in_tx(&mut tx, &current, &event, None).await?;
        Self::enqueue_action_in_tx(
            &mut tx,
            current.id,
            company_id,
            "wake_assignee",
            serde_json::json!({
                "issueId": root_issue_id,
                "reason": if approved { "team_plan_approved" } else { "team_plan_rejected" },
            }),
            Some(&format!(
                "team-plan-decision:{}:{}",
                current.id, row.stage_version
            )),
        )
        .await?;
        tx.commit().await?;
        let _ = self.drain_actions(current.id).await;
        Ok(row)
    }

    /// Board decision on a hire approval created by a plan revision. Updates
    /// the member row and wakes the CEO so it can replan or continue.
    pub async fn link_hire_decision(
        &self,
        company_id: Uuid,
        approval_id: Uuid,
        approved: bool,
    ) -> Result<(), OrchestrationError> {
        let member: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT plan_id, member.id FROM task_team_members member
               JOIN task_team_plans plan ON plan.id = member.plan_id
              WHERE member.company_id = $1 AND member.approval_id = $2",
        )
        .bind(company_id)
        .bind(approval_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((_plan_id, member_id)) = member else {
            return Ok(());
        };
        sqlx::query(
            "UPDATE task_team_members
                SET hire_status = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(member_id)
        .bind(company_id)
        .bind(if approved { "hired" } else { "rejected" })
        .execute(&self.pool)
        .await?;

        let orchestration: Option<Uuid> = sqlx::query_scalar(
            "SELECT orchestration_id FROM task_team_plans plan
               JOIN task_team_members member ON member.plan_id = plan.id
              WHERE member.id = $1 AND member.company_id = $2",
        )
        .bind(member_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(orchestration_id) = orchestration {
            let root_issue: Option<Uuid> = sqlx::query_scalar(
                "SELECT root_issue_id FROM task_orchestrations WHERE id = $1 AND company_id = $2",
            )
            .bind(orchestration_id)
            .bind(company_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(root_issue) = root_issue {
                self.enqueue_action(
                    orchestration_id,
                    company_id,
                    "wake_assignee",
                    serde_json::json!({
                        "issueId": root_issue,
                        "reason": "hire_decision",
                    }),
                    Some(&format!("hire-decision:{approval_id}")),
                )
                .await?;
                let _ = self.drain_actions(orchestration_id).await;
            }
        }
        Ok(())
    }

    /// The current (latest) team plan with its members.
    pub async fn current_plan(
        &self,
        orchestration_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<TeamPlan>, OrchestrationError> {
        let header: Option<(Uuid, i32, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT id, revision, status, proposed_at
               FROM task_team_plans
              WHERE orchestration_id = $1 AND company_id = $2
              ORDER BY revision DESC
              LIMIT 1",
        )
        .bind(orchestration_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((plan_id, revision, status, proposed_at)) = header else {
            return Ok(None);
        };

        let rows = sqlx::query(
            "SELECT agent_id, action, role, responsibilities, reason,
                    reports_to_agent_id, approval_id, hire_status, hired_agent_id,
                    exited_at, exit_reason
               FROM task_team_members
              WHERE plan_id = $1 AND company_id = $2
              ORDER BY created_at, id",
        )
        .bind(plan_id)
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;

        let members = rows
            .into_iter()
            .map(|row| PlanMember {
                agent_id: row.get("agent_id"),
                action: row.get("action"),
                role: row.get("role"),
                responsibilities: row.get("responsibilities"),
                reason: row.get("reason"),
                reports_to_agent_id: row.get("reports_to_agent_id"),
                approval_id: row.get("approval_id"),
                hire_status: row.get("hire_status"),
                hired_agent_id: row.get("hired_agent_id"),
                exited_at: row.get("exited_at"),
                exit_reason: row.get("exit_reason"),
            })
            .collect();

        Ok(Some(TeamPlan {
            plan_id,
            revision,
            status,
            members,
            proposed_at,
        }))
    }
}

// ───────────────────────────── clarification ─────────────────────────────

impl TaskOrchestrationService {
    /// Adjust task-scope membership after the plan has been approved.
    ///
    /// A team change is a new plan revision that copies the current members and
    /// applies the reuse/exit deltas. Exiting an agent only ends its *task*
    /// membership: `agents` is never touched, and every prior subtask, review,
    /// deliverable and plan revision stays readable. An agent that still owns an
    /// open subtask must hand it off before it can exit.
    pub async fn adjust_team(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        author_agent_id: Uuid,
        adjustments: Vec<TeamAdjustment>,
    ) -> Result<TeamPlan, OrchestrationError> {
        let orchestration = self.find(company_id, root_issue_id).await?.ok_or(
            OrchestrationError::NotOrchestrated {
                issue_id: root_issue_id,
            },
        )?;
        if author_agent_id != orchestration.ceo_agent_id {
            return Err(OrchestrationError::NotDirectReviewer);
        }
        if orchestration.parsed_stage()?.is_terminal() {
            return Err(OrchestrationError::TerminalState);
        }
        if adjustments.is_empty() {
            return Err(OrchestrationError::ContractInvalid {
                field: "members".to_string(),
                message: "at least one member adjustment is required".to_string(),
            });
        }

        let mut seen = BTreeSet::new();
        for adjustment in &adjustments {
            if !seen.insert(adjustment.agent_id) {
                return Err(OrchestrationError::Conflict(
                    "the same agent cannot be adjusted twice".to_string(),
                ));
            }
            if !matches!(adjustment.action.as_str(), "reuse" | "exit") {
                return Err(OrchestrationError::ContractInvalid {
                    field: "action".to_string(),
                    message: "action must be reuse or exit".to_string(),
                });
            }
            let executable: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM agents
                                 WHERE id = $1 AND company_id = $2
                                   AND status IN ('idle','running') AND paused_at IS NULL)",
            )
            .bind(adjustment.agent_id)
            .bind(company_id)
            .fetch_one(&self.pool)
            .await?;
            if !executable {
                return Err(OrchestrationError::AgentNotExecutable(adjustment.agent_id));
            }
        }

        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let current = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;

        let base_plan_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM task_team_plans
              WHERE orchestration_id = $1 AND company_id = $2
              ORDER BY CASE WHEN status = 'approved' THEN 0 ELSE 1 END, revision DESC
              LIMIT 1",
        )
        .bind(current.id)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await?;

        let existing_members = if let Some(plan_id) = base_plan_id {
            sqlx::query(
                "SELECT agent_id, hired_agent_id, action, role, responsibilities, reason,
                        reports_to_agent_id, approval_id, hire_status, exited_at, exit_reason
                   FROM task_team_members
                  WHERE plan_id = $1 AND company_id = $2",
            )
            .bind(plan_id)
            .bind(company_id)
            .fetch_all(&mut *tx)
            .await?
        } else {
            Vec::new()
        };

        // A member cannot exit while an open subtask still names it.
        for adjustment in adjustments
            .iter()
            .filter(|adjustment| adjustment.action == "exit")
        {
            let open_subtasks: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM task_orchestration_subtasks
                  WHERE orchestration_id = $1 AND company_id = $2
                    AND owner_agent_id = $3 AND status <> 'accepted'",
            )
            .bind(current.id)
            .bind(company_id)
            .bind(adjustment.agent_id)
            .fetch_one(&mut *tx)
            .await?;
            if open_subtasks > 0 {
                return Err(OrchestrationError::Conflict(
                    "the member still owns open subtasks and must hand them off before exiting"
                        .to_string(),
                ));
            }
        }

        struct MemberRow {
            agent_id: Option<Uuid>,
            hired_agent_id: Option<Uuid>,
            role: Option<String>,
            responsibilities: Option<String>,
            reason: Option<String>,
            reports_to_agent_id: Option<Uuid>,
            approval_id: Option<Uuid>,
            hire_status: Option<String>,
            exited_at: Option<DateTime<Utc>>,
            exit_reason: Option<String>,
        }

        let mut members: Vec<MemberRow> = existing_members
            .into_iter()
            .map(|row| MemberRow {
                agent_id: row.get("agent_id"),
                hired_agent_id: row.get("hired_agent_id"),
                role: row.get("role"),
                responsibilities: row.get("responsibilities"),
                reason: row.get("reason"),
                reports_to_agent_id: row.get("reports_to_agent_id"),
                approval_id: row.get("approval_id"),
                hire_status: row.get("hire_status"),
                exited_at: row.get("exited_at"),
                exit_reason: row.get("exit_reason"),
            })
            .collect();

        for adjustment in &adjustments {
            let index = members
                .iter()
                .position(|member| member.agent_id == Some(adjustment.agent_id));
            match (adjustment.action.as_str(), index) {
                ("exit", Some(index)) => {
                    let member = &mut members[index];
                    member.exited_at = Some(Utc::now());
                    member.exit_reason = Some(adjustment.reason.clone());
                }
                (_, Some(index)) => {
                    let member = &mut members[index];
                    member.role = Some(adjustment.role.clone());
                    member.responsibilities = Some(adjustment.responsibilities.clone());
                    member.reason = Some(adjustment.reason.clone());
                    member.exited_at = None;
                    member.exit_reason = None;
                    if let Some(reports_to) = adjustment.reports_to_agent_id {
                        member.reports_to_agent_id = Some(reports_to);
                    }
                }
                (_, None) => members.push(MemberRow {
                    agent_id: Some(adjustment.agent_id),
                    hired_agent_id: None,
                    role: Some(adjustment.role.clone()),
                    responsibilities: Some(adjustment.responsibilities.clone()),
                    reason: Some(adjustment.reason.clone()),
                    reports_to_agent_id: Some(
                        adjustment
                            .reports_to_agent_id
                            .unwrap_or(current.ceo_agent_id),
                    ),
                    approval_id: None,
                    hire_status: None,
                    exited_at: None,
                    exit_reason: None,
                }),
            }
        }

        // Validate the adjusted reporting graph: acyclic, rooted at the CEO.
        let mut parents: HashMap<Uuid, Uuid> = HashMap::new();
        for member in members.iter().filter(|member| member.exited_at.is_none()) {
            if let Some(agent) = member.agent_id {
                parents.insert(
                    agent,
                    member.reports_to_agent_id.unwrap_or(current.ceo_agent_id),
                );
            }
        }
        for (child, parent) in &parents {
            if child == parent {
                return Err(OrchestrationError::ReportingCycleDetected);
            }
            if *parent != current.ceo_agent_id && !parents.contains_key(parent) {
                return Err(OrchestrationError::ContractInvalid {
                    field: "members".to_string(),
                    message: "reportsToAgentId must be a member of this task team or the CEO"
                        .to_string(),
                });
            }
        }
        for start in parents.keys() {
            let mut visited = BTreeSet::new();
            let mut cursor = Some(*start);
            while let Some(agent) = cursor {
                if !visited.insert(agent) {
                    return Err(OrchestrationError::ReportingCycleDetected);
                }
                if agent == current.ceo_agent_id {
                    break;
                }
                cursor = parents.get(&agent).copied();
            }
        }

        let revision: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM task_team_plans WHERE orchestration_id = $1",
        )
        .bind(current.id)
        .fetch_one(&mut *tx)
        .await?;
        let plan_id = Uuid::new_v4();

        sqlx::query(
            "UPDATE task_team_plans SET status = 'superseded'
              WHERE orchestration_id = $1 AND status IN ('approved', 'proposed')",
        )
        .bind(current.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO task_team_plans
                (id, orchestration_id, company_id, revision, status, created_by_agent_id,
                 decided_at, decision_note, payload)
             VALUES ($1, $2, $3, $4, 'approved', $5, NOW(), $6, $7)",
        )
        .bind(plan_id)
        .bind(current.id)
        .bind(company_id)
        .bind(revision)
        .bind(author_agent_id)
        .bind("team membership adjusted by the CEO")
        .bind(serde_json::json!({
            "adjustment": adjustments
                .iter()
                .map(|adjustment| serde_json::json!({
                    "agentId": adjustment.agent_id,
                    "action": adjustment.action,
                    "reason": adjustment.reason,
                }))
                .collect::<Vec<_>>(),
            "memberCount": members.len(),
        }))
        .execute(&mut *tx)
        .await?;

        let mut member_list = Vec::with_capacity(members.len());
        for member in &members {
            let action = if member.exited_at.is_some() {
                "exit"
            } else if member.approval_id.is_some() || member.hire_status.is_some() {
                "hire"
            } else {
                "reuse"
            };
            sqlx::query(
                "INSERT INTO task_team_members
                    (plan_id, company_id, agent_id, action, role, responsibilities, reason,
                     reports_to_agent_id, approval_id, hire_status, hired_agent_id,
                     exited_at, exit_reason)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(plan_id)
            .bind(company_id)
            .bind(member.agent_id)
            .bind(action)
            .bind(member.role.as_deref())
            .bind(member.responsibilities.as_deref())
            .bind(member.reason.as_deref())
            .bind(member.reports_to_agent_id)
            .bind(member.approval_id)
            .bind(member.hire_status.as_deref())
            .bind(member.hired_agent_id)
            .bind(member.exited_at)
            .bind(member.exit_reason.as_deref())
            .execute(&mut *tx)
            .await?;
            member_list.push(PlanMember {
                agent_id: member.agent_id,
                action: action.to_string(),
                role: member.role.clone(),
                responsibilities: member.responsibilities.clone(),
                reason: member.reason.clone(),
                reports_to_agent_id: member.reports_to_agent_id,
                approval_id: member.approval_id,
                hire_status: member.hire_status.clone(),
                hired_agent_id: member.hired_agent_id,
                exited_at: member.exited_at,
                exit_reason: member.exit_reason.clone(),
            });
        }

        sqlx::query(
            "INSERT INTO task_orchestration_events
                (orchestration_id, company_id, event_type, actor_agent_id, payload)
             VALUES ($1, $2, 'team_members_adjusted', $3, $4)",
        )
        .bind(current.id)
        .bind(company_id)
        .bind(author_agent_id)
        .bind(serde_json::json!({
            "planId": plan_id,
            "revision": revision,
            "changes": adjustments
                .iter()
                .map(|adjustment| serde_json::json!({
                    "agentId": adjustment.agent_id,
                    "action": adjustment.action,
                }))
                .collect::<Vec<_>>(),
        }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(TeamPlan {
            plan_id,
            revision,
            status: "approved".to_string(),
            members: member_list,
            proposed_at: Utc::now(),
        })
    }

    /// Record a clarification question that is already persisted as an
    /// `ask_user_questions` interaction on the root issue. Idempotent.
    pub async fn record_clarification_request(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        interaction_id: Uuid,
    ) -> Result<(), OrchestrationError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM issue_thread_interactions
              WHERE id = $1 AND company_id = $2 AND issue_id = $3
                AND kind = 'ask_user_questions'",
        )
        .bind(interaction_id)
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_optional(&self.pool)
        .await?;
        match status.as_deref() {
            None => return Err(OrchestrationError::InteractionNotFound(interaction_id)),
            Some("pending") => {}
            Some(_) => {
                return Err(OrchestrationError::Conflict(
                    "interaction is already resolved".to_string(),
                ))
            }
        }
        // Skip when this interaction has already been recorded.
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM task_orchestration_events event
                  JOIN task_orchestrations orchestration
                    ON orchestration.id = event.orchestration_id
                 WHERE orchestration.company_id = $1
                   AND orchestration.root_issue_id = $2
                   AND event.event_type = 'clarification_requested'
                   AND event.payload->>'interactionId' = $3)",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .bind(interaction_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        if recorded {
            return Ok(());
        }
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::ClarificationRequested { interaction_id },
            None,
        )
        .await?;
        Ok(())
    }

    /// Record that the user answered a clarification question.
    ///
    /// Idempotent: a replayed answer (interaction already answered and the
    /// event already written) is a no-op. When the last pending question
    /// resolves, the stage returns to the pre-clarification stage.
    pub async fn record_clarification_answer(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        interaction_id: Uuid,
    ) -> Result<(), OrchestrationError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM issue_thread_interactions
              WHERE id = $1 AND company_id = $2 AND issue_id = $3
                AND kind = 'ask_user_questions'",
        )
        .bind(interaction_id)
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_optional(&self.pool)
        .await?;
        match status.as_deref() {
            None => return Err(OrchestrationError::InteractionNotFound(interaction_id)),
            Some("pending") => {
                return Err(OrchestrationError::Conflict(
                    "interaction is still pending".to_string(),
                ))
            }
            Some(_) => {}
        }
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM task_orchestration_events event
                  JOIN task_orchestrations orchestration
                    ON orchestration.id = event.orchestration_id
                 WHERE orchestration.company_id = $1
                   AND orchestration.root_issue_id = $2
                   AND event.event_type = 'user_clarification_answered'
                   AND event.payload->>'interactionId' = $3)",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .bind(interaction_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        if recorded {
            return Ok(());
        }
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::UserClarificationAnswered { interaction_id },
            None,
        )
        .await?;
        Ok(())
    }
}

// ───────────────────────────── execution ─────────────────────────────

impl TaskOrchestrationService {
    /// Register a required subtask issue that already exists as a descendant of
    /// the root issue. Idempotent; used by the recovery path and tests.
    pub async fn plan_subtask(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        subtask_issue_id: Uuid,
        owner_agent_id: Option<Uuid>,
        reviewer_agent_id: Option<Uuid>,
        acceptance_criteria: Option<&str>,
        required: bool,
    ) -> Result<OrchRow, OrchestrationError> {
        let orchestration = self.find(company_id, root_issue_id).await?.ok_or(
            OrchestrationError::NotOrchestrated {
                issue_id: root_issue_id,
            },
        )?;

        for agent in [owner_agent_id, reviewer_agent_id].into_iter().flatten() {
            let executable: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM agents
                                 WHERE id = $1 AND company_id = $2
                                   AND status IN ('idle','running') AND paused_at IS NULL)",
            )
            .bind(agent)
            .bind(company_id)
            .fetch_one(&self.pool)
            .await?;
            if !executable {
                return Err(OrchestrationError::AgentNotExecutable(agent));
            }
        }
        if owner_agent_id.is_some() && owner_agent_id == reviewer_agent_id {
            return Err(OrchestrationError::SelfReviewBlocked);
        }

        let child = sqlx::query("SELECT id, parent_id, company_id FROM issues WHERE id = $1")
            .bind(subtask_issue_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(OrchestrationError::SubtaskNotFound(subtask_issue_id))?;
        if child.get::<Uuid, _>("company_id") != company_id {
            return Err(OrchestrationError::CrossCompanyAccess);
        }
        let ancestor_root: Option<Uuid> = sqlx::query_scalar(
            "WITH RECURSIVE ancestors(issue_id, parent_id) AS (
                 SELECT id, parent_id FROM issues WHERE id = $1
                 UNION
                 SELECT issue.id, issue.parent_id
                   FROM issues issue
                   JOIN ancestors ON ancestors.parent_id = issue.id
             )
             SELECT issue_id FROM ancestors WHERE issue_id = $2",
        )
        .bind(subtask_issue_id)
        .bind(root_issue_id)
        .fetch_optional(&self.pool)
        .await?;
        if ancestor_root.is_none() {
            return Err(OrchestrationError::SubtaskNotFound(subtask_issue_id));
        }

        let inserted = sqlx::query(
            "INSERT INTO task_orchestration_subtasks
                (orchestration_id, company_id, issue_id, owner_agent_id, reviewer_agent_id,
                 acceptance_criteria, required)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (orchestration_id, issue_id) DO NOTHING",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(subtask_issue_id)
        .bind(owner_agent_id)
        .bind(reviewer_agent_id)
        .bind(acceptance_criteria)
        .bind(required)
        .execute(&self.pool)
        .await?;
        if inserted.rows_affected() == 0 {
            return Ok(orchestration);
        }
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::RequiredSubtaskPlanned {
                issue_id: subtask_issue_id,
            },
            None,
        )
        .await
    }

    /// Record a submitted deliverable for a subtask, bumping its version.
    ///
    /// Only the subtask owner may submit. Submission is blocked while any
    /// required child subtask is not accepted or any `blocks` dependency is
    /// still open. Re-submitting after a rejection creates a new version; an
    /// already accepted subtask cannot be overwritten.
    pub async fn submit_deliverable(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        subtask_issue_id: Uuid,
        submitted_by_agent_id: Uuid,
        body: Option<&str>,
        evidence: serde_json::Value,
    ) -> Result<(Uuid, i32, OrchRow), OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let orchestration = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        if orchestration.parsed_stage()?.is_terminal() {
            return Err(OrchestrationError::TerminalState);
        }

        let subtask = sqlx::query(
            "SELECT id, owner_agent_id, status FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2 AND issue_id = $3",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(subtask_issue_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(OrchestrationError::SubtaskNotFound(subtask_issue_id))?;
        let subtask_row_id: Uuid = subtask.get("id");
        let owner: Option<Uuid> = subtask.get("owner_agent_id");
        let status: String = subtask.get("status");
        if owner != Some(submitted_by_agent_id) {
            return Err(OrchestrationError::NotSubtaskOwner);
        }
        if status == "accepted" {
            return Err(OrchestrationError::Conflict(
                "this subtask is already accepted".to_string(),
            ));
        }

        // Required child subtasks must be accepted before the parent's own
        // deliverable can be reviewed.
        let pending_children: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_orchestration_subtasks child
               JOIN issues child_issue ON child_issue.id = child.issue_id
              WHERE child.orchestration_id = $1 AND child.company_id = $2
                AND child.required AND child.status <> 'accepted'
                AND child_issue.parent_id = $3",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(subtask_issue_id)
        .fetch_one(&mut *tx)
        .await?;
        if pending_children > 0 {
            return Err(OrchestrationError::RequiredSubtasksNotAccepted {
                count: pending_children as usize,
            });
        }

        // Open `blocks` dependencies gate the submission just like they gate
        // the run itself.
        let open_blockers: Vec<Uuid> = sqlx::query_scalar(
            "SELECT blocker.id FROM issue_relations relation
               JOIN issues blocker ON blocker.id = relation.issue_id
              WHERE relation.company_id = $1
                AND relation.related_issue_id = $2
                AND relation.type = 'blocks'
                AND blocker.status <> 'done'",
        )
        .bind(company_id)
        .bind(subtask_issue_id)
        .fetch_all(&mut *tx)
        .await?;
        if !open_blockers.is_empty() {
            return Err(OrchestrationError::DependenciesNotSatisfied {
                issue_ids: open_blockers,
            });
        }

        let version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM task_deliverables WHERE subtask_id = $1",
        )
        .bind(subtask_row_id)
        .fetch_one(&mut *tx)
        .await?;
        let deliverable_id: Uuid = sqlx::query_scalar(
            "INSERT INTO task_deliverables
                (orchestration_id, company_id, kind, subtask_id, version,
                 submitted_by_agent_id, body, evidence)
             VALUES ($1, $2, 'subtask', $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(subtask_row_id)
        .bind(version)
        .bind(submitted_by_agent_id)
        .bind(body)
        .bind(&evidence)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE task_orchestration_subtasks SET status = 'submitted', updated_at = NOW()
              WHERE id = $1",
        )
        .bind(subtask_row_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE issues
                SET status = 'in_review', completed_at = NULL, updated_at = NOW()
              WHERE id = $1 AND company_id = $2 AND status <> 'done'",
        )
        .bind(subtask_issue_id)
        .bind(company_id)
        .execute(&mut *tx)
        .await?;

        let row = Self::apply_event_in_tx(
            &mut tx,
            &orchestration,
            &OrchestrationEvent::ExecutionSubmitted {
                issue_id: subtask_issue_id,
            },
            Some(submitted_by_agent_id),
        )
        .await?;
        tx.commit().await?;
        Ok((deliverable_id, version, row))
    }

    /// Resolve the reviewer for a subtask.
    ///
    /// Precedence: the reviewer recorded on the subtask row; then the owner of
    /// the parent subtask (the task-internal direct superior for a nested
    /// work item); then the owner's task-scope superior from the approved plan;
    /// then the CEO.
    async fn resolve_reviewer(
        &self,
        orchestration_id: Uuid,
        company_id: Uuid,
        ceo_agent_id: Uuid,
        subtask_row_id: Uuid,
        assigned_reviewer: Option<Uuid>,
        owner: Option<Uuid>,
    ) -> Result<Uuid, OrchestrationError> {
        if let Some(reviewer) = assigned_reviewer {
            return Ok(reviewer);
        }
        let parent_owner: Option<Uuid> = sqlx::query_scalar(
            "SELECT parent_subtask.owner_agent_id
               FROM task_orchestration_subtasks child
               JOIN issues child_issue ON child_issue.id = child.issue_id
               JOIN task_orchestration_subtasks parent_subtask
                 ON parent_subtask.orchestration_id = child.orchestration_id
                AND parent_subtask.company_id = child.company_id
                AND parent_subtask.issue_id = child_issue.parent_id
              WHERE child.id = $1 AND child.company_id = $2",
        )
        .bind(subtask_row_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        if let Some(parent_owner) = parent_owner {
            return Ok(parent_owner);
        }
        if let Some(owner) = owner {
            let superior: Option<Uuid> = sqlx::query_scalar(
                "SELECT member.reports_to_agent_id
                   FROM task_team_members member
                   JOIN task_team_plans plan ON plan.id = member.plan_id
                  WHERE plan.orchestration_id = $1 AND plan.company_id = $2
                    AND plan.status = 'approved'
                    AND (member.agent_id = $3 OR member.hired_agent_id = $3)
                  ORDER BY plan.revision DESC
                  LIMIT 1",
            )
            .bind(orchestration_id)
            .bind(company_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await?
            .flatten();
            if let Some(superior) = superior {
                if superior != owner {
                    return Ok(superior);
                }
            }
        }
        Ok(ceo_agent_id)
    }

    /// Record an acceptance verdict for a submitted subtask deliverable.
    ///
    /// Authority is exactly the resolved task-internal direct superior;
    /// cross-company, unrelated and self-review are rejected. A rejection
    /// returns the subtask to `pending`, reopens its issue and queues a durable
    /// wake for the owner. A rejection never overwrites the previous
    /// deliverable version.
    pub async fn review_subtask(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        subtask_issue_id: Uuid,
        reviewer_agent_id: Uuid,
        accepted: bool,
        feedback: Option<&str>,
    ) -> Result<OrchRow, OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let orchestration = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        if orchestration.parsed_stage()?.is_terminal() {
            return Err(OrchestrationError::TerminalState);
        }

        let subtask = sqlx::query(
            "SELECT id, owner_agent_id, reviewer_agent_id, acceptance_criteria, status
               FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2 AND issue_id = $3",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(subtask_issue_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(OrchestrationError::SubtaskNotFound(subtask_issue_id))?;
        let subtask_row_id: Uuid = subtask.get("id");
        let owner: Option<Uuid> = subtask.get("owner_agent_id");
        let assigned_reviewer: Option<Uuid> = subtask.get("reviewer_agent_id");
        let criteria: Option<String> = subtask.get("acceptance_criteria");
        let status: String = subtask.get("status");
        if status != "submitted" {
            return Err(OrchestrationError::Conflict(format!(
                "subtask is in status {status}, not submitted"
            )));
        }
        if owner == Some(reviewer_agent_id) {
            return Err(OrchestrationError::SelfReviewBlocked);
        }
        // The reviewer must be an executable member of the same company before
        // any authority comparison, so a foreign agent id cannot be probed.
        let reviewer_ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM agents
                             WHERE id = $1 AND company_id = $2
                               AND status IN ('idle','running') AND paused_at IS NULL)",
        )
        .bind(reviewer_agent_id)
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await?;
        if !reviewer_ok {
            return Err(OrchestrationError::AgentNotExecutable(reviewer_agent_id));
        }
        drop(tx);

        let expected_reviewer = self
            .resolve_reviewer(
                orchestration.id,
                company_id,
                orchestration.ceo_agent_id,
                subtask_row_id,
                assigned_reviewer,
                owner,
            )
            .await?;
        if reviewer_agent_id != expected_reviewer {
            return Err(OrchestrationError::NotDirectReviewer);
        }

        let deliverable_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM task_deliverables
              WHERE subtask_id = $1 ORDER BY version DESC LIMIT 1",
        )
        .bind(subtask_row_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(deliverable_id) = deliverable_id else {
            return Err(OrchestrationError::SubtaskNotFound(subtask_issue_id));
        };

        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let orchestration = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        let decided = sqlx::query(
            "INSERT INTO task_acceptance_decisions
                (orchestration_id, company_id, deliverable_id, reviewer_agent_id,
                 verdict, criteria, feedback)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (deliverable_id) DO NOTHING",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(deliverable_id)
        .bind(reviewer_agent_id)
        .bind(if accepted { "accepted" } else { "rejected" })
        .bind(criteria.as_deref())
        .bind(feedback)
        .execute(&mut *tx)
        .await?;
        if decided.rows_affected() == 0 {
            return Err(OrchestrationError::Conflict(
                "this deliverable version already has a verdict".to_string(),
            ));
        }

        sqlx::query(
            "UPDATE task_orchestration_subtasks
                SET status = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(subtask_row_id)
        .bind(if accepted { "accepted" } else { "pending" })
        .execute(&mut *tx)
        .await?;

        if accepted {
            sqlx::query(
                "UPDATE issues
                    SET status = 'done', completed_at = NOW(), updated_at = NOW()
                  WHERE id = $1 AND company_id = $2",
            )
            .bind(subtask_issue_id)
            .bind(company_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE issues
                    SET status = 'todo', completed_at = NULL, updated_at = NOW()
                  WHERE id = $1 AND company_id = $2",
            )
            .bind(subtask_issue_id)
            .bind(company_id)
            .execute(&mut *tx)
            .await?;
        }

        let event = if accepted {
            OrchestrationEvent::AcceptancePassed {
                issue_id: subtask_issue_id,
            }
        } else {
            OrchestrationEvent::AcceptanceRejected {
                issue_id: subtask_issue_id,
            }
        };
        let row = Self::apply_event_in_tx(&mut tx, &orchestration, &event, Some(reviewer_agent_id))
            .await?;

        if accepted {
            Self::release_dependents_in_tx(&mut tx, orchestration.id, company_id, subtask_issue_id)
                .await?;
            // The reviewer or the CEO should continue: either review the next
            // submission or write the final report.
            let wake_target = expected_reviewer;
            Self::enqueue_action_in_tx(
                &mut tx,
                orchestration.id,
                company_id,
                "wake_assignee",
                serde_json::json!({
                    "issueId": root_issue_id,
                    "agentId": wake_target,
                    "reason": "subtask_accepted",
                }),
                Some(&format!(
                    "subtask-accepted:{subtask_issue_id}:{deliverable_id}"
                )),
            )
            .await?;
        } else {
            Self::enqueue_action_in_tx(
                &mut tx,
                orchestration.id,
                company_id,
                "reject_deliverable",
                serde_json::json!({
                    "issueId": subtask_issue_id,
                    "agentId": owner,
                    "feedback": feedback,
                    "deliverableId": deliverable_id,
                }),
                Some(&format!("reject:{deliverable_id}")),
            )
            .await?;
        }
        tx.commit().await?;
        let _ = self.drain_actions(orchestration.id).await;
        Ok(row)
    }

    /// Submit the CEO's final summary and complete the orchestration.
    ///
    /// Legal only when all required subtasks are accepted, no clarification
    /// question is open and no `blocks` dependency is unresolved. The report is
    /// persisted as the orchestration's final deliverable; completion is driven
    /// by this row, never by a run's exit status.
    pub async fn record_final_report(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        author_agent_id: Uuid,
        body: &str,
        evidence: serde_json::Value,
    ) -> Result<(Uuid, OrchRow), OrchestrationError> {
        let orchestration = self.find(company_id, root_issue_id).await?.ok_or(
            OrchestrationError::NotOrchestrated {
                issue_id: root_issue_id,
            },
        )?;
        if author_agent_id != orchestration.ceo_agent_id {
            return Err(OrchestrationError::NotDirectReviewer);
        }
        let stage = orchestration.parsed_stage()?;
        if !matches!(
            stage,
            OrchestrationStage::Executing
                | OrchestrationStage::Reviewing
                | OrchestrationStage::Reporting
        ) {
            return Err(OrchestrationError::InvalidTransition {
                stage,
                event: "record_final_report".to_string(),
            });
        }
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2
                AND required AND status <> 'accepted'",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .fetch_one(&self.pool)
        .await?;
        if pending > 0 {
            return Err(OrchestrationError::RequiredSubtasksNotAccepted {
                count: pending as usize,
            });
        }
        let pending_questions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issue_thread_interactions
              WHERE company_id = $1 AND issue_id = $2
                AND kind = 'ask_user_questions' AND status = 'pending'",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_one(&self.pool)
        .await?;
        if pending_questions > 0 {
            return Err(OrchestrationError::UnansweredQuestionsRemain {
                count: pending_questions as usize,
            });
        }
        let open_blockers: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issue_relations relation
               JOIN issues blocker ON blocker.id = relation.issue_id
              WHERE relation.company_id = $1 AND relation.related_issue_id = $2
                AND relation.type = 'blocks' AND blocker.status <> 'done'",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_one(&self.pool)
        .await?;
        if open_blockers > 0 {
            return Err(OrchestrationError::Conflict(
                "the root task still has unresolved blockers".to_string(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let mut row = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;

        let deliverable_id: Uuid = sqlx::query_scalar(
            "INSERT INTO task_deliverables
                (orchestration_id, company_id, kind, version, submitted_by_agent_id, body, evidence)
             VALUES ($1, $2, 'final', 1, $3, $4, $5)
             ON CONFLICT (orchestration_id) WHERE kind = 'final'
             DO UPDATE SET body = EXCLUDED.body, evidence = EXCLUDED.evidence,
                           submitted_by_agent_id = EXCLUDED.submitted_by_agent_id
             RETURNING id",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(author_agent_id)
        .bind(body)
        .bind(&evidence)
        .fetch_one(&mut *tx)
        .await?;

        if row.parsed_stage()? != OrchestrationStage::Reporting {
            row = Self::apply_event_in_tx(
                &mut tx,
                &row,
                &OrchestrationEvent::AcceptancePassed {
                    issue_id: root_issue_id,
                },
                Some(author_agent_id),
            )
            .await?;
        }
        row = Self::apply_event_in_tx(
            &mut tx,
            &row,
            &OrchestrationEvent::FinalReportRecorded,
            Some(author_agent_id),
        )
        .await?;

        sqlx::query(
            "UPDATE task_orchestrations SET final_report = $3, updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(orchestration.id)
        .bind(company_id)
        .bind(body)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE issues
                SET status = 'done', completed_at = COALESCE(completed_at, NOW()), updated_at = NOW()
              WHERE id = $1 AND company_id = $2",
        )
        .bind(root_issue_id)
        .bind(company_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok((deliverable_id, row))
    }

    /// Cancel the orchestration. Cancelling a task team never terminates a
    /// shared company agent; only task-scope membership and open child issues
    /// are closed.
    pub async fn cancel(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<OrchRow, OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, root_issue_id).await?;
        let current = Self::load_for_update(&mut tx, company_id, root_issue_id).await?;
        let row = Self::apply_event_in_tx(
            &mut tx,
            &current,
            &OrchestrationEvent::CancellationRequested,
            None,
        )
        .await?;

        sqlx::query(
            "UPDATE issues
                SET status = 'cancelled', updated_at = NOW()
              WHERE company_id = $1
                AND id IN (SELECT issue_id FROM task_orchestration_subtasks
                            WHERE orchestration_id = $2 AND company_id = $1)
                AND status NOT IN ('done', 'cancelled')",
        )
        .bind(company_id)
        .bind(current.id)
        .execute(&mut *tx)
        .await?;

        // Do not let a queued wake dispatch work for a cancelled task.
        sqlx::query(
            "UPDATE task_orchestration_actions
                SET status = 'skipped', finished_at = NOW(), updated_at = NOW()
              WHERE orchestration_id = $1 AND status IN ('pending', 'running')",
        )
        .bind(current.id)
        .execute(&mut *tx)
        .await?;

        // Ask the heartbeat to stop any adapter run that is still executing for
        // the CEO or an owner. A run cannot be killed mid-commit, so the action
        // is durable and replayed after the transaction.
        let targets = sqlx::query(
            "SELECT issue_id, owner_agent_id FROM task_orchestration_subtasks
              WHERE orchestration_id = $1 AND company_id = $2
                AND owner_agent_id IS NOT NULL AND status <> 'accepted'
              ORDER BY created_at, id
              LIMIT 64",
        )
        .bind(current.id)
        .bind(company_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut cancel_targets = Vec::new();
        cancel_targets.push(serde_json::json!({
            "issueId": root_issue_id,
            "agentId": current.ceo_agent_id,
        }));
        for target in targets {
            let issue_id: Uuid = target.get("issue_id");
            let agent_id: Option<Uuid> = target.get("owner_agent_id");
            if let Some(agent_id) = agent_id {
                cancel_targets.push(serde_json::json!({
                    "issueId": issue_id,
                    "agentId": agent_id,
                }));
            }
        }
        Self::enqueue_action_in_tx(
            &mut tx,
            current.id,
            company_id,
            "cancel",
            serde_json::json!({ "targets": cancel_targets }),
            Some(&format!("cancel-live-runs:{}", current.stage_version + 1)),
        )
        .await?;

        tx.commit().await?;
        let _ = self.drain_actions(current.id).await;
        Ok(row)
    }

    /// Record a terminal failure with its reason.
    pub async fn fail(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        reason: &str,
    ) -> Result<OrchRow, OrchestrationError> {
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::FailureRecorded {
                reason: reason.to_string(),
            },
            None,
        )
        .await
    }

    /// Retry a failed orchestration at its recovery stage.
    pub async fn retry(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<OrchRow, OrchestrationError> {
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::RetryAfterFailure,
            None,
        )
        .await
    }

    /// Reopen a cancelled orchestration at its recovery stage.
    pub async fn resume(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<OrchRow, OrchestrationError> {
        self.apply_event(
            company_id,
            root_issue_id,
            &OrchestrationEvent::ResumeAfterCancellation,
            None,
        )
        .await
    }
}

// ───────────────────────────── outbox ─────────────────────────────

impl TaskOrchestrationService {
    async fn enqueue_action(
        &self,
        orchestration_id: Uuid,
        company_id: Uuid,
        action_type: &str,
        payload: serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<(), OrchestrationError> {
        sqlx::query(
            "INSERT INTO task_orchestration_actions
                (orchestration_id, company_id, action_type, payload, idempotency_key)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (orchestration_id, idempotency_key) DO NOTHING",
        )
        .bind(orchestration_id)
        .bind(company_id)
        .bind(action_type)
        .bind(&payload)
        .bind(idempotency_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn enqueue_action_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        orchestration_id: Uuid,
        company_id: Uuid,
        action_type: &str,
        payload: serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<(), OrchestrationError> {
        sqlx::query(
            "INSERT INTO task_orchestration_actions
                (orchestration_id, company_id, action_type, payload, idempotency_key)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (orchestration_id, idempotency_key) DO NOTHING",
        )
        .bind(orchestration_id)
        .bind(company_id)
        .bind(action_type)
        .bind(&payload)
        .bind(idempotency_key)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Apply every pending action for an orchestration.
    ///
    /// Each row is claimed with `UPDATE ... WHERE status = 'pending'`, so
    /// concurrent workers cannot apply it twice. A crash between claim and
    /// apply leaves the row `running`; [`Self::recover_stale_actions`] returns
    /// it to `pending` after the lease expires.
    pub async fn drain_actions(&self, orchestration_id: Uuid) -> Result<usize, OrchestrationError> {
        let rows = sqlx::query(
            "SELECT id, company_id, action_type, payload
               FROM task_orchestration_actions
              WHERE orchestration_id = $1 AND status = 'pending'
              ORDER BY created_at, id
              LIMIT 32",
        )
        .bind(orchestration_id)
        .fetch_all(&self.pool)
        .await?;

        let mut applied = 0usize;
        for row in rows {
            let action_id: Uuid = row.get("id");
            let company_id: Uuid = row.get("company_id");
            let action_type: String = row.get("action_type");
            let payload: serde_json::Value = row.get("payload");

            let claimed = sqlx::query(
                "UPDATE task_orchestration_actions
                    SET status = 'running', attempts = attempts + 1, updated_at = NOW()
                  WHERE id = $1 AND status = 'pending'",
            )
            .bind(action_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
            if claimed == 0 {
                continue;
            }

            match self
                .apply_action(company_id, orchestration_id, action_id, &action_type, &payload)
                .await
            {
                Ok(()) => {
                    let finished = sqlx::query(
                        "UPDATE task_orchestration_actions
                            SET status = 'applied', finished_at = NOW(), updated_at = NOW()
                          WHERE id = $1 AND status = 'running'",
                    )
                    .bind(action_id)
                    .execute(&self.pool)
                    .await?
                    .rows_affected();
                    if finished > 0 {
                        applied += 1;
                    }
                }
                Err(error) => {
                    let initial_wake_key = payload
                        .get("idempotencyKey")
                        .and_then(serde_json::Value::as_str);
                    let issue_id = payload.get("issueId").and_then(serde_json::Value::as_str);
                    let expected_initial_key =
                        issue_id.map(|issue_id| format!("task-orchestration-initial:{issue_id}"));
                    let is_initial_wake = action_type == "wake_assignee"
                        && initial_wake_key == expected_initial_key.as_deref();
                    let deferred_for_ceo = is_initial_wake
                        && matches!(
                            &error,
                            OrchestrationError::AgentNotExecutable(_)
                                | OrchestrationError::AgentNotInCompany(_)
                        );
                    if deferred_for_ceo {
                        tracing::debug!(
                            %action_id,
                            error = %error,
                            "initial task orchestration wake deferred until its CEO is runnable"
                        );
                        sqlx::query(
                            "UPDATE task_orchestration_actions
                                SET status = 'pending',
                                    attempts = GREATEST(attempts - 1, 0),
                                    last_error = $2, updated_at = NOW()
                              WHERE id = $1 AND status = 'running'",
                        )
                        .bind(action_id)
                        .bind(error.to_string())
                        .execute(&self.pool)
                        .await?;
                    } else {
                        tracing::warn!(
                            %action_id,
                            action_type = %action_type,
                            error = %error,
                            "task orchestration action failed"
                        );
                        sqlx::query(
                            "UPDATE task_orchestration_actions
                                SET status = CASE WHEN attempts >= 5 THEN 'failed' ELSE 'pending' END,
                                    last_error = $2, updated_at = NOW()
                              WHERE id = $1 AND status = 'running'",
                        )
                        .bind(action_id)
                        .bind(error.to_string())
                        .execute(&self.pool)
                        .await?;
                    }
                }
            }
        }
        Ok(applied)
    }

    /// Dispatch the recovered root wake while holding the same root lock used
    /// by cancel. If cancellation wins, the pending action becomes a no-op; if
    /// this wake wins, cancel runs after the durable heartbeat request exists
    /// and can stop the resulting run.
    async fn wake_initial_action_with_root_lock(
        &self,
        company_id: Uuid,
        orchestration_id: Uuid,
        issue_id: Uuid,
        agent_id: Uuid,
        idempotency_key: &str,
        options: HeartbeatWakeupOptions,
    ) -> Result<(), OrchestrationError> {
        let mut tx = self.pool.begin().await?;
        Self::lock_issue(&mut tx, issue_id).await?;
        let task_active: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1
                   FROM task_orchestrations orchestration
                   JOIN companies company ON company.id = orchestration.company_id
                   JOIN issues issue
                     ON issue.id = orchestration.root_issue_id
                    AND issue.company_id = orchestration.company_id
                  WHERE orchestration.id = $1
                    AND orchestration.company_id = $2
                    AND orchestration.root_issue_id = $3
                    AND orchestration.stage NOT IN ('completed', 'failed', 'cancelled')
                    AND company.status::text = 'active'
                    AND issue.task_orchestration_required = TRUE
                    AND issue.parent_id IS NULL
                    AND issue.status::text IN ('todo', 'in_progress', 'in_review')
             )",
        )
        .bind(orchestration_id)
        .bind(company_id)
        .bind(issue_id)
        .fetch_one(&mut *tx)
        .await?;
        if !task_active {
            tx.commit().await?;
            return Ok(());
        }

        let ceo_runnable: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM agents ceo
                  JOIN companies company ON company.id = ceo.company_id
                 WHERE ceo.id = $1 AND ceo.company_id = $2
                   AND company.status::text = 'active'
                   AND ceo.role = 'ceo'
                   AND ceo.status::text IN ('idle', 'running')
                   AND ceo.paused_at IS NULL
             )",
        )
        .bind(agent_id)
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await?;
        if !ceo_runnable {
            return Err(OrchestrationError::AgentNotExecutable(agent_id));
        }

        let has_run: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM heartbeat_runs run
                  WHERE run.company_id = $1
                    AND (run.context_snapshot->>'issueId' = $2::text
                         OR run.context_snapshot->>'taskId' = $2::text)
             )",
        )
        .bind(company_id)
        .bind(issue_id)
        .fetch_one(&mut *tx)
        .await?;
        let has_other_live_wake: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM agent_wakeup_requests wake
                  WHERE wake.company_id = $1 AND wake.agent_id = $2
                    AND wake.status::text IN ('queued', 'dispatched', 'running')
                    AND wake.payload->>'issueId' = $3::text
                    AND wake.idempotency_key IS DISTINCT FROM $4
             )",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(issue_id)
        .bind(idempotency_key)
        .fetch_one(&mut *tx)
        .await?;
        if has_run || has_other_live_wake {
            tx.commit().await?;
            return Ok(());
        }

        self.heartbeat
            .wakeup_with_options(agent_id, issue_id, company_id, options)
            .await
            .map_err(|error| OrchestrationError::Storage(error.to_string()))?;
        tx.commit().await?;
        Ok(())
    }

    async fn apply_action(
        &self,
        company_id: Uuid,
        orchestration_id: Uuid,
        action_id: Uuid,
        action_type: &str,
        payload: &serde_json::Value,
    ) -> Result<(), OrchestrationError> {
        let issue_id = payload
            .get("issueId")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        let agent_id = payload
            .get("agentId")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        match action_type {
            "wake_assignee" => {
                let (Some(issue_id), Some(agent_id)) = (issue_id, agent_id) else {
                    return Err(OrchestrationError::Storage(
                        "wake action is missing issueId/agentId".to_string(),
                    ));
                };
                let reason = payload
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("task_orchestration");
                let default_idempotency_key = format!(
                    "task-orchestration:{issue_id}:{reason}:{}",
                    payload
                        .get("deliverableId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("wake")
                );
                let task_budget_resume_reference = match reason {
                    "subtask_assigned" => Some(
                        TaskBudgetResumeReference::SubtaskAssigned { action_id },
                    ),
                    "subtask_accepted" => Some(
                        TaskBudgetResumeReference::SubtaskAccepted { action_id },
                    ),
                    _ => None,
                };
                // The action row is the stable source event. Include its UUID
                // in the Heartbeat key so distinct accepted deliverables (and
                // their potentially different reviewers) never share a wake.
                let idempotency_key = task_budget_resume_reference
                    .map(|_| format!("task-orchestration-action:{action_id}"))
                    .or_else(|| {
                        payload
                            .get("idempotencyKey")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or(default_idempotency_key);
                let source = payload
                    .get("wakeupSource")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("automation");
                let trigger_detail = payload
                    .get("triggerDetail")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("task_orchestration");
                let context_snapshot = match payload
                    .get("contextSource")
                    .and_then(serde_json::Value::as_str)
                {
                    Some(context_source) => serde_json::json!({
                        "issueId": issue_id,
                        "source": context_source,
                    }),
                    None => serde_json::json!({
                        "issueId": issue_id,
                        "taskId": issue_id,
                        "wakeReason": reason,
                        "source": "task_orchestration",
                    }),
                };
                let wakeup_payload = payload
                    .get("wakeupPayload")
                    .cloned()
                    .unwrap_or_else(|| payload.clone());
                let is_initial_wake =
                    idempotency_key == format!("task-orchestration-initial:{issue_id}");
                let options = HeartbeatWakeupOptions {
                    source: Some(source.to_string()),
                    trigger_detail: Some(trigger_detail.to_string()),
                    reason: Some(reason.to_string()),
                    requested_by_actor_type: Some("system".to_string()),
                    requested_by_actor_id: None,
                    idempotency_key: Some(idempotency_key.clone()),
                    payload: Some(wakeup_payload),
                    context_snapshot: Some(context_snapshot),
                    task_budget_resume_reference,
                    ..Default::default()
                };
                if is_initial_wake {
                    self.wake_initial_action_with_root_lock(
                        company_id,
                        orchestration_id,
                        issue_id,
                        agent_id,
                        &idempotency_key,
                        options,
                    )
                    .await?;
                } else {
                    self.heartbeat
                        .wakeup_with_options(agent_id, issue_id, company_id, options)
                        .await
                        .map_err(|error: HeartbeatError| {
                            OrchestrationError::Storage(error.to_string())
                        })?;
                }
                Ok(())
            }
            "reject_deliverable" => {
                let Some(issue_id) = issue_id else {
                    return Err(OrchestrationError::Storage(
                        "reject action is missing issueId".to_string(),
                    ));
                };
                let deliverable_id = payload
                    .get("deliverableId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                sqlx::query(
                    "UPDATE issues SET status = 'todo', completed_at = NULL, updated_at = NOW()
                      WHERE id = $1 AND company_id = $2 AND status = 'in_review'",
                )
                .bind(issue_id)
                .bind(company_id)
                .execute(&self.pool)
                .await?;
                if let Some(agent_id) = agent_id {
                    self.heartbeat
                        .wakeup_with_options(
                            agent_id,
                            issue_id,
                            company_id,
                            HeartbeatWakeupOptions {
                                source: Some("automation".to_string()),
                                trigger_detail: Some("task_orchestration".to_string()),
                                reason: Some("deliverable_rejected".to_string()),
                                requested_by_actor_type: Some("system".to_string()),
                                requested_by_actor_id: None,
                                idempotency_key: Some(format!(
                                    "reject-deliverable:{deliverable_id}"
                                )),
                                payload: Some(payload.clone()),
                                context_snapshot: Some(serde_json::json!({
                                    "issueId": issue_id,
                                    "taskId": issue_id,
                                    "wakeReason": "deliverable_rejected",
                                    "source": "task_orchestration",
                                    "feedback": payload.get("feedback"),
                                })),
                                ..Default::default()
                            },
                        )
                        .await
                        .map_err(|error: HeartbeatError| {
                            OrchestrationError::Storage(error.to_string())
                        })?;
                }
                Ok(())
            }
            "cancel" => {
                let targets = payload
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for target in targets {
                    let issue = target
                        .get("issueId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok());
                    let agent = target
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| Uuid::parse_str(value).ok());
                    let (Some(issue), Some(agent)) = (issue, agent) else {
                        continue;
                    };
                    // A cancelled task may have no live run for an agent; that is
                    // not an error.
                    if let Err(error) = self
                        .heartbeat
                        .cancel_run(agent, issue, company_id, "task_cancelled")
                        .await
                    {
                        tracing::debug!(
                            %issue,
                            %agent,
                            error = %error,
                            "no live run to cancel for a cancelled task"
                        );
                    }
                }
                Ok(())
            }
            other => Err(OrchestrationError::Storage(format!(
                "unsupported orchestration action {other}"
            ))),
        }
    }

    /// Return actions stuck in `running` (their worker crashed) to `pending`.
    pub async fn recover_stale_actions(
        &self,
        stale_after_seconds: i64,
    ) -> Result<u64, OrchestrationError> {
        let affected = sqlx::query(
            "UPDATE task_orchestration_actions
                SET status = 'pending', updated_at = NOW()
              WHERE status = 'running'
                AND updated_at < NOW() - ($1 * INTERVAL '1 second')",
        )
        .bind(stale_after_seconds)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(affected)
    }

    /// Recover Board-created roots that were durably marked for orchestration
    /// but whose request process exited before completing the start/wake path.
    ///
    /// The persisted marker is the authorization boundary: ordinary issues
    /// assigned to a CEO are never converted into orchestrated tasks here.
    /// The initial wake is placed in the existing durable outbox with the same
    /// idempotency key as the synchronous issue-create path. This covers both
    /// crashes before `start` commits and after it commits but before the wake
    /// request is enqueued. The caller should drain the outbox after this scan.
    pub async fn recover_required_task_orchestrations(
        &self,
        limit: usize,
    ) -> Result<usize, OrchestrationError> {
        if limit == 0 {
            return Ok(0);
        }

        let candidates = sqlx::query_as::<_, RequiredOrchestrationRecoveryCandidate>(
            r#"
            SELECT issue.id AS issue_id,
                   issue.company_id,
                   COALESCE(orchestration.ceo_agent_id, issue.assignee_agent_id) AS ceo_agent_id,
                   orchestration.id AS orchestration_id
              FROM issues issue
              JOIN companies company ON company.id = issue.company_id
              LEFT JOIN task_orchestrations orchestration
                ON orchestration.company_id = issue.company_id
               AND orchestration.root_issue_id = issue.id
              JOIN agents ceo
                ON ceo.id = COALESCE(orchestration.ceo_agent_id, issue.assignee_agent_id)
               AND ceo.company_id = issue.company_id
             WHERE issue.task_orchestration_required = TRUE
               AND issue.parent_id IS NULL
               AND issue.status::text IN ('todo', 'in_progress', 'in_review')
               -- Leave time for the synchronous create handler to commit its
               -- orchestration and initial wake before recovery competes.
               AND issue.created_at < NOW() - INTERVAL '30 seconds'
               AND (orchestration.id IS NULL
                    OR orchestration.stage NOT IN ('completed', 'failed', 'cancelled'))
               AND company.status::text = 'active'
               AND ceo.role = 'ceo'
               AND ceo.status::text IN ('idle', 'running')
               AND ceo.paused_at IS NULL
               -- Any run means the issue has already been attempted. Recovery
               -- must not turn a terminal task back into a new CEO invocation.
               AND NOT EXISTS (
                   SELECT 1 FROM heartbeat_runs run
                    WHERE run.company_id = issue.company_id
                      AND (run.context_snapshot->>'issueId' = issue.id::text
                           OR run.context_snapshot->>'taskId' = issue.id::text)
               )
               -- An existing outbox row already owns this recovery attempt.
               AND NOT EXISTS (
                   SELECT 1 FROM task_orchestration_actions action
                    WHERE action.orchestration_id = orchestration.id
                      AND action.idempotency_key =
                          'task-orchestration-initial:' || issue.id::text
               )
               -- Reuse/retry only a wakeup that has not dispatched a run. A
               -- fresh queued claim may still belong to a live request; an old
               -- queued claim is recoverable after the caller crashed.
               AND NOT EXISTS (
                   SELECT 1 FROM agent_wakeup_requests wake
                    WHERE wake.company_id = issue.company_id
                      AND wake.idempotency_key =
                          'task-orchestration-initial:' || issue.id::text
                      AND (
                          wake.status::text IN ('dispatched', 'running', 'completed')
                          OR (wake.status::text = 'queued'
                              AND (wake.run_id IS NOT NULL
                                   OR wake.updated_at >= NOW() - INTERVAL '60 seconds'))
                          OR (wake.status::text = 'skipped'
                              AND COALESCE(wake.reason, '') <> 'task_orchestration_start_pending')
                      )
               )
               -- Do not compete with any other live issue wake, even if it was
               -- requested through a different idempotency key.
               AND NOT EXISTS (
                   SELECT 1 FROM agent_wakeup_requests live_wake
                    WHERE live_wake.company_id = issue.company_id
                      AND live_wake.agent_id = ceo.id
                      AND live_wake.status::text IN ('queued', 'dispatched', 'running')
                      AND live_wake.payload->>'issueId' = issue.id::text
                      AND (
                          live_wake.idempotency_key IS DISTINCT FROM
                              ('task-orchestration-initial:' || issue.id::text)
                          OR live_wake.status::text <> 'queued'
                          OR live_wake.run_id IS NOT NULL
                          OR live_wake.updated_at >= NOW() - INTERVAL '60 seconds'
                      )
               )
             ORDER BY issue.created_at, issue.id
             LIMIT $1
            "#,
        )
        .bind(limit.min(256) as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut recovered = 0usize;
        for candidate in candidates {
            if candidate.orchestration_id.is_none() {
                match self
                    .start(
                        candidate.company_id,
                        candidate.issue_id,
                        candidate.ceo_agent_id,
                    )
                    .await
                {
                    Ok(_) => {}
                    // The CEO can be paused/terminated between selection and
                    // start. Keep the marker and retry after it is runnable.
                    Err(OrchestrationError::AgentNotInCompany(_)) => continue,
                    Err(error) => return Err(error),
                }
            }

            // Serialize the final state check and outbox insert with cancel,
            // retry and other root-owned transitions. The initial scan above
            // is only a bounded candidate list; it is not authorization to
            // wake a task whose state changed while the worker was starting it.
            let mut tx = self.pool.begin().await?;
            Self::lock_issue(&mut tx, candidate.issue_id).await?;
            let issue_state: Option<(bool, Option<Uuid>, String)> = sqlx::query_as(
                "SELECT task_orchestration_required, parent_id, status::text
                   FROM issues
                  WHERE id = $1 AND company_id = $2
                  FOR UPDATE",
            )
            .bind(candidate.issue_id)
            .bind(candidate.company_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some((required, parent_id, issue_status)) = issue_state else {
                tx.commit().await?;
                continue;
            };
            if !required
                || parent_id.is_some()
                || !matches!(issue_status.as_str(), "todo" | "in_progress" | "in_review")
            {
                tx.commit().await?;
                continue;
            }

            let orchestration = sqlx::query_as::<_, OrchRow>(&format!(
                "SELECT {ORCH_COLUMNS} FROM task_orchestrations \
                 WHERE company_id = $1 AND root_issue_id = $2 FOR UPDATE"
            ))
            .bind(candidate.company_id)
            .bind(candidate.issue_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(orchestration) = orchestration else {
                tx.commit().await?;
                continue;
            };
            if orchestration.parsed_stage()?.is_terminal() {
                tx.commit().await?;
                continue;
            }

            // Recheck immediately before queuing; a CEO that stopped being
            // runnable during this sweep must not receive a wake.
            let runnable: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM companies company
                     JOIN agents agent ON agent.company_id = company.id
                    WHERE company.id = $1 AND company.status::text = 'active'
                      AND agent.id = $2 AND agent.role = 'ceo'
                      AND agent.status::text IN ('idle', 'running')
                      AND agent.paused_at IS NULL
                 )",
            )
            .bind(candidate.company_id)
            .bind(orchestration.ceo_agent_id)
            .fetch_one(&mut *tx)
            .await?;
            if !runnable {
                tx.commit().await?;
                continue;
            }

            let idempotency_key = format!("task-orchestration-initial:{}", candidate.issue_id);
            let has_run: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM heartbeat_runs run
                      WHERE run.company_id = $1
                        AND (run.context_snapshot->>'issueId' = $2::text
                             OR run.context_snapshot->>'taskId' = $2::text)
                 )",
            )
            .bind(candidate.company_id)
            .bind(candidate.issue_id)
            .fetch_one(&mut *tx)
            .await?;
            let has_blocking_initial_wake: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM agent_wakeup_requests wake
                      WHERE wake.company_id = $1
                        AND wake.idempotency_key = $2
                        AND (
                            wake.status::text IN ('dispatched', 'running', 'completed')
                            OR (wake.status::text = 'queued'
                                AND (wake.run_id IS NOT NULL
                                     OR wake.updated_at >= NOW() - INTERVAL '60 seconds'))
                            OR (wake.status::text = 'skipped'
                                AND COALESCE(wake.reason, '') <> 'task_orchestration_start_pending')
                        )
                 )",
            )
            .bind(candidate.company_id)
            .bind(&idempotency_key)
            .fetch_one(&mut *tx)
            .await?;
            let has_other_live_wake: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM agent_wakeup_requests wake
                      WHERE wake.company_id = $1 AND wake.agent_id = $2
                        AND wake.status::text IN ('queued', 'dispatched', 'running')
                        AND wake.payload->>'issueId' = $3::text
                        AND wake.idempotency_key IS DISTINCT FROM $4
                 )",
            )
            .bind(candidate.company_id)
            .bind(orchestration.ceo_agent_id)
            .bind(candidate.issue_id)
            .bind(&idempotency_key)
            .fetch_one(&mut *tx)
            .await?;
            let has_initial_action: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1 FROM task_orchestration_actions action
                      WHERE action.orchestration_id = $1
                        AND action.idempotency_key = $2
                 )",
            )
            .bind(orchestration.id)
            .bind(&idempotency_key)
            .fetch_one(&mut *tx)
            .await?;
            if has_run || has_blocking_initial_wake || has_other_live_wake || has_initial_action {
                tx.commit().await?;
                continue;
            }

            let payload = serde_json::json!({
                "issueId": candidate.issue_id,
                "agentId": orchestration.ceo_agent_id,
                "reason": "issue_assigned",
                "idempotencyKey": idempotency_key.clone(),
                "wakeupSource": "assignment",
                "triggerDetail": "system",
                "contextSource": "issue.create.recovery",
                "wakeupPayload": {
                    "issueId": candidate.issue_id,
                    "mutation": "create_recovery",
                },
            });
            Self::enqueue_action_in_tx(
                &mut tx,
                orchestration.id,
                candidate.company_id,
                "wake_assignee",
                payload,
                Some(&idempotency_key),
            )
            .await?;
            tx.commit().await?;
            recovered += 1;
        }
        Ok(recovered)
    }

    /// Recovery sweep: release stale claims, then drain pending actions for
    /// every orchestration that still has work queued. Returns the number of
    /// applied actions.
    pub async fn recover_pending_work(&self, limit: usize) -> Result<usize, OrchestrationError> {
        self.recover_stale_actions(60).await?;
        let orchestration_ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT orchestration_id FROM task_orchestration_actions
              WHERE status = 'pending'
              ORDER BY orchestration_id
              LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        let mut applied = 0usize;
        for orchestration_id in orchestration_ids {
            applied += self.drain_actions(orchestration_id).await?;
        }
        Ok(applied)
    }

    /// Wake the CEO idempotently per `(issue, reason, scope)`.
    pub async fn wake_ceo(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        reason: &str,
        idempotency_scope: &str,
    ) -> Result<(), OrchestrationError> {
        let Some(row) = self.find(company_id, root_issue_id).await? else {
            return Err(OrchestrationError::NotOrchestrated {
                issue_id: root_issue_id,
            });
        };
        self.heartbeat
            .wakeup_with_options(
                row.ceo_agent_id,
                root_issue_id,
                company_id,
                HeartbeatWakeupOptions {
                    source: Some("automation".to_string()),
                    trigger_detail: Some("system".to_string()),
                    reason: Some(reason.to_string()),
                    requested_by_actor_type: Some("system".to_string()),
                    requested_by_actor_id: None,
                    idempotency_key: Some(format!(
                        "task-orchestration:{root_issue_id}:{reason}:{idempotency_scope}"
                    )),
                    payload: Some(serde_json::json!({
                        "issueId": root_issue_id,
                        "orchestrationId": row.id,
                        "mutation": "task_orchestration",
                        "stage": row.stage,
                    })),
                    context_snapshot: Some(serde_json::json!({
                        "issueId": root_issue_id,
                        "taskId": root_issue_id,
                        "orchestrationId": row.id,
                        "wakeReason": reason,
                        "source": "task_orchestration",
                        "stage": row.stage,
                    })),
                    ..Default::default()
                },
            )
            .await
            .map_err(|error| OrchestrationError::Storage(error.to_string()))
    }

    /// Wake a subtask owner once its blockers resolved.
    async fn release_dependents_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        orchestration_id: Uuid,
        company_id: Uuid,
        blocker_issue_id: Uuid,
    ) -> Result<(), OrchestrationError> {
        let ready = sqlx::query(
            "SELECT DISTINCT dependent.id, dependent.assignee_agent_id
               FROM issue_relations relation
               JOIN issues dependent ON dependent.id = relation.related_issue_id
              WHERE relation.company_id = $1
                AND relation.issue_id = $2
                AND relation.type = 'blocks'
                AND dependent.status = 'blocked'
                AND dependent.assignee_agent_id IS NOT NULL
                AND NOT EXISTS (
                    SELECT 1 FROM issue_relations remaining
                      JOIN issues unresolved ON unresolved.id = remaining.issue_id
                     WHERE remaining.company_id = $1
                       AND remaining.related_issue_id = dependent.id
                       AND remaining.type = 'blocks'
                       AND unresolved.status <> 'done')",
        )
        .bind(company_id)
        .bind(blocker_issue_id)
        .fetch_all(&mut **tx)
        .await?;
        for row in ready {
            let dependent_id: Uuid = row.get("id");
            let Some(assignee) = row.get::<Option<Uuid>, _>("assignee_agent_id") else {
                continue;
            };
            sqlx::query(
                "UPDATE issues SET status = 'todo', updated_at = NOW()
                  WHERE id = $1 AND company_id = $2 AND status = 'blocked'",
            )
            .bind(dependent_id)
            .bind(company_id)
            .execute(&mut **tx)
            .await?;
            Self::enqueue_action_in_tx(
                tx,
                orchestration_id,
                company_id,
                "wake_assignee",
                serde_json::json!({
                    "issueId": dependent_id,
                    "agentId": assignee,
                    "reason": "blockers_resolved",
                }),
                Some(&format!(
                    "blockers-resolved:{dependent_id}:{blocker_issue_id}"
                )),
            )
            .await?;
        }
        Ok(())
    }
}

// ───────────────────────────── read model ─────────────────────────────

impl TaskOrchestrationService {
    /// Full read model for the task-detail UI.
    pub async fn view(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
    ) -> Result<Option<OrchestrationView>, OrchestrationError> {
        let Some(row) = self.find(company_id, root_issue_id).await? else {
            return Ok(None);
        };
        let budget_root = task_budget_root(&self.pool, company_id, root_issue_id)
            .await?
            .ok_or(OrchestrationError::CrossCompanyAccess)?;
        if budget_root.root_issue_id != root_issue_id {
            return Err(OrchestrationError::CrossCompanyAccess);
        }
        let task_spent_cents = task_tree_spend_cents(&self.pool, company_id, root_issue_id).await?;
        let plan = self.current_plan(row.id, company_id).await?;

        let clarifications = sqlx::query(
            "SELECT id, COALESCE(title, summary, 'Clarification requested') AS prompt, created_at
               FROM issue_thread_interactions
              WHERE company_id = $1 AND issue_id = $2
                AND kind = 'ask_user_questions' AND status = 'pending'
              ORDER BY created_at",
        )
        .bind(company_id)
        .bind(root_issue_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| PendingClarification {
            interaction_id: row.get("id"),
            prompt: row.get("prompt"),
            asked_at: row.get("created_at"),
        })
        .collect();

        let subtasks = sqlx::query(
            "SELECT subtask.issue_id, issue.parent_id AS parent_issue_id,
                    subtask.owner_agent_id, subtask.reviewer_agent_id,
                    subtask.acceptance_criteria, subtask.status, subtask.required
               FROM task_orchestration_subtasks subtask
               JOIN issues issue ON issue.id = subtask.issue_id
              WHERE subtask.orchestration_id = $1 AND subtask.company_id = $2
              ORDER BY subtask.created_at, subtask.id",
        )
        .bind(row.id)
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| OrchestrationSubtask {
            issue_id: row.get("issue_id"),
            parent_issue_id: row.get("parent_issue_id"),
            owner_agent_id: row.get("owner_agent_id"),
            reviewer_agent_id: row.get("reviewer_agent_id"),
            acceptance_criteria: row.get("acceptance_criteria"),
            status: row.get("status"),
            required: row.get("required"),
        })
        .collect();

        let deliverables = sqlx::query(
            "SELECT deliverable.id, deliverable.subtask_id, deliverable.version,
                    deliverable.submitted_by_agent_id, deliverable.body, deliverable.evidence,
                    deliverable.created_at,
                    subtask.issue_id AS subtask_issue_id,
                    decision.verdict, decision.feedback, decision.reviewer_agent_id
               FROM task_deliverables deliverable
               LEFT JOIN task_orchestration_subtasks subtask ON subtask.id = deliverable.subtask_id
               LEFT JOIN task_acceptance_decisions decision ON decision.deliverable_id = deliverable.id
              WHERE deliverable.orchestration_id = $1 AND deliverable.company_id = $2
              ORDER BY deliverable.created_at, deliverable.id",
        )
        .bind(row.id)
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| DeliverableView {
            deliverable_id: row.get("id"),
            subtask_issue_id: row.get("subtask_issue_id"),
            version: row.get("version"),
            submitted_by_agent_id: row.get("submitted_by_agent_id"),
            body: row.get("body"),
            evidence: row.get("evidence"),
            created_at: row.get("created_at"),
            verdict: row.get("verdict"),
            feedback: row.get("feedback"),
            reviewer_agent_id: row.get("reviewer_agent_id"),
        })
        .collect();

        Ok(Some(OrchestrationView {
            orchestration_id: row.id,
            root_issue_id: row.root_issue_id,
            ceo_agent_id: row.ceo_agent_id,
            stage: row.parsed_stage()?,
            stage_version: row.stage_version,
            decision_contract_version: row.decision_contract_version,
            plan,
            pending_clarifications: clarifications,
            subtasks,
            deliverables,
            final_report: row.final_report,
            error_reason: row.error_reason,
            created_at: row.created_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
            task_budget_cents: budget_root.budget_cents,
            task_spent_cents,
            task_remaining_cents: budget_root
                .budget_cents
                .map(|amount| amount.saturating_sub(task_spent_cents)),
            task_budget_exhausted: budget_root
                .budget_cents
                .is_some_and(|amount| task_spent_cents >= amount),
        }))
    }
}

// ───────────────────────────── heartbeat hooks ─────────────────────────────

/// Mark a completed heartbeat run's issue as awaiting review when the issue is
/// part of an orchestration that is not complete. Called from the heartbeat
/// settlement path, which runs its own transaction.
pub async fn orchestration_blocks_completion(
    connection: &mut sqlx::PgConnection,
    company_id: Uuid,
    issue_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM task_orchestrations orchestration
              WHERE orchestration.company_id = $1 AND orchestration.root_issue_id = $2
                AND orchestration.stage <> 'completed'
             UNION ALL
             SELECT 1 FROM task_orchestration_subtasks subtask
              WHERE subtask.company_id = $1 AND subtask.issue_id = $2
                AND subtask.status <> 'accepted')",
    )
    .bind(company_id)
    .bind(issue_id)
    .fetch_one(connection)
    .await
}

/// Pool-based wrapper for the run-settlement gate. A successful heartbeat run
/// must not close an orchestrated root or subtask issue until acceptance/the
/// final report is persisted.
pub async fn orchestration_requires_review(
    pool: &PgPool,
    company_id: Uuid,
    issue_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut connection = pool.acquire().await?;
    orchestration_blocks_completion(&mut connection, company_id, issue_id).await
}

/// Number of required subtasks that still await acceptance (used by tests and
/// diagnostics).
pub async fn pending_required_subtasks(
    pool: &PgPool,
    company_id: Uuid,
    root_issue_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_orchestration_subtasks subtask
           JOIN task_orchestrations orchestration ON orchestration.id = subtask.orchestration_id
          WHERE orchestration.company_id = $1 AND orchestration.root_issue_id = $2
            AND subtask.required AND subtask.status <> 'accepted'",
    )
    .bind(company_id)
    .bind(root_issue_id)
    .fetch_one(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_with(subtasks: Vec<ContractSubtask>) -> DecisionContract {
        DecisionContract {
            contract_version: DECISION_CONTRACT_VERSION,
            assessment: "Needs three pieces of work".to_string(),
            complexity: "complex".to_string(),
            rationale: "Split by expertise".to_string(),
            members: Vec::new(),
            subtasks,
            questions: Vec::new(),
            ceo_executes: true,
        }
    }

    fn subtask(key: &str, owner: Uuid) -> ContractSubtask {
        ContractSubtask {
            key: key.to_string(),
            title: format!("Work {key}"),
            description: "Do the work".to_string(),
            owner_agent_id: owner,
            reviewer_agent_id: None,
            acceptance_criteria: "It works".to_string(),
            blocked_by: Vec::new(),
            parent_key: None,
        }
    }

    #[test]
    fn contract_rejects_self_dependency_and_cycles() {
        let owner = Uuid::new_v4();
        let mut first = subtask("a", owner);
        first.blocked_by = vec!["b".to_string()];
        let mut second = subtask("b", owner);
        second.blocked_by = vec!["a".to_string()];
        let contract = contract_with(vec![first, second]);
        assert!(validate_decision_contract(&contract).is_err());
    }

    #[test]
    fn contract_rejects_subtasks_while_questions_are_open() {
        let owner = Uuid::new_v4();
        let mut contract = contract_with(vec![subtask("a", owner)]);
        contract.members.push(ContractMember {
            agent_id: Some(owner),
            action: "reuse".to_string(),
            role: "analyst".to_string(),
            responsibilities: "analysis".to_string(),
            reason: "has the context".to_string(),
            reports_to_agent_id: None,
            hire: None,
        });
        contract.questions.push(ContractQuestion {
            question: "Which market?".to_string(),
            why_it_blocks: "Scope depends on it".to_string(),
        });
        assert!(validate_decision_contract(&contract).is_err());
    }

    #[test]
    fn simple_task_cannot_declare_subtasks() {
        let owner = Uuid::new_v4();
        let mut contract = contract_with(vec![subtask("a", owner)]);
        contract.complexity = "simple".to_string();
        contract.members.push(ContractMember {
            agent_id: Some(owner),
            action: "reuse".to_string(),
            role: "analyst".to_string(),
            responsibilities: "analysis".to_string(),
            reason: "has the context".to_string(),
            reports_to_agent_id: None,
            hire: None,
        });
        assert!(validate_decision_contract(&contract).is_err());
    }

    #[test]
    fn a_delegated_task_must_declare_a_subtask() {
        let owner = Uuid::new_v4();
        let mut contract = contract_with(Vec::new());
        contract.complexity = "simple".to_string();
        contract.ceo_executes = false;
        contract.members.push(ContractMember {
            agent_id: Some(owner),
            action: "reuse".to_string(),
            role: "analyst".to_string(),
            responsibilities: "analysis".to_string(),
            reason: "has the context".to_string(),
            reports_to_agent_id: None,
            hire: None,
        });
        assert!(validate_decision_contract(&contract).is_err());
        // An asking-only revision is legitimate: the plan comes after answers.
        contract.questions.push(ContractQuestion {
            question: "Which scope?".to_string(),
            why_it_blocks: "It decides the plan".to_string(),
        });
        assert!(validate_decision_contract(&contract).is_ok());
    }

    #[test]
    fn hire_must_not_name_an_existing_agent_and_needs_details() {
        let contract = DecisionContract {
            contract_version: DECISION_CONTRACT_VERSION,
            assessment: "Needs a designer".to_string(),
            complexity: "complex".to_string(),
            rationale: "No designer on the team".to_string(),
            members: vec![ContractMember {
                agent_id: None,
                action: "hire".to_string(),
                role: "designer".to_string(),
                responsibilities: "design".to_string(),
                reason: "gap".to_string(),
                reports_to_agent_id: None,
                hire: None,
            }],
            subtasks: Vec::new(),
            questions: Vec::new(),
            ceo_executes: false,
        };
        assert!(validate_decision_contract(&contract).is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let raw = serde_json::json!({
            "contractVersion": 1,
            "assessment": "x",
            "complexity": "simple",
            "rationale": "y",
            "ceoExecutes": true,
            "unexpected": "nope"
        });
        assert!(parse_decision_contract(&raw).is_err());
    }

    #[test]
    fn invalid_contract_version_is_rejected() {
        let mut contract = contract_with(Vec::new());
        contract.contract_version = 7;
        assert!(validate_decision_contract(&contract).is_err());
    }

    #[test]
    fn valid_simple_contract_parses() {
        let raw = serde_json::json!({
            "contractVersion": DECISION_CONTRACT_VERSION,
            "assessment": "small fix",
            "complexity": "simple",
            "rationale": "one owner",
            "ceoExecutes": true,
            "members": [],
            "subtasks": [],
            "questions": []
        });
        let contract = parse_decision_contract(&raw).expect("valid");
        assert!(contract.ceo_executes);
    }
}
