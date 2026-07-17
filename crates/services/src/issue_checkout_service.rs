use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use models::{Issue, IssueStatus, EnvironmentLease};
use repositories::IssueRepository;
use crate::errors::ServiceError;
use crate::lease_service::{LeaseService, AcquireLeaseRequest};
use crate::heartbeat_service::HeartbeatService;

/// Checkout input with environment acquisition
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutIssueInput {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub expected_statuses: Vec<String>,
    pub checkout_run_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
}

/// Checkout result with lease information
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutResult {
    pub issue: Issue,
    pub lease: Option<EnvironmentLease>,
    pub should_wake_assignee: bool,
}

/// Release input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseIssueInput {
    pub release_run_id: Uuid,
    pub result: Option<String>,
    pub target_status: Option<String>,
    pub release_lease: bool,
}

/// Force release input (admin operation)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceReleaseInput {
    pub admin_user_id: Uuid,
    pub reason: String,
    pub release_lease: bool,
}

/// Actor type for wakeup decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorType {
    Board,
    User,
    Agent,
}

/// Wakeup decision input
#[derive(Debug, Clone)]
pub struct CheckoutWakeInput {
    pub actor_type: ActorType,
    pub actor_agent_id: Option<Uuid>,
    pub checkout_agent_id: Uuid,
    pub checkout_run_id: Option<Uuid>,
}

/// Issue Checkout Service with atomic environment lease integration
#[async_trait]
pub trait IssueCheckoutService: Send + Sync {
    /// Checkout issue with atomic environment lease acquisition
    async fn checkout(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CheckoutIssueInput,
    ) -> Result<CheckoutResult, ServiceError>;

    /// Release issue and optionally release environment lease
    async fn release(
        &self,
        issue_id: Uuid,   company_id: Uuid,
        input: ReleaseIssueInput,
    ) -> Result<Issue, ServiceError>;

    /// Force release (admin operation)
    async fn force_release(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: ForceReleaseInput,
    ) -> Result<Issue, ServiceError>;

    /// Decide if assignee should be woken up on checkout
    fn should_wake_assignee_on_checkout(&self, input: &CheckoutWakeInput) -> bool;
}

/// Default implementation with atomic checkout/lease
pub struct DefaultIssueCheckoutService {
    issue_repo: Arc<dyn IssueRepository>,
    lease_service: Arc<dyn LeaseService>,
    heartbeat_service: Arc<dyn HeartbeatService>,
}

impl DefaultIssueCheckoutService {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        lease_service: Arc<dyn LeaseService>,
        heartbeat_service: Arc<dyn HeartbeatService>,
    ) -> Self {
        Self {
            issue_repo,
            lease_service,
            heartbeat_service,
        }
    }

    /// Validate status is in expected list
    fn validate_expected_status(
        &self,
        current_status: &IssueStatus,
        expected_statuses: &[String],
    ) -> Result<(), ServiceError> {
        if expected_statuses.is_empty() {
            return Ok(());
        }

        let status_str = current_status.to_string();
        if !expected_statuses.contains(&status_str) {
            return Err(ServiceError::Conflict(format!(
                "Issue status '{}' not in expected statuses: {:?}",
                status_str, expected_statuses
            )));
        }

        Ok(())
    }

    /// Determine target status after checkout
    fn determine_checkout_status(&self, current_status: &IssueStatus) -> IssueStatus {
        match current_status {
            IssueStatus::Todo | IssueStatus::Backlog | IssueStatus::Blocked | IssueStatus::InReview => IssueStatus::InProgress,
            _ => current_status.clone(),
        }
    }

    /// Determine target status after release
    fn determine_release_status(
        &self,
        current_status: &IssueStatus,
        result: Option<&str>,
        explicit_target: Option<&str>,
    ) -> IssueStatus {
        if let Some(target) = explicit_target {
            return match target {
                "done" => IssueStatus::Done,
                "todo" => IssueStatus::Todo,
                "cancelled" => IssueStatus::Cancelled,
                "in_progress" => IssueStatus::InProgress,
                "in_review" => IssueStatus::InReview,
                _ => current_status.clone(),
            };
        }

        match result {
            Some("success") => IssueStatus::Done,
            Some("failed") => IssueStatus::Todo,
            Some("cancelled") => IssueStatus::Cancelled,
            Some("needs_review") => IssueStatus::InReview,
            _ => {
                // Default: in_progress -> in_review (paperclip pattern)
                if *current_status == IssueStatus::InProgress {
                    IssueStatus::InReview
                } else {
                    current_status.clone()
                }
            }
        }
    }

    /// Get issue with company isolation check
    async fn get_issue_verified(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Issue, ServiceError> {
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Access denied to issue from different company".to_string()
            ));
        }

        Ok(issue)
    }
}

