use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt;
use uuid::Uuid;

/// Event trait for all system events
#[async_trait]
pub trait Event: Send + Sync + fmt::Debug {
    fn event_type(&self) -> &str;
    fn payload(&self) -> &serde_json::Value;
    fn metadata(&self) -> &EventMetadata;
    fn timestamp(&self) -> DateTime<Utc>;
    /// 用于在 handler 中将 `&dyn Event` 向下转型为具体事件类型（如 `SystemEvent`）。
    fn as_any(&self) -> &dyn Any;
}

/// Event metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub event_id: Uuid,
    pub correlation_id: Option<Uuid>,
    pub causation_id: Option<Uuid>,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub company_id: Uuid,
}

/// Event handler trait
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: &dyn Event) -> Result<(), String>;
    fn event_types(&self) -> Vec<String>;
    fn handler_name(&self) -> &str;
}

/// Event bus trait
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: Box<dyn Event>) -> Result<(), String>;
    async fn subscribe(&self, handler: Box<dyn EventHandler>) -> Result<(), String>;
    async fn unsubscribe(&self, handler_name: &str) -> Result<(), String>;
}

/// Issue events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IssueEvent {
    Created {
        issue_id: Uuid,
        company_id: Uuid,
        title: String,
        created_by: Uuid,
    },
    CheckedOut {
        issue_id: Uuid,
        company_id: Uuid,
        agent_id: Uuid,
        checked_out_by: Uuid,
    },
    Released {
        issue_id: Uuid,
        company_id: Uuid,
        released_by: Uuid,
    },
    StatusChanged {
        issue_id: Uuid,
        company_id: Uuid,
        old_status: String,
        new_status: String,
        changed_by: Uuid,
    },
    Completed {
        issue_id: Uuid,
        company_id: Uuid,
        completed_by: Uuid,
        resolution: Option<String>,
    },
}

impl IssueEvent {
    pub fn issue_id(&self) -> Uuid {
        match self {
            Self::Created { issue_id, .. } => *issue_id,
            Self::CheckedOut { issue_id, .. } => *issue_id,
            Self::Released { issue_id, .. } => *issue_id,
            Self::StatusChanged { issue_id, .. } => *issue_id,
            Self::Completed { issue_id, .. } => *issue_id,
        }
    }

    pub fn company_id(&self) -> Uuid {
        match self {
            Self::Created { company_id, .. } => *company_id,
            Self::CheckedOut { company_id, .. } => *company_id,
            Self::Released { company_id, .. } => *company_id,
            Self::StatusChanged { company_id, .. } => *company_id,
            Self::Completed { company_id, .. } => *company_id,
        }
    }
}

/// Approval events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Requested {
        approval_id: Uuid,
        company_id: Uuid,
        requester_id: Uuid,
        issue_id: Option<Uuid>,
    },
    Approved {
        approval_id: Uuid,
        company_id: Uuid,
        approver_id: Uuid,
        issue_id: Option<Uuid>,
    },
    Rejected {
        approval_id: Uuid,
        company_id: Uuid,
        approver_id: Uuid,
        reason: String,
    },
    RevisionRequested {
        approval_id: Uuid,
        company_id: Uuid,
        approver_id: Uuid,
        feedback: String,
    },
}

/// Routine events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineEvent {
    Triggered {
        routine_id: Uuid,
        company_id: Uuid,
        trigger_type: String,
    },
    RunStarted {
        routine_id: Uuid,
        run_id: Uuid,
        company_id: Uuid,
    },
    RunCompleted {
        routine_id: Uuid,
        run_id: Uuid,
        company_id: Uuid,
        success: bool,
    },
    RunFailed {
        routine_id: Uuid,
        run_id: Uuid,
        company_id: Uuid,
        error: String,
    },
}

/// Agent events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Hired {
        agent_id: Uuid,
        company_id: Uuid,
        hired_by: Uuid,
    },
    StatusChanged {
        agent_id: Uuid,
        company_id: Uuid,
        old_status: String,
        new_status: String,
    },
    Terminated {
        agent_id: Uuid,
        company_id: Uuid,
        terminated_by: Uuid,
        reason: Option<String>,
    },
}

