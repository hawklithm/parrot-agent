use crate::heartbeat_service::{HeartbeatService, HeartbeatWakeupOptions};
use models::AppError;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Issue assignment wakeup service
/// 
/// Migrated from paperclip: server/src/services/issue-assignment-wakeup.ts
/// Handles automatic agent wakeup when an issue is assigned or reassigned.
pub struct IssueAssignmentWakeupService {
    heartbeat: Arc<dyn HeartbeatService>,
}

#[derive(Debug, Clone)]
pub struct QueueWakeupInput {
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub assignee_agent_id: Option<Uuid>,
    pub status: String,
    pub reason: String,
    pub mutation: String,
    pub context_source: String,
    pub requested_by_actor_type: Option<String>,
    pub requested_by_actor_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub rethrow_on_error: bool,
}

impl IssueAssignmentWakeupService {
    pub fn new(heartbeat: Arc<dyn HeartbeatService>) -> Self {
        Self { heartbeat }
    }

    /// Queue issue assignment wakeup
    /// 
    /// Wakes the assigned agent when an issue is created/updated,
    /// unless the issue is in backlog status or has no assignee.
    pub async fn queue_wakeup(&self, input: QueueWakeupInput) -> Result<(), AppError> {
        // Skip if no assignee or issue is in backlog
        if input.assignee_agent_id.is_none() || input.status == "backlog" {
            info!(
                issue_id = %input.issue_id,
                status = %input.status,
                "Skipping wakeup: no assignee or backlog status"
            );
            return Ok(());
        }

        let agent_id = input.assignee_agent_id.unwrap();

        info!(
            issue_id = %input.issue_id,
            agent_id = %agent_id,
            reason = %input.reason,
            mutation = %input.mutation,
            "Queueing issue assignment wakeup"
        );

        // Call heartbeat service to wake the agent
        let result = self
            .heartbeat
            .wakeup_with_options(
                agent_id,
                input.issue_id,
                input.company_id,
                HeartbeatWakeupOptions {
                    source: Some("assignment".to_string()),
                    trigger_detail: Some("system".to_string()),
                    reason: Some(input.reason.clone()),
                    requested_by_actor_type: input.requested_by_actor_type.clone(),
                    requested_by_actor_id: input.requested_by_actor_id,
                    payload: Some(serde_json::json!({
                        "issueId": input.issue_id,
                        "mutation": input.mutation,
                    })),
                    context_snapshot: Some(serde_json::json!({
                        "issueId": input.issue_id,
                        "source": input.context_source,
                    })),
                    idempotency_key: input.idempotency_key.clone(),
                    retry_of_run_id: None,
                },
            )
            .await;

        match result {
            Ok(_) => {
                info!(
                    issue_id = %input.issue_id,
                    agent_id = %agent_id,
                    "Successfully queued agent wakeup"
                );
                Ok(())
            }
            Err(err) => {
                warn!(
                    issue_id = %input.issue_id,
                    agent_id = %agent_id,
                    error = ?err,
                    "Failed to wake assignee on issue assignment"
                );

                if input.rethrow_on_error {
                    Err(AppError::Internal(format!(
                        "Failed to wake agent {}: {}",
                        agent_id, err
                    )))
                } else {
                    // Swallow error by default (matches paperclip behavior)
                    Ok(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heartbeat_service::mock::MockHeartbeatService;

    #[tokio::test]
    async fn test_queue_wakeup_success() {
        let heartbeat = Arc::new(MockHeartbeatService::new());
        let service = IssueAssignmentWakeupService::new(heartbeat.clone());

        let input = QueueWakeupInput {
            company_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            assignee_agent_id: Some(Uuid::new_v4()),
            status: "todo".to_string(),
            reason: "issue_assigned".to_string(),
            mutation: "create".to_string(),
            context_source: "issue.create".to_string(),
            requested_by_actor_type: Some("user".to_string()),
            requested_by_actor_id: Some(Uuid::new_v4()),
            idempotency_key: None,
            rethrow_on_error: false,
        };

        let result = service.queue_wakeup(input).await;
        assert!(result.is_ok());
        assert_eq!(heartbeat.wakeup_count(), 1);
    }

    #[tokio::test]
    async fn test_queue_wakeup_skips_backlog() {
        let heartbeat = Arc::new(MockHeartbeatService::new());
        let service = IssueAssignmentWakeupService::new(heartbeat.clone());

        let input = QueueWakeupInput {
            company_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            assignee_agent_id: Some(Uuid::new_v4()),
            status: "backlog".to_string(),
            reason: "issue_assigned".to_string(),
            mutation: "create".to_string(),
            context_source: "issue.create".to_string(),
            requested_by_actor_type: None,
            requested_by_actor_id: None,
            idempotency_key: None,
            rethrow_on_error: false,
        };

        let result = service.queue_wakeup(input).await;
        assert!(result.is_ok());
        assert_eq!(heartbeat.wakeup_count(), 0); // Should not wake
    }

    #[tokio::test]
    async fn test_queue_wakeup_skips_no_assignee() {
        let heartbeat = Arc::new(MockHeartbeatService::new());
        let service = IssueAssignmentWakeupService::new(heartbeat.clone());

        let input = QueueWakeupInput {
            company_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            assignee_agent_id: None,
            status: "todo".to_string(),
            reason: "issue_assigned".to_string(),
            mutation: "create".to_string(),
            context_source: "issue.create".to_string(),
            requested_by_actor_type: None,
            requested_by_actor_id: None,
            idempotency_key: None,
            rethrow_on_error: false,
        };

        let result = service.queue_wakeup(input).await;
        assert!(result.is_ok());
        assert_eq!(heartbeat.wakeup_count(), 0); // Should not wake
    }

    #[tokio::test]
    async fn test_queue_wakeup_handles_error_gracefully() {
        let heartbeat = Arc::new(MockHeartbeatService::new());
        heartbeat.set_should_fail(true);
        let service = IssueAssignmentWakeupService::new(heartbeat.clone());

        let input = QueueWakeupInput {
            company_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            assignee_agent_id: Some(Uuid::new_v4()),
            status: "todo".to_string(),
            reason: "issue_assigned".to_string(),
            mutation: "create".to_string(),
            context_source: "issue.create".to_string(),
            requested_by_actor_type: None,
            requested_by_actor_id: None,
            idempotency_key: None,
            rethrow_on_error: false, // Should swallow error
        };

        let result = service.queue_wakeup(input).await;
        assert!(result.is_ok()); // Error should be swallowed
    }

    #[tokio::test]
    async fn test_queue_wakeup_rethrows_error_when_requested() {
        let heartbeat = Arc::new(MockHeartbeatService::new());
        heartbeat.set_should_fail(true);
        let service = IssueAssignmentWakeupService::new(heartbeat.clone());

        let input = QueueWakeupInput {
            company_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            assignee_agent_id: Some(Uuid::new_v4()),
            status: "todo".to_string(),
            reason: "issue_assigned".to_string(),
            mutation: "create".to_string(),
            context_source: "issue.create".to_string(),
            requested_by_actor_type: None,
            requested_by_actor_id: None,
            idempotency_key: None,
            rethrow_on_error: true, // Should rethrow
        };

        let result = service.queue_wakeup(input).await;
        assert!(result.is_err()); // Error should be rethrown
    }
}