#[async_trait]
impl IssueCheckoutService for DefaultIssueCheckoutService {
    async fn checkout(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CheckoutIssueInput,
    ) -> Result<CheckoutResult, ServiceError> {
        let issue = self.get_issue_verified(issue_id, company_id).await?;

        // Step 1: Validate expected s
        self.validate_expected_status(&issue.status, &input.expected_statuses)?;

        // Step 2: Acquire environment lease if environment_id provided
        let lease = if let Some(environment_id) = input.environment_id {
            let lease_request = AcquireLeaseRequest {
                environment_id,
                execution_workspace_id: input.execution_workspace_id,
                issue_id: Some(issue_id),
                heartbeat_run_id: Some(input.checkout_run_id),
            };

            match self.lease_service.acquire_lease(company_id, lease_request).await {
                Ok(lease) => Some(lease),
                Err(e) => {
                    // Lease acquisition failed - do NOT update issue
                    return Err(ServiceError::Internal(format!(
                        "Failed to acquire environment lease: {}. Issue checkout aborted.",
                        e
                    )));
                }
            }
        } else {
            None
        };

        // Step 3: Update issue (only if lease acquired successfully or no lease needed)
        let new_status = self.determine_checkout_status(&issue.status);
        let update_input = models::UpdateIssueInput {
            title: None, description: None, status: Some(new_status),
            priority: None, assignee_agent_id: None, assignee_user_id: None,
            work_mode: None, responsible_user_id: None, source_trust: None,
            monitor_scheduled_by: None, monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None, hidden_at: None,
            execution_workspace_preference: None, execution_workspace_settings: None,
            execution_policy: None, execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
        };

        match self.issue_repo.update(issue_id, update_input).await {
            Ok(updated_issue) => {
                // Determine wakeup decision
                let should_wake = self.should_wake_assignee_on_checkout(&CheckoutWakeInput {
                    actor_type: if input.agent_id.is_some() {
                        ActorType::Agent
                    } else if input.user_id.is_some() {
                        ActorType::User
                    } else {
                        ActorType::Board
                    },
                    actor_agent_id: input.agent_id,
                    checkout_agent_id: input.agent_id.unwrap_or_else(Uuid::nil),
                    checkout_run_id: Some(input.checkout_run_id),
                });

                // Heartbeat integration: wake up the assignee if needed
                if should_wake {
                    if let Some(assignee_agent_id) = updated_issue.assignee_agent_id {
                        let heartbeat = self.heartbeat_service.clone();
                        let issue_id = updated_issue.id;
                        let company_id = updated_issue.company_id;
                        // Fire-and-forget wakeup — don't block the checkout response
                        tokio::spawn(async move {
                            if let Err(e) = heartbeat.wakeup(assignee_agent_id, issue_id, company_id).await {
                                tracing::warn!("Heartbeat wakeup failed for agent={}, issue={}: {}", assignee_agent_id, issue_id, e);
                            }
                        });
                    }
                }

                Ok(CheckoutResult {
                    issue: updated_issue,
                    lease,
                    should_wake_assignee: should_wake,
                })
            }
            Err(e) => {
                // Issue update failed - release the acquired lease
                if let Some(lease) = lease {
                    let _ = self.lease_service.release_lease(lease.id, company_id).await;
                }

                Err(ServiceError::Internal(format!(
                    "Failed to update issue during checkout: {}",
                    e
                )))
            }
        }
    }

