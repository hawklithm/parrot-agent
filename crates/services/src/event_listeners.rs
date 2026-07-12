use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use models::event_bus::{
    Event, EventHandler, SystemEvent, SystemEventPayload,
    IssueEvent, ApprovalEvent, RoutineEvent, EnvironmentEvent,
};

use repositories::ActivityLogRepository;

// ==================== Issue完成 → Goal进度更新监听器 ====================

pub struct IssueCompletedToGoalProgressListener<G> {
    goal_service: Arc<G>,
}

impl<G> IssueCompletedToGoalProgressListener<G> {
    pub fn new(goal_service: Arc<G>) -> Self {
        Self { goal_service }
    }
}

#[async_trait]
impl<G: GoalService> EventHandler for IssueCompletedToGoalProgressListener<G> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Issue(IssueEvent::Completed { issue_id, .. }) = &system_event.payload {
            self.goal_service
                .recalculate_progress_for_issue(*issue_id)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["issue.completed".to_string()]
    }

    fn handler_name(&self) -> &str {
        "issue_completed_to_goal_progress"
    }
}

// ==================== Approval批准 → Issue解除阻塞监听器 ====================

pub struct ApprovalApprovedToIssueUnblockListener<I> {
    issue_service: Arc<I>,
}

impl<I> ApprovalApprovedToIssueUnblockListener<I> {
    pub fn new(issue_service: Arc<I>) -> Self {
        Self { issue_service }
    }
}

#[async_trait]
impl<I: IssueService> EventHandler for ApprovalApprovedToIssueUnblockListener<I> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Approval(ApprovalEvent::Approved { approval_id, .. }) = &system_event.payload {
            self.issue_service
                .unblock_by_approval(*approval_id)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["approval.approved".to_string()]
    }

    fn handler_name(&self) -> &str {
        "approval_approved_to_issue_unblock"
    }
}

// ==================== Routine触发 → Issue创建监听器 ====================

pub struct RoutineTriggeredToIssueCreationListener<I> {
    issue_service: Arc<I>,
}

impl<I> RoutineTriggeredToIssueCreationListener<I> {
    pub fn new(issue_service: Arc<I>) -> Self {
        Self { issue_service }
    }
}

#[async_trait]
impl<I: IssueService> EventHandler for RoutineTriggeredToIssueCreationListener<I> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Routine(RoutineEvent::Triggered { routine_id, .. }) = &system_event.payload {
            self.issue_service
                .create_and_checkout_for_routine(*routine_id)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["routine.triggered".to_string()]
    }

    fn handler_name(&self) -> &str {
        "routine_triggered_to_issue_creation"
    }
}

// ==================== Environment Lease过期 → Workspace清理监听器 ====================

pub struct LeaseExpiredToWorkspaceCleanupListener<W, A> {
    workspace_service: Arc<W>,
    activity_log_repo: Arc<A>,
}

impl<W, A> LeaseExpiredToWorkspaceCleanupListener<W, A> {
    pub fn new(workspace_service: Arc<W>, activity_log_repo: Arc<A>) -> Self {
        Self {
            workspace_service,
            activity_log_repo,
        }
    }
}

#[async_trait]
impl<W: WorkspaceService, A: ActivityLogRepository> EventHandler
    for LeaseExpiredToWorkspaceCleanupListener<W, A>
{
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Environment(EnvironmentEvent::LeaseExpired {
            environment_id,
            company_id,
            ..
        }) = &system_event.payload
        {
            self.workspace_service
                .cleanup_by_environment(*environment_id)
                .await
                .map_err(|e| e.to_string())?;

            let activity = crate::activity_log_service::Activity::new(
                *company_id,
                crate::activity_log_service::ActorType::System,
                *environment_id,
                crate::activity_log_service::ActivityAction::WorkspaceDeleted,
                crate::activity_log_service::ResourceType::Environment,
                *environment_id,
                crate::activity_log_service::ActivityMetadata {
                    category: Some("workspace_cleanup".to_string()),
                    severity: Some("info".to_string()),
                    audit_critical: false,
                    extra: serde_json::json!({
                        "reason": "lease_expired"
                    }),
                },
            );

            let repo_activity = repositories::activity_log_repository::Activity {
                id: activity.id,
                company_id: activity.company_id,
                actor_type: repositories::activity_log_repository::ActorType::Agent,
                actor_id: activity.actor_id,
                action: repositories::activity_log_repository::ActivityAction::Execute,
                resource_type: repositories::activity_log_repository::ResourceType::Agent,
                resource_id: activity.actor_id,
                metadata: None,
                created_at: chrono::Utc::now(),
            };
            let _ = self.activity_log_repo.log_activity(&repo_activity).await;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["environment.lease_expired".to_string()]
    }

    fn handler_name(&self) -> &str {
        "lease_expired_to_workspace_cleanup"
    }
}

