use async_trait::async_trait;
use chrono::Utc;
use repositories::{ApprovalRepository, IssueRepository};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::{Approval, ApprovalType, ApprovalStatus};
use crate::ServiceError;

/// Create Approval Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApprovalInput {
    pub company_id: Uuid,
    pub approval_type: ApprovalType,
    pub requested_by_agent_id: Option<Uuid>,
    pub requested_by_user_id: Option<Uuid>,
    pub payload: serde_json::Value,
    pub linked_issue_ids: Vec<Uuid>,
}

/// Review Approval Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewApprovalInput {
    pub approval_id: Uuid,
    pub decision: ApprovalDecision,
    pub decided_by_user_id: Uuid,
    pub decision_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approve,
    Reject,
    RequestRevision,
}

/// Approval with linked issues info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalWithContext {
    pub approval: Approval,
    pub linked_issue_ids: Vec<Uuid>,
    pub can_proceed: bool,
}

/// Approval Service trait
#[async_trait]
pub trait ApprovalService: Send + Sync {
    /// Create approval request
    async fn create(&self, input: CreateApprovalInput) -> Result<Approval, ServiceError>;

    /// Get approval by ID
    async fn get_by_id(&self, id: Uuid) -> Result<ApprovalWithContext, ServiceError>;

    /// List approvals by company
    async fn list_by_company(&self, company_id: Uuid, status: Option<ApprovalStatus>) -> Result<Vec<Approval>, ServiceError>;

    /// List pending approvals for user
    async fn list_pending_for_user(&self, user_id: Uuid) -> Result<Vec<ApprovalWithContext>, ServiceError>;

    /// Review approval (approve/reject/request revision)
    async fn review(&self, input: ReviewApprovalInput) -> Result<Approval, ServiceError>;

    /// Cancel approval
    async fn cancel(&self, approval_id: Uuid, user_id: Uuid) -> Result<Approval, ServiceError>;

    /// Check if approval can proceed (all linked issues resolved)
    async fn check_can_proceed(&self, approval_id: Uuid) -> Result<bool, ServiceError>;

    /// Get approvals for issue
    async fn get_by_issue_id(&self, issue_id: Uuid) -> Result<Vec<Approval>, ServiceError>;
}

/// Default Approval Service Implementation
pub struct DefaultApprovalService {
    approval_repo: Arc<dyn ApprovalRepository>,
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultApprovalService {
    pub fn new(
        approval_repo: Arc<dyn ApprovalRepository>,
        issue_repo: Arc<dyn IssueRepository>,
    ) -> Self {
        Self {
            approval_repo,
            issue_repo,
        }
    }

    /// Validate approval payload based on type
    fn validate_payload(&self, approval_type: ApprovalType, payload: &serde_json::Value) -> Result<(), ServiceError> {
        match approval_type {
            ApprovalType::HireAgent => {
                // Validate agent hiring payload
                if !payload.get("agent_role").is_some() {
                    return Err(ServiceError::InvalidInput("Missing agent_role in payload".to_string()));
                }
                if !payload.get("agent_name").is_some() {
                    return Err(ServiceError::InvalidInput("Missing agent_name in payload".to_string()));
                }
            }
            ApprovalType::SpendCredits => {
                // Validate credit spending payload
                if !payload.get("amount").and_then(|v| v.as_i64()).is_some() {
                    return Err(ServiceError::InvalidInput("Missing or invalid amount in payload".to_string()));
                }
                if !payload.get("purpose").is_some() {
                    return Err(ServiceError::InvalidInput("Missing purpose in payload".to_string()));
                }
            }
            ApprovalType::CreateResource => {
                // Validate resource creation payload
                if !payload.get("resource_type").is_some() {
                    return Err(ServiceError::InvalidInput("Missing resource_type in payload".to_string()));
                }
            }
            ApprovalType::DeployAgent => {
                // Validate agent deployment payload
                if !payload.get("agent_id").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("Missing agent_id in payload".to_string()));
                }
                if !payload.get("environment").is_some() {
                    return Err(ServiceError::InvalidInput("Missing environment in payload".to_string()));
                }
            }
        }