    async fn release(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: ReleaseIssueInput,
    ) -> Result<Issue, ServiceError> {
        let issue = self.get_issue_verified(issue_id, company_id).await?;

        // Update status
        let new_status = self.determine_release_status(
            &issue.status,
            input.result.as_deref(),
            input.target_status.as_deref(),
        );
        let update_input = models::UpdateIssueInput {
            title: None, description: None, status: Some(new_status),
            priority: None, assignee_agent_id: None, assignee_user_id: None,
            work_mode: None, responsible_user_id: None, source_trust: None,
            monitor_scheduled_by: None, monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None, hidden_at: None,
            execution_workspace_preference: None, execution_workspace_settings: None,
            execution_policy: None, execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
        };

        let updated_issue = self.issue_repo
            .update(issue_id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to release issue: {}", e)))?;

        // Release lease if requested
        if input.release_lease {
            // Find active leases for this issue
            let active_leases = self.lease_service
                .get_active_leases(company_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to get active leases: {}", e)))?;

            for lease in active_leases {
                if lease.issue_id == Some(issue_id) {
                    let _ = self.lease_service.release_lease(lease.id, company_id).await;
                }
            }
        }

        Ok(updated_issue)
    }

    async fn force_release(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: ForceReleaseInput,
    ) -> Result<Issue, ServiceError> {
        let _issue = self.get_issue_verified(issue_id, company_id).await?;

        // Admin can force release from any status
        let update_input = models::UpdateIssueInput {
            title: None, description: None, status: Some(IssueStatus::Todo),
            priority: None, assignee_agent_id: None, assignee_user_id: None,
            work_mode: None, responsible_user_id: None, source_trust: None,
            monitor_scheduled_by: None, monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None, hidden_at: None,
            execution_workspace_preference: None, execution_workspace_settings: None,
            execution_policy: None, execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
        };

        let updated_issue = self.issue_repo
            .update(issue_id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to force release issue: {}", e)))?;

        // Heartbeat integration: cancel active run for the assignee
        if let Some(assignee_agent_id) = updated_issue.assignee_agent_id {
            let heartbeat = self.heartbeat_service.clone();
            let issue_id = updated_issue.id;
            let company_id = updated_issue.company_id;
            let reason = format!("force_release by admin={}", input.admin_user_id);
            // Fire-and-forget cancel — don't block the force_release response
            tokio::spawn(async move {
                if let Err(e) = heartbeat.cancel_run(assignee_agent_id, issue_id, company_id, &reason).await {
                    tracing::warn!("Heartbeat cancel_run failed for agent={}, issue={}: {}", assignee_agent_id, issue_id, e);
                }
            });
        }

        // Always release lease on force release
        if input.release_lease {
            let active_leases = self.lease_service
                .get_active_leases(company_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to get active leases: {}", e)))?;

            for lease in active_leases {
                if lease.issue_id == Some(issue_id) {
                    let _ = self.lease_service.release_lease(lease.id, company_id).await;
                }
            }
        }

        Ok(updated_issue)
    }

    fn should_wake_assignee_on_checkout(&self, input: &CheckoutWakeInput) -> bool {
        // Non-agent actors (board/user) always wake the assignee
        if input.actor_type != ActorType::Agent {
            return true;
        }

        // No actor_agent_id means external trigger -> wake
        let Some(actor_agent_id) = input.actor_agent_id else {
            return true;
        };

        // Actor is different agent -> wake
        if actor_agent_id != input.checkout_agent_id {
            return true;
        }

        // No checkout_run_id means fresh checkout -> wake
        // If checkout_run_id exists, it's the same agent continuing work -> don't re-wake
        input.checkout_run_id.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease_service::MockLeaseService;

    #[tokio::test]
    async fn test_should_wake_assignee_on_checkout() {
        let heartbeat = Arc::new(crate::heartbeat_service::mock::MockHeartbeatService::new());
        let service = DefaultIssueCheckoutService::new(
            Arc::new(crate::MockIssueRepository::new()),
            Arc::new(MockLeaseService::new()),
            heartbeat,
        );

        let agent_id = Uuid::new_v4();
        let other_agent_id = Uuid::new_v4();

        // Board actor -> always wake
        assert!(service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::Board,
            actor_agent_id: None,
            checkout_agent_id: agent_id,
            checkout_run_id: Some(Uuid::new_v4()),
        }));

        // User actor -> always wake
        assert!(service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::User,
            actor_agent_id: None,
            checkout_agent_id: agent_id,
            checkout_run_id: Some(Uuid::new_v4()),
        }));

        // Agent actor, no actor_agent_id -> wake
        assert!(service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::Agent,
            actor_agent_id: None,
            checkout_agent_id: agent_id,
            checkout_run_id: Some(Uuid::new_v4()),
        }));

