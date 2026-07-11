use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::event_bus::{
    Event, EventHandler, EventHandlerError,
    IssueEvent, IssueEventAction,
    ApprovalEvent, ApprovalEventAction,
    RoutineEvent, RoutineEventAction,
    EnvironmentEvent, EnvironmentEventAction,
};

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
    async fn handle_event(&self, event: Arc<dyn Event>) -> Result<(), EventHandlerError> {
        let payload = event.payload();

        if let Ok(issue_event) = serde_json::from_value::<IssueEvent>(payload) {
            if issue_event.action == IssueEventAction::Completed {
                // 查询Issue关联的Goal并重新计算进度
                if let Err(e) = self.goal_service
                    .recalculate_progress_for_issue(issue_event.issue_id)
                    .await
                {
                    return Err(EventHandlerError::ExecutionFailed(e.to_string()));
                }
            }
        }

        Ok(())
    }

    fn handler_id(&self) -> &str {
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
    async fn handle_event(&self, event: Arc<dyn Event>) -> Result<(), EventHandlerError> {
        let payload = event.payload();

        if let Ok(approval_event) = serde_json::from_value::<ApprovalEvent>(payload) {
            if approval_event.action == ApprovalEventAction::Approved {
                // 查询关联的Issue并更新状态为in_progress
                if let Err(e) = self.issue_service
                    .unblock_by_approval(approval_event.approval_id)
                    .await
                {
                    return Err(EventHandlerError::ExecutionFailed(e.to_string()));
                }
            }
        }

        Ok(())
    }

    fn handler_id(&self) -> &str {
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
    async fn handle_event(&self, event: Arc<dyn Event>) -> Result<(), EventHandlerError> {
        let payload = event.payload();

        if let Ok(routine_event) = serde_json::from_value::<RoutineEvent>(payload) {
            if routine_event.action == RoutineEventAction::Triggered {
                // 创建Issue并checkout
                if let Err(e) = self.issue_service
                    .create_and_checkout_for_routine(routine_event.routine_id)
                    .await
                {
                    return Err(EventHandlerError::ExecutionFailed(e.to_string()));
                }
            }
        }

        Ok(())
    }

    fn handler_id(&self) -> &str {
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
impl<W: WorkspaceService, A: ActivityLogRepository> EventHandler for LeaseExpiredToWorkspaceCleanupListener<W, A> {
    async fn handle_event(&self, event: Arc<dyn Event>) -> Result<(), EventHandlerError> {
        let payload = event.payload();

        if let Ok(env_event) = serde_json::from_value::<EnvironmentEvent>(payload) {
            if env_event.action == EnvironmentEventAction::LeaseExpired {
                // 清理关联的工作空间
                if let Err(e) = self.workspace_service
                    .cleanup_by_environment(env_event.environment_id)
                    .await
                {
                    return Err(EventHandlerError::ExecutionFailed(e.to_string()));
                }

                // 记录活动日志
                let activity = crate::activity_log_service::Activity::new(
                    env_event.company_id,
                    crate::activity_log_service::ActorType::System,
                    env_event.environment_id,
                    crate::activity_log_service::ActivityAction::WorkspaceDeleted,
                    crate::activity_log_service::ResourceType::Environment,
                    env_event.environment_id,
                    crate::activity_log_service::ActivityMetadata {
                        category: Some("workspace_cleanup".to_string()),
                        severity: Some("info".to_string()),
                        audit_critical: false,
                        extra: serde_json::json!({
                            "reason": "lease_expired"
                        }),
                    },
                );

                let _ = self.activity_log_repo.log_activity(&activity).await;
            }
        }

        Ok(())
    }

    fn handler_id(&self) -> &str {
        "lease_expired_to_workspace_cleanup"
    }
}

// ==================== Service trait placeholders ====================

use repositories::ActivityLogRepository;

#[async_trait]
pub trait GoalService: Send + Sync {
    async fn recalculate_progress_for_issue(&self, issue_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait IssueService: Send + Sync {
    async fn unblock_by_approval(&self, approval_id: Uuid) -> Result<(), String>;
    async fn create_and_checkout_for_routine(&self, routine_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait WorkspaceService: Send + Sync {
    async fn cleanup_by_environment(&self, environment_id: Uuid) -> Result<(), String>;
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
        ];

        let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, ids.len(), "Listener IDs must be unique");
    }
}