// ==================== Issue CheckedOut → Recovery Action Reconcile监听器 ====================

pub struct IssueCheckedOutToRecoveryReconcileListener<R> {
    recovery_service: Arc<R>,
}

impl<R> IssueCheckedOutToRecoveryReconcileListener<R> {
    pub fn new(recovery_service: Arc<R>) -> Self {
        Self { recovery_service }
    }
}

#[async_trait]
impl<R: RecoveryActionService> EventHandler for IssueCheckedOutToRecoveryReconcileListener<R> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Issue(IssueEvent::CheckedOut { issue_id, company_id, .. }) = &system_event.payload {
            // When an issue is checked out, reconcile any pending recovery actions
            self.recovery_service
                .reconcile_for_issue(*company_id, *issue_id)
                .await?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["issue.checked_out".to_string()]
    }

    fn handler_name(&self) -> &str {
        "issue_checked_out_to_recovery_reconcile"
    }
}

// ==================== Issue StatusChanged → Watchdog Evaluation监听器 ====================

pub struct IssueStatusChangedToWatchdogEvalListener<W> {
    watchdog_service: Arc<W>,
}

impl<W> IssueStatusChangedToWatchdogEvalListener<W> {
    pub fn new(watchdog_service: Arc<W>) -> Self {
        Self { watchdog_service }
    }
}

#[async_trait]
impl<W: TaskWatchdogService> EventHandler for IssueStatusChangedToWatchdogEvalListener<W> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Issue(IssueEvent::StatusChanged { issue_id, company_id, new_status, .. }) = &system_event.payload {
            // When issue status changes, reconcile watchdogs for this issue and ancestors
            self.watchdog_service
                .reconcile_for_issue_and_ancestors(*company_id, *issue_id, new_status)
                .await?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["issue.status_changed".to_string()]
    }

    fn handler_name(&self) -> &str {
        "issue_status_changed_to_watchdog_eval"
    }
}

// ==================== Issue Completed → Recovery Action Resolve监听器 ====================

pub struct IssueCompletedToRecoveryResolveListener<R> {
    recovery_service: Arc<R>,
}

impl<R> IssueCompletedToRecoveryResolveListener<R> {
    pub fn new(recovery_service: Arc<R>) -> Self {
        Self { recovery_service }
    }
}

#[async_trait]
impl<R: RecoveryActionService> EventHandler for IssueCompletedToRecoveryResolveListener<R> {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        let system_event = match event.as_any().downcast_ref::<SystemEvent>() {
            Some(e) => e,
            None => return Ok(()),
        };

        if let SystemEventPayload::Issue(IssueEvent::Completed { issue_id, company_id, .. }) = &system_event.payload {
            // When an issue is completed, resolve all active recovery actions
            self.recovery_service
                .resolve_active_for_issue(*company_id, *issue_id)
                .await?;
        }

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["issue.completed".to_string()]
    }

    fn handler_name(&self) -> &str {
        "issue_completed_to_recovery_resolve"
    }
}

// ==================== Service trait placeholders (existing) ====================

// RecoveryActionService trait used by the listeners above
#[async_trait]
pub trait RecoveryActionService: Send + Sync {
    async fn reconcile_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, String>;
    async fn resolve_active_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, String>;
}

#[async_trait]
pub trait TaskWatchdogService: Send + Sync {
    async fn reconcile_for_issue_and_ancestors(&self, company_id: Uuid, issue_id: Uuid, new_status: &str) -> Result<(), String>;
}

// Re-export RecoveryAction for use in trait
use models::RecoveryAction;

// Original service traits (existing)
#[async_trait]
pub trait IssueService: Send + Sync {
    async fn unblock_by_approval(&self, approval_id: Uuid) -> Result<(), String>;
    async fn create_and_checkout_for_routine(&self, routine_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait WorkspaceService: Send + Sync {
    async fn cleanup_by_environment(&self, environment_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait GoalService: Send + Sync {
    async fn recalculate_progress_for_issue(&self, issue_id: Uuid) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_ids_are_unique() {
        let ids = vec![
            "issue_completed_to_goal_progress",
            "approval_approved_to_issue_unblock",
            "routine_triggered_to_issue_creation",
            "lease_expired_to_workspace_cleanup",
            "issue_checked_out_to_recovery_reconcile",
            "issue_status_changed_to_watchdog_eval",
            "issue_completed_to_recovery_resolve",
        ];

        let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, ids.len(), "Listener IDs must be unique");
    }
}
