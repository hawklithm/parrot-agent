use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::sync::Arc;

use models::{Issue, IssueStatus, IssuePriority};
use repositories::IssueRepository;
use crate::errors::ServiceError;
use crate::issue_service::{ForceReleaseInput};

// Import existing services
use crate::issue_tree_control_service::IssueTreeControlService;
use crate::issue_comment_service::IssueCommentService;
use crate::issue_document_service::IssueDocumentService;
use crate::work_product_service::WorkProductService;
use crate::attachment_service::AttachmentService;

/// Issue mutation result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueMutationResult {
    pub changed: bool,
    pub issue: Issue,
    pub change_kind: String, // "created" | "updated" | "deleted" | "status_changed"
}

/// Checkout input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutInput {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub expected_statuses: Vec<String>,
    pub checkout_run_id: Uuid,
}

/// Release input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInput {
    pub release_run_id: Uuid,
    pub result: Option<String>,
    pub target_status: Option<String>,
}

/// Create issue input
#[derive(Debug, Clone, Deserialize)]
pub struct CreateIssueInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assigned_to: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
}

/// Update issue input
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateIssueInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assigned_to: Option<Uuid>,
}

/// Issue query filter
#[derive(Debug, Clone, Default)]
pub struct IssueQueryFilter {
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
}

/// Pagination
#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