        // Different agent -> wake
        assert!(service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::Agent,
            actor_agent_id: Some(other_agent_id),
            checkout_agent_id: agent_id,
            checkout_run_id: Some(Uuid::new_v4()),
        }));

        // Same agent, no checkout_run_id -> wake
        assert!(service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::Agent,
            actor_agent_id: Some(agent_id),
            checkout_agent_id: agent_id,
            checkout_run_id: None,
        }));

        // Same agent, same run -> don't wake (continuation)
        assert!(!service.should_wake_assignee_on_checkout(&CheckoutWakeInput {
            actor_type: ActorType::Agent,
            actor_agent_id: Some(agent_id),
            checkout_agent_id: agent_id,
            checkout_run_id: Some(Uuid::new_v4()),
        }));
    }

    #[test]
    fn test_determine_checkout_status() {
        let heartbeat = Arc::new(crate::heartbeat_service::mock::MockHeartbeatService::new());
        let service = DefaultIssueCheckoutService::new(
            Arc::new(crate::MockIssueRepository::new()),
            Arc::new(MockLeaseService::new()),
            heartbeat,
        );

        assert_eq!(service.determine_checkout_status("todo"), "in_progress");
        assert_eq!(service.determine_checkout_status("backlog"), "in_progress");
        assert_eq!(service.determine_checkout_status("blocked"), "in_progress");
        assert_eq!(service.determine_checkout_status("in_review"), "in_progress");
        assert_eq!(service.determine_checkout_status("done"), "done");
    }

    #[test]
    fn test_determine_release_status() {
        let heartbeat = Arc::new(crate::heartbeat_service::mock::MockHeartbeatService::new());
        let service = DefaultIssueCheckoutService::new(
            Arc::new(crate::MockIssueRepository::new()),
            Arc::new(MockLeaseService::new()),
            heartbeat,
        );

        // Explicit target
        assert_eq!(
            service.determine_release_status("in_progress", None, Some("done")),
            "done"
        );

        // Result-based
        assert_eq!(
            service.determine_release_status("in_progress", Some("success"), None),
            "done"
        );
        assert_eq!(
            service.determine_release_status("in_progress", Some("failed"), None),
            "todo"
        );
        assert_eq!(
            service.determine_release_status("in_progress", Some("cancelled"), None),
            "cancelled"
        );
        assert_eq!(
            service.determine_release_status("in_progress", Some("needs_review"), None),
            "in_review"
        );

        // Default: in_progress -> in_review
        assert_eq!(
            service.determine_release_status("in_progress", None, None),
            "in_review"
        );

        // Other statuses unchanged
        assert_eq!(
            service.determine_release_status("blocked", None, None),
            "blocked"
        );
    }
}
