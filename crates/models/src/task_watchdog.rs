//! Task watchdog domain models.
//!
//! Mirrors paperclip's task-watchdogs subsystem: a watchdog tracks a watched
//! issue and evaluates its subtree (recursively) for liveness, stopping, and
//! review state. These models back the four watchdog support tables.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A slim issue projection used by the watchdog subtree classifier.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TaskWatchdogClassifierIssue {
    pub id: Uuid,
    pub company_id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub status: String,
    pub parent_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub origin_kind: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

// ============================================================================
// heartbeat_runs
// ============================================================================

/// Execution run status. Live = queued|running; terminal = the rest.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "heartbeat_run_status", rename_all = "snake_case")]
pub enum HeartbeatRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl HeartbeatRunStatus {
    /// Whether this status counts as a live (in-progress) execution path.
    pub fn is_live(&self) -> bool {
        matches!(self, HeartbeatRunStatus::Queued | HeartbeatRunStatus::Running)
    }
}

/// Agent execution heartbeat run. Only the columns the watchdog reads.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct HeartbeatRun {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub invocation_source: String,
    pub status: HeartbeatRunStatus,
    pub responsible_user_id: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    /// context_snapshot carries `{ issueId, taskId }` used for live-path matching.
    pub context_snapshot: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// issue_watchdogs
// ============================================================================

/// Watchdog lifecycle status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "issue_watchdog_status", rename_all = "snake_case")]
pub enum IssueWatchdogStatus {
    Active,
    Paused,
    Resolved,
    Archived,
}

/// One watchdog per (company, watched issue).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueWatchdog {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub watchdog_agent_id: Uuid,
    pub instructions: Option<String>,
    pub status: IssueWatchdogStatus,
    pub watchdog_issue_id: Option<Uuid>,
    pub last_observed_fingerprint: Option<String>,
    pub last_reviewed_fingerprint: Option<String>,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub last_completed_at: Option<DateTime<Utc>>,
    pub trigger_count: i32,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_by_run_id: Option<Uuid>,
    pub updated_by_agent_id: Option<Uuid>,
    pub updated_by_user_id: Option<String>,
    pub updated_by_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// agent_wakeup_requests
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "agent_wakeup_request_status", rename_all = "snake_case")]
pub enum AgentWakeupRequestStatus {
    Queued,
    Dispatched,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentWakeupRequestStatus {
    /// Whether this wake keeps the subtree "live".
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            AgentWakeupRequestStatus::Queued
                | AgentWakeupRequestStatus::Dispatched
                | AgentWakeupRequestStatus::Running
        )
    }
}

/// A queued/active agent wake that keeps an issue subtree live until the run starts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentWakeupRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub status: AgentWakeupRequestStatus,
    /// payload carries `{ issueId, taskId, _paperclipWakeContext: { issueId } }`.
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// issue_thread_interactions
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "issue_thread_interaction_status", rename_all = "snake_case")]
pub enum IssueThreadInteractionStatus {
    Pending,
    Resolved,
    Cancelled,
}

/// A pending thread interaction that keeps a stopped watchdog issue in the
/// in_review review path.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueThreadInteraction {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub kind: String,
    pub status: IssueThreadInteractionStatus,
    pub source_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