        Ok(())
    }

    /// Send notification about approval status change
    async fn notify_approval_change(&self, approval: &Approval, decision: Option<ApprovalDecision>) -> Result<(), ServiceError> {
        // TODO: Implement notification system
        // This would send emails, webhooks, or push notifications
        // For now, just log the event

        let event_type = match decision {
            Some(ApprovalDecision::Approve) => "approval.approved",
            Some(ApprovalDecision::Reject) => "approval.rejected",
            Some(ApprovalDecision::RequestRevision) => "approval.revision_requested",
            None => "approval.created",
        };

        // Would publish event to EventBus here
        println!("Approval event: {} for approval {}", event_type, approval.id);

        Ok(())
    }
}

#[async_trait]
impl ApprovalService for DefaultApprovalService {
    async fn create(&self, input: CreateApprovalInput) -> Result<Approval, ServiceError> {
        // Validate requester
        if input.requested_by_agent_id.is_none() && input.requested_by_user_id.is_none() {
            return Err(ServiceError::InvalidInput("Either agent_id or user_id must be provided".to_string()));
        }

        // Validate payload
        self.validate_payload(input.approval_type, &input.payload)?;

        // Validate linked issues exist
        for issue_id in &input.linked_issue_ids {
            let issue = self.issue_repo
                .find_by_id(*issue_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to find issue: {}", e)))?;

            if issue.is_none() {
                return Err(ServiceError::NotFound(format!("Issue {} not found", issue_id)));
            }
        }

        let now = Utc::now();
        let approval = Approval {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            approval_type: input.approval_type,
            requested_by_agent_id: input.requested_by_agent_id,
            requested_by_user_id: input.requested_by_user_id,
            status: ApprovalStatus::Pending,
            payload: input.payload,
            decision_note: None,
            decided_by_user_id: None,
            decided_at: None,
            created_at: now,
            updated_at: now,
        };

        // Create approval
        let created_approval = self.approval_repo
            .create(approval.clone())
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create approval: {}", e)))?;

        // Link to issues
        for issue_id in input.linked_issue_ids {
            self.approval_repo
                .link_to_issue(created_approval.id, issue_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to link approval to issue: {}", e)))?;
        }

        // Send notification
        self.notify_approval_change(&created_approval, None).await?;

        Ok(created_approval)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<ApprovalWithContext, ServiceError> {
        let approval = self.approval_repo
            .find_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find approval: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Approval not found".to_string()))?;

        let linked_issue_ids = self.approval_repo
            .find_linked_issues(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find linked issues: {}", e)))?;

        let can_proceed = self.check_can_proceed(id).await?;

        Ok(ApprovalWithContext {
            approval,
            linked_issue_ids,
            can_proceed,
        })
    }

    async fn list_by_company(&self, company_id: Uuid, status: Option<ApprovalStatus>) -> Result<Vec<Approval>, ServiceError> {
        self.approval_repo
            .find_by_company_id(company_id, status)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list approvals: {}", e)))
    }

    async fn list_pending_for_user(&self, user_id: Uuid) -> Result<Vec<ApprovalWithContext>, ServiceError> {
        // Get user's company memberships
        // For now, simplified - get all pending approvals the user can see
        let approvals = self.approval_repo
            .find_pending_for_reviewer(user_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list pending approvals: {}", e)))?;

        let mut results = Vec::new();
        for approval in approvals {
            let linked_issue_ids = self.approval_repo
                .find_linked_issues(approval.id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to find linked issues: {}", e)))?;

            let can_proceed = self.check_can_proceed(approval.id).await?;

            results.push(ApprovalWithContext {
                approval,
                linked_issue_ids,
                can_proceed,
            });
        }

        Ok(results)
    }

    async fn review(&self, input: ReviewApprovalInput) -> Result<Approval, ServiceError> {
        let mut approval = self.approval_repo
            .find_by_id(input.approval_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find approval: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Approval not found".to_string()))?;

        // Check if approval is still pending
        if approval.status != ApprovalStatus::Pending {
            return Err(ServiceError::InvalidInput(format!(
                "Approval is already {}, cannot review",
                match approval.status {
                    ApprovalStatus::Approved => "approved",
                    ApprovalStatus::Rejected => "rejected",
                    ApprovalStatus::RevisionRequested => "in revision",
                    ApprovalStatus::Pending => "pending",
                }
            )));
        }

        // Update approval based on decision
        approval.status = match input.decision {
            ApprovalDecision::Approve => ApprovalStatus::Approved,
            ApprovalDecision::Reject => ApprovalStatus::Rejected,
            ApprovalDecision::RequestRevision => ApprovalStatus::RevisionRequested,
        };

        approval.decided_by_user_id = Some(input.decided_by_user_id);
        approval.decided_at = Some(Utc::now());
        approval.decision_note = input.decision_note;
        approval.updated_at = Utc::now();

        // Update approval
        let updated_approval = self.approval_repo
            .update(approval)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update approval: {}", e)))?;

        // Send notification
        self.notify_approval_change(&updated_approval, Some(input.decision)).await?;

        // If approved, publish event to unblock linked issues
        if input.decision == ApprovalDecision::Approve {
            // TODO: Publish ApprovalApproved event to EventBus
            // This will trigger the listener to unblock issues
        }

        Ok(updated_approval)
    }

    async fn cancel(&self, approval_id: Uuid, user_id: Uuid) -> Result<Approval, ServiceError> {
        let mut approval = self.approval_repo
            .find_by_id(approval_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find approval: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Approval not found".to_string()))?;

        // Check if approval can be cancelled
        if approval.status != ApprovalStatus::Pending && approval.status != ApprovalStatus::RevisionRequested {
            return Err(ServiceError::InvalidInput("Only pending or revision-requested approvals can be cancelled".to_string()));
        }

        // Check if user is the requester
        let is_requester = approval.requested_by_user_id == Some(user_id)
            || approval.requested_by_agent_id.is_some(); // Agent-requested can be cancelled by any board user

        if !is_requester {
            return Err(ServiceError::Forbidden("Only the requester can cancel this approval".to_string()));
        }

        approval.status = ApprovalStatus::Rejected;
        approval.decided_by_user_id = Some(user_id);
        approval.decided_at = Some(Utc::now());
        approval.decision_note = Some("Cancelled by requester".to_string());
        approval.updated_at = Utc::now();

        let updated_approval = self.approval_repo
            .update(approval)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update approval: {}", e)))?;

        Ok(updated_approval)
    }

    async fn check_can_proceed(&self, approval_id: Uuid) -> Result<bool, ServiceError> {
        let approval = self.approval_repo
            .find_by_id(approval_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find approval: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound("Approval not found".to_string()))?;

        // If not approved, cannot proceed
        if approval.status != ApprovalStatus::Approved {
            return Ok(false);
        }

        // Check if all linked issues are resolved
        let linked_issue_ids = self.approval_repo
            .find_linked_issues(approval_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find linked issues: {}", e)))?;

        for issue_id in linked_issue_ids {
            let issue = self.issue_repo
                .find_by_id(issue_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to find issue: {}", e)))?
                .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

            // Check if issue is in terminal state
            let is_terminal = matches!(
                issue.status,
                models::IssueStatus::Done | models::IssueStatus::Cancelled
            );

            if !is_terminal {
                return Ok(false); // At least one issue is not resolved
            }
        }

        Ok(true)
    }

    async fn get_by_issue_id(&self, issue_id: Uuid) -> Result<Vec<Approval>, ServiceError> {
        self.approval_repo
            .find_by_issue_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find approvals for issue: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_hire_agent_payload() {
        let service = DefaultApprovalService::new(
            Arc::new(MockApprovalRepository::new()),
            Arc::new(MockIssueRepository::new()),
        );

        // Valid payload
        let valid_payload = serde_json::json!({
            "agent_role": "researcher",
            "agent_name": "Research Agent",
            "budget": 1000
        });

        assert!(service.validate_payload(ApprovalType::HireAgent, &valid_payload).is_ok());

        // Invalid payload - missing agent_role
        let invalid_payload = serde_json::json!({
            "agent_name": "Research Agent"
        });

        assert!(service.validate_payload(ApprovalType::HireAgent, &invalid_payload).is_err());
    }

    #[test]
    fn test_validate_spend_credits_payload() {
        let service = DefaultApprovalService::new(
            Arc::new(MockApprovalRepository::new()),
            Arc::new(MockIssueRepository::new()),
        );

        // Valid payload
        let valid_payload = serde_json::json!({
            "amount": 5000,
            "purpose": "API calls for data analysis"
        });

        assert!(service.validate_payload(ApprovalType::SpendCredits, &valid_payload).is_ok());

        // Invalid payload - missing amount
        let invalid_payload = serde_json::json!({
            "purpose": "API calls"
        });

        assert!(service.validate_payload(ApprovalType::SpendCredits, &invalid_payload).is_err());
    }
}