/// Environment events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvironmentEvent {
    LeaseAcquired {
        environment_id: Uuid,
        company_id: Uuid,
        agent_id: Uuid,
        lease_id: Uuid,
    },
    LeaseReleased {
        environment_id: Uuid,
        company_id: Uuid,
        agent_id: Uuid,
        lease_id: Uuid,
    },
    LeaseExpired {
        environment_id: Uuid,
        company_id: Uuid,
        agent_id: Uuid,
        lease_id: Uuid,
    },
}

/// Goal events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GoalEvent {
    Created {
        goal_id: Uuid,
        company_id: Uuid,
        created_by: Uuid,
    },
    ProgressUpdated {
        goal_id: Uuid,
        company_id: Uuid,
        old_progress: f64,
        new_progress: f64,
    },
    Completed {
        goal_id: Uuid,
        company_id: Uuid,
        completed_at: DateTime<Utc>,
    },
}

/// Unified event wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub metadata: EventMetadata,
    pub timestamp: DateTime<Utc>,
    pub payload: SystemEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum SystemEventPayload {
    Issue(IssueEvent),
    Approval(ApprovalEvent),
    Routine(RoutineEvent),
    Agent(AgentEvent),
    Environment(EnvironmentEvent),
    Goal(GoalEvent),
}

impl SystemEvent {
    pub fn new(metadata: EventMetadata, payload: SystemEventPayload) -> Self {
        Self {
            metadata,
            timestamp: Utc::now(),
            payload,
        }
    }

    pub fn event_type_str(&self) -> &str {
        match &self.payload {
            SystemEventPayload::Issue(e) => match e {
                IssueEvent::Created { .. } => "issue.created",
                IssueEvent::CheckedOut { .. } => "issue.checked_out",
                IssueEvent::Released { .. } => "issue.released",
                IssueEvent::StatusChanged { .. } => "issue.status_changed",
                IssueEvent::Completed { .. } => "issue.completed",
            },
            SystemEventPayload::Approval(e) => match e {
                ApprovalEvent::Requested { .. } => "approval.requested",
                ApprovalEvent::Approved { .. } => "approval.approved",
                ApprovalEvent::Rejected { .. } => "approval.rejected",
                ApprovalEvent::RevisionRequested { .. } => "approval.revision_requested",
            },
            SystemEventPayload::Routine(e) => match e {
                RoutineEvent::Triggered { .. } => "routine.triggered",
                RoutineEvent::RunStarted { .. } => "routine.run_started",
                RoutineEvent::RunCompleted { .. } => "routine.run_completed",
                RoutineEvent::RunFailed { .. } => "routine.run_failed",
            },
            SystemEventPayload::Agent(e) => match e {
                AgentEvent::Hired { .. } => "agent.hired",
                AgentEvent::StatusChanged { .. } => "agent.status_changed",
                AgentEvent::Terminated { .. } => "agent.terminated",
            },
            SystemEventPayload::Environment(e) => match e {
                EnvironmentEvent::LeaseAcquired { .. } => "environment.lease_acquired",
                EnvironmentEvent::LeaseReleased { .. } => "environment.lease_released",
                EnvironmentEvent::LeaseExpired { .. } => "environment.lease_expired",
            },
            SystemEventPayload::Goal(e) => match e {
                GoalEvent::Created { .. } => "goal.created",
                GoalEvent::ProgressUpdated { .. } => "goal.progress_updated",
                GoalEvent::Completed { .. } => "goal.completed",
            },
        }
    }
}

#[async_trait]
impl Event for SystemEvent {
    fn event_type(&self) -> &str {
        self.event_type_str()
    }

    fn payload(&self) -> &serde_json::Value {
        // This requires serialization - in production, cache this
        static EMPTY: serde_json::Value = serde_json::Value::Null;
        &EMPTY
    }

    fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