/// Comprehensive Issue service trait with advanced features
#[async_trait]
pub trait IssueService: Send + Sync {
    /// Create a new issue
    async fn create(&self, input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError>;

    /// Create a child issue
    async fn create_child(&self, parent_id: Uuid, input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError>;

    /// Get issue by ID
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Issue, ServiceError>;

    /// List issues with filtering
    async fn list(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, ServiceError>;

    /// Update issue
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateIssueInput) -> Result<IssueMutationResult, ServiceError>;

    /// Delete issue
    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<IssueMutationResult, ServiceError>;

    /// Checkout issue for execution
    async fn checkout(&self, id: Uuid, company_id: Uuid, input: CheckoutInput) -> Result<Issue, ServiceError>;

    /// Release issue from execution
    async fn release(&self, id: Uuid, company_id: Uuid, input: ReleaseInput) -> Result<Issue, ServiceError>;

    /// Force release issue (admin operation)
    async fn force_release(&self, id: Uuid, company_id: Uuid, input: ForceReleaseInput) -> Result<Issue, ServiceError>;

    /// Search issues
    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, ServiceError>;

    /// Batch update issues
    async fn batch_update(
        &self,
        company_id: Uuid,
        issue_ids: Vec<Uuid>,
        status: Option<String>,
        priority: Option<String>,
        assignee_agent_id: Option<Uuid>,
        assignee_user_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, ServiceError>;

    /// Get heartbeat context for issue
    async fn get_heartbeat_context(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// Get tree control service
    fn tree_control(&self) -> Arc<dyn IssueTreeControlService>;

    /// Get comment service
    fn comments(&self) -> Arc<dyn IssueCommentService>;

    /// Get document service
    fn documents(&self) -> Arc<dyn IssueDocumentService>;

    /// Get work product service
    fn work_products(&self) -> Arc<dyn WorkProductService>;

    /// Get attachment service
    fn attachments(&self) -> Arc<dyn AttachmentService>;
}

/// Default Issue Service Implementation
pub struct DefaultIssueService {
    issue_repo: Arc<dyn IssueRepository>,
    tree_control_service: Arc<dyn IssueTreeControlService>,
    comment_service: Arc<dyn IssueCommentService>,
    document_service: Arc<dyn IssueDocumentService>,
    work_product_service: Arc<dyn WorkProductService>,
    attachment_service: Arc<dyn AttachmentService>,
}

impl DefaultIssueService {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        tree_control_service: Arc<dyn IssueTreeControlService>,
        comment_service: Arc<dyn IssueCommentService>,
        document_service: Arc<dyn IssueDocumentService>,
        work_product_service: Arc<dyn WorkProductService>,
        attachment_service: Arc<dyn AttachmentService>,
    ) -> Self {
        Self {
            issue_repo,
            tree_control_service,
            comment_service,
            document_service,
            work_product_service,
            attachment_service,
        }
    }

    /// Validate status transition
    fn validate_status_transition(&self, from_status: &IssueStatus, to_status: &IssueStatus) -> Result<(), ServiceError> {
        let valid_transitions: Vec<(IssueStatus, IssueStatus)> = vec![
            (IssueStatus::Todo, IssueStatus::InProgress),
            (IssueStatus::Todo, IssueStatus::Blocked),
            (IssueStatus::InProgress, IssueStatus::Blocked),
            (IssueStatus::InProgress, IssueStatus::Done),
            (IssueStatus::InProgress, IssueStatus::Cancelled),
            (IssueStatus::Blocked, IssueStatus::InProgress),
            (IssueStatus::Blocked, IssueStatus::Cancelled),
        ];

        let is_valid = valid_transitions.iter().any(|(from, to)| {
            from == from_status && to == to_status
        });

        if !is_valid {
            return Err(ServiceError::InvalidInput(format!(
                "Invalid status transition from '{}' to '{}'",
                from_status, to_status
            )));
        }

        Ok(())
    }
}

#[async_trait]
impl IssueService for DefaultIssueService {
    async fn create(&self, input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError> {
        // Validate parent exists if specified
        if let Some(parent_id) = input.parent_id {
            let parent = self.issue_repo
                .get_by_id(parent_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to verify parent: {}", e)))?;

            if parent.is_none() {
                return Err(ServiceError::NotFound(format!("Parent issue {} not found", parent_id)));
            }
        }

        let models_input = models::issue::CreateIssueInput {
            company_id: input.company_id,
            project_id: input.project_id,
            project_workspace_id: None,
            goal_id: input.goal_id,
            title: input.title,
            description: input.description,
            status: input.status,
            priority: input.priority,
            parent_id: input.parent_id,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            origin_kind: None,
            origin_id: None,
            origin_run_id: None,
            request_depth: None,
            billing_code: None,
            execution_workspace_id: None,
            execution_workspace_preference: None,
            assignee_adapter_overrides: None,
            created_by_agent_id: None,
            created_by_user_id: None,
        };
        let created_issue = self.issue_repo
            .create(models_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create issue: {}", e)))?;

        Ok(IssueMutationResult {
            changed: true,
            issue: created_issue,
            change_kind: "created".to_string(),
        })
    }

    async fn create_child(&self, parent_id: Uuid, mut input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError> {
        input.parent_id = Some(parent_id);
        self.create(input).await
    }

    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Issue, ServiceError> {
        let issue = self.issue_repo
            .get_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", id)))?;

        // Verify company access
        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden("Access denied to issue from different company".to_string()));
        }

        Ok(issue)
    }

    async fn list(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, ServiceError> {
        // Convert local types to repository types
        let models_filter = models::IssueQueryFilter {
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            project_id: filter.project_id,
            goal_id: filter.goal_id,
            parent_id: filter.parent_id,
            work_mode: None,
        };
        let models_pagination = models::Pagination {
            limit: pagination.limit,
            offset: pagination.offset,
            cursor: None,
        };
        self.issue_repo
            .list_by_company(company_id, &models_filter, &models_pagination)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list issues: {}", e)))
    }

    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateIssueInput) -> Result<IssueMutationResult, ServiceError> {
        let issue = self.get(id, company_id).await?;
        let status_changed = input.status.is_some();

        let update_input = models::UpdateIssueInput {
            title: input.title,
            description: input.description,
            status: input.status,
            priority: input.priority,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
        };

        let change_kind = if status_changed {
            "status_changed".to_string()
        } else {
            "updated".to_string()
        };

        let updated_issue = self.issue_repo
            .update(id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update issue: {}", e)))?;

        Ok(IssueMutationResult {
            changed: true,
            issue: updated_issue,
            change_kind,
        })
    }

    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<IssueMutationResult, ServiceError> {
        let issue = self.get(id, company_id).await?;

        // Check for child issues
        let children = self.issue_repo
            .list_children(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to check child issues: {}", e)))?;

        if !children.is_empty() {
            return Err(ServiceError::Conflict(format!(
                "Cannot delete issue with {} child issues",
                children.len()
            )));
        }

        self.issue_repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to delete issue: {}", e)))?;

        Ok(IssueMutationResult {
            changed: true,
            issue,
            change_kind: "deleted".to_string(),
        })
    }

    async fn checkout(&self, id: Uuid, company_id: Uuid, input: CheckoutInput) -> Result<Issue, ServiceError> {
        let mut issue = self.get(id, company_id).await?;

        // Verify expected status
        let status_str = issue.status.to_string();
        if !input.expected_statuses.is_empty() && !input.expected_statuses.contains(&status_str) {
            return Err(ServiceError::Conflict(format!(
                "Issue status '{}' not in expected statuses: {:?}",
                issue.status, input.expected_statuses
            )));
        }

        // Update to in_progress and assign
        let update_input = models::UpdateIssueInput {
            title: None,
            description: None,
            status: Some(IssueStatus::InProgress),
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
        };

        let updated_issue = self.issue_repo
            .update(id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to checkout issue: {}", e)))?;

        Ok(updated_issue)
    }

    async fn release(&self, id: Uuid, company_id: Uuid, input: ReleaseInput) -> Result<Issue, ServiceError> {
        let issue = self.get(id, company_id).await?;

        // Determine new status based on result
        let new_status = if let Some(target_status) = input.target_status {
            Some(target_status)
        } else if let Some(result) = input.result.as_deref() {
            Some(match result {
                "success" => "done",
                "failed" => "todo",
                "cancelled" => "cancelled",
                _ => "todo",
            }.to_string())
        } else {
            None
        };

        let update_input = models::UpdateIssueInput {
            title: None,
            description: None,
            status: new_status.and_then(|s| match s.as_str() {
                "done" => Some(IssueStatus::Done),
                "todo" => Some(IssueStatus::Todo),
                "cancelled" => Some(IssueStatus::Cancelled),
                "in_progress" => Some(IssueStatus::InProgress),
                "blocked" => Some(IssueStatus::Blocked),
                _ => None,
            }),
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
        };

        let updated_issue = self.issue_repo
            .update(id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to release issue: {}", e)))?;

        Ok(updated_issue)
    }

    async fn force_release(&self, id: Uuid, company_id: Uuid, _input: ForceReleaseInput) -> Result<Issue, ServiceError> {
        let _issue = self.get(id, company_id).await?;

        // Admin force release: reset to todo and clear execution state
        let update_input = models::UpdateIssueInput {
            title: None,
            description: None,
            status: Some(IssueStatus::Todo),
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
        };

        self.issue_repo
            .update(id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to force release issue: {}", e)))
    }

    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, ServiceError> {
        let models_pagination = models::Pagination {
            limit: pagination.limit,
            offset: pagination.offset,
            cursor: None,
        };
        self.issue_repo
            .search(company_id, query, &models_pagination)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to search issues: {}", e)))
    }

    async fn batch_update(
        &self,
        company_id: Uuid,
        issue_ids: Vec<Uuid>,
        status: Option<String>,
        _priority: Option<String>,
        _assignee_agent_id: Option<Uuid>,
        _assignee_user_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, ServiceError> {
        let mut results = Vec::new();

        for id in &issue_ids {
            let issue = self.get(*id, company_id).await?;

            let parsed_status = status.as_ref().and_then(|s| match s.as_str() {
                "backlog" => Some(IssueStatus::Backlog),
                "todo" => Some(IssueStatus::Todo),
                "in_progress" => Some(IssueStatus::InProgress),
                "in_review" => Some(IssueStatus::InReview),
                "blocked" => Some(IssueStatus::Blocked),
                "done" => Some(IssueStatus::Done),
                "cancelled" => Some(IssueStatus::Cancelled),
                _ => None,
            });

            let update_input = models::UpdateIssueInput {
                title: None,
                description: None,
                status: parsed_status,
                priority: None,
                assignee_agent_id: _assignee_agent_id,
                assignee_user_id: _assignee_user_id,
                work_mode: None,
                responsible_user_id: None,
                source_trust: None,
                monitor_scheduled_by: None,
                monitor_notes: None,
                hidden_at: None,
                execution_workspace_preference: None,
                execution_workspace_settings: None,
                execution_policy: None,
                execution_state: None,
            };

            let updated = self.issue_repo
                .update(*id, update_input)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to batch update issue {}: {}", id, e)))?;

            results.push(updated);
        }

        Ok(results)
    }

    async fn get_heartbeat_context(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, ServiceError> {
        let issue = self.get(id, company_id).await?;

        Ok(serde_json::json!({
            "issueId": id.to_string(),
            "companyId": company_id.to_string(),
            "title": issue.title,
            "status": issue.status.to_string(),
            "priority": issue.priority,
            "assigneeAgentId": issue.assignee_agent_id.map(|id| id.to_string()),
            "assigneeUserId": issue.assignee_user_id.map(|id| id.to_string()),
            "checkoutRunId": issue.checkout_run_id.map(|id| id.to_string()),
            "executionRunId": issue.execution_run_id.map(|id| id.to_string()),
            "executionLockedAt": issue.execution_locked_at,
            "activeRuns": [],
            "executionState": issue.execution_state.map(|s| s.0),
        }))
    }

    fn tree_control(&self) -> Arc<dyn IssueTreeControlService> {
        self.tree_control_service.clone()
    }

    fn comments(&self) -> Arc<dyn IssueCommentService> {
        self.comment_service.clone()
    }

    fn documents(&self) -> Arc<dyn IssueDocumentService> {
        self.document_service.clone()
    }

    fn work_products(&self) -> Arc<dyn WorkProductService> {
        self.work_product_service.clone()
    }

    fn attachments(&self) -> Arc<dyn AttachmentService> {
        self.attachment_service.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_status_transition() {
        let service = DefaultIssueService::new(
            Arc::new(MockIssueRepository::new()),
            Arc::new(MockIssueTreeControlService::new()),
            Arc::new(MockIssueCommentService::new()),
            Arc::new(MockIssueDocumentService::new()),
            Arc::new(MockWorkProductService::new()),
            Arc::new(MockAttachmentService::new()),
        );

        // Valid transitions
        assert!(service.validate_status_transition(&IssueStatus::Todo, &IssueStatus::InProgress).is_ok());
        assert!(service.validate_status_transition(&IssueStatus::InProgress, &IssueStatus::Done).is_ok());
        assert!(service.validate_status_transition(&IssueStatus::Blocked, &IssueStatus::InProgress).is_ok());

        // Invalid transitions
        assert!(service.validate_status_transition(&IssueStatus::Done, &IssueStatus::Todo).is_err());
        assert!(service.validate_status_transition(&IssueStatus::Todo, &IssueStatus::Done).is_err());
        assert!(service.validate_status_transition(&IssueStatus::Cancelled, &IssueStatus::InProgress).is_err());
    }
}
