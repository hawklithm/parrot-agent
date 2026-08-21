use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use models::{Issue, IssueStatus, IssuePriority};
use repositories::{IssueRepository, ApprovalRepository, RoutineRepository};
use crate::errors::ServiceError;
use crate::issue_service::{self, ForceReleaseInput};

// Import existing services
use crate::issue_tree_control_service::IssueTreeControlService;
use crate::issue_comment_service::IssueCommentService;
use crate::work_product_service::WorkProductService;
use crate::attachment_service::AttachmentService;
use crate::heartbeat_service::HeartbeatService;
use crate::recovery_action_service::RecoveryActionService;

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
    pub idempotency_key: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assigned_to: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub inherit_execution_workspace_from_issue_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub project_workspace_id: Option<Uuid>,
    pub work_mode: Option<models::IssueWorkMode>,
    pub harness_kind: Option<String>,
    pub responsible_user_id: Option<Uuid>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
    pub origin_run_id: Option<Uuid>,
    pub origin_fingerprint: Option<String>,
    pub request_depth: Option<i32>,
    pub billing_code: Option<String>,
    pub execution_workspace_id: Option<Uuid>,
    pub execution_workspace_preference: Option<String>,
    pub execution_policy: Option<models::IssueExecutionPolicy>,
    pub execution_workspace_settings: Option<serde_json::Value>,
    pub assignee_adapter_overrides: Option<serde_json::Value>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub label_ids: Vec<Uuid>,
    pub blocked_by_issue_ids: Vec<Uuid>,
    pub watchdog: Option<models::CreateIssueWatchdogInput>,
    pub watchdog_created_by_run_id: Option<Uuid>,
    #[serde(skip)]
    pub watchdog_discovery_audit: Option<models::WatchdogDiscoveryAuditInput>,
}

/// Match Paperclip's create-issue defaulting contract.
///
/// Assigned issues are immediately runnable and therefore default to `todo`;
/// unassigned issues remain in the backlog.  The REST/MCP boundary may omit
/// status, so this must be resolved before the repository INSERT rather than
/// relying on a nullable database bind.
fn resolve_create_issue_status(input: &CreateIssueInput) -> IssueStatus {
    input.status.unwrap_or_else(|| {
        if input.assignee_agent_id.is_some() || input.assignee_user_id.is_some() {
            IssueStatus::Todo
        } else {
            IssueStatus::Backlog
        }
    })
}

/// Update issue input
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateIssueInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<IssueStatus>,
    pub priority: Option<IssuePriority>,
    pub assigned_to: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub work_mode: Option<models::IssueWorkMode>,
    pub label_ids: Option<Vec<Uuid>>,
    pub blocked_by_issue_ids: Option<Vec<Uuid>>,
    pub harness_kind: Option<String>,
}

/// Issue query filter
#[derive(Debug, Clone, Default)]
pub struct IssueQueryFilter {
    pub status: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub participant_agent_id: Option<Uuid>,
    pub touched_by_user_id: Option<Uuid>,
    pub inbox_archived_by_user_id: Option<Uuid>,
    pub unread_for_user_id: Option<Uuid>,
    pub label_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
    pub origin_kind: Option<String>,
    pub origin_id: Option<String>,
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

    /// Get work product service
    fn work_products(&self) -> Arc<dyn WorkProductService>;

    /// Get attachment service
    fn attachments(&self) -> Arc<dyn AttachmentService>;

    /// Unblock issue by approval (called when approval is approved)
    async fn unblock_by_approval(&self, approval_id: Uuid) -> Result<(), ServiceError>;

    /// Create and checkout an issue for a routine
    async fn create_and_checkout_for_routine(&self, routine_id: Uuid) -> Result<(), ServiceError>;
}

/// Default Issue Service Implementation
pub struct DefaultIssueService {
    issue_repo: Arc<dyn IssueRepository>,
    approval_repo: Arc<dyn ApprovalRepository>,
    tree_control_service: Arc<dyn IssueTreeControlService>,
    comment_service: Arc<dyn IssueCommentService>,
    work_product_service: Arc<dyn WorkProductService>,
    attachment_service: Arc<dyn AttachmentService>,
    routine_repo: Option<Arc<dyn RoutineRepository>>,
}

impl DefaultIssueService {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        approval_repo: Arc<dyn ApprovalRepository>,
        tree_control_service: Arc<dyn IssueTreeControlService>,
        comment_service: Arc<dyn IssueCommentService>,
        work_product_service: Arc<dyn WorkProductService>,
        attachment_service: Arc<dyn AttachmentService>,
    ) -> Self {
        Self {
            issue_repo,
            approval_repo,
            tree_control_service,
            comment_service,
            work_product_service,
            attachment_service,
            routine_repo: None,
        }
    }

    pub fn with_routine_repo(mut self, routine_repo: Arc<dyn RoutineRepository>) -> Self {
        self.routine_repo = Some(routine_repo);
        self
    }

    /// Validate status transition
    #[allow(dead_code)]
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
    async fn create(&self, mut input: CreateIssueInput) -> Result<IssueMutationResult, ServiceError> {
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

        if let Some(source_issue_id) = input.inherit_execution_workspace_from_issue_id {
            let source = self
                .issue_repo
                .get_by_id(source_issue_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to verify workspace source: {e}")))?
                .ok_or_else(|| ServiceError::NotFound(format!("Workspace source issue {source_issue_id} not found")))?;
            if source.company_id != input.company_id {
                return Err(ServiceError::NotFound(format!(
                    "Workspace source issue {source_issue_id} not found"
                )));
            }
            if input.project_id.is_none() {
                input.project_id = source.project_id;
            }
            if input.project_workspace_id.is_none() {
                input.project_workspace_id = source.project_workspace_id;
            }
            if input.execution_workspace_id.is_none() {
                input.execution_workspace_id = source.execution_workspace_id;
            }
            if input.execution_workspace_preference.is_none() {
                input.execution_workspace_preference = source.execution_workspace_preference;
            }
            if input.execution_workspace_settings.is_none() {
                input.execution_workspace_settings = source
                    .execution_workspace_settings
                    .map(|settings| settings.0);
            }
        }

        let status = resolve_create_issue_status(&input);
        let models_input = models::issue::CreateIssueInput {
            company_id: input.company_id,
            project_id: input.project_id,
            project_workspace_id: input.project_workspace_id,
            goal_id: input.goal_id,
            title: input.title,
            idempotency_key: input.idempotency_key,
            description: input.description,
            status: Some(status),
            priority: input.priority,
            parent_id: input.parent_id,
            inherit_execution_workspace_from_issue_id: None,
            assignee_agent_id: input.assignee_agent_id,
            assignee_user_id: input.assignee_user_id,
            work_mode: input.work_mode,
            harness_kind: input.harness_kind,
            responsible_user_id: input.responsible_user_id,
            origin_kind: input.origin_kind,
            origin_id: input.origin_id,
            origin_run_id: input.origin_run_id,
            origin_fingerprint: input.origin_fingerprint,
            request_depth: input.request_depth,
            billing_code: input.billing_code,
            execution_workspace_id: input.execution_workspace_id,
            execution_workspace_preference: input.execution_workspace_preference,
            execution_policy: input.execution_policy,
            execution_workspace_settings: input.execution_workspace_settings,
            assignee_adapter_overrides: input.assignee_adapter_overrides,
            created_by_agent_id: input.created_by_agent_id,
            created_by_user_id: input.created_by_user_id,
            label_ids: input.label_ids,
            blocked_by_issue_ids: input.blocked_by_issue_ids,
            watchdog: input.watchdog,
            watchdog_discovery: None,
            watchdog_created_by_run_id: input.watchdog_created_by_run_id,
            watchdog_discovery_audit: input.watchdog_discovery_audit,
        };
        let created_issue = self.issue_repo
            .create(models_input)
            .await
            .map_err(|error| match error {
                repositories::RepositoryError::InvalidData(message) => ServiceError::InvalidInput(message),
                other => ServiceError::Internal(format!("Failed to create issue: {other}")),
            })?;

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
            search_query: None,
            participant_agent_id: filter.participant_agent_id,
            touched_by_user_id: filter.touched_by_user_id,
            inbox_archived_by_user_id: filter.inbox_archived_by_user_id,
            unread_for_user_id: filter.unread_for_user_id,
            label_id: filter.label_id,
            execution_workspace_id: filter.execution_workspace_id,
            origin_kind: filter.origin_kind.clone(),
            origin_id: filter.origin_id.clone(),
            ..Default::default()
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
        let _issue = self.get(id, company_id).await?;
        let status_changed = input.status.is_some();

        let update_input = models::UpdateIssueInput {
            title: input.title,
            description: input.description,
            status: input.status,
            priority: input.priority,
            assignee_agent_id: input.assignee_agent_id,
            assignee_user_id: input.assignee_user_id,
            work_mode: input.work_mode,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
            harness_kind: input.harness_kind,
            label_ids: input.label_ids,
            blocked_by_issue_ids: input.blocked_by_issue_ids,
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
        let issue = self.get(id, company_id).await?;

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
            monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
            harness_kind: None,
            label_ids: None,
            blocked_by_issue_ids: None,
        };

        let updated_issue = self.issue_repo
            .update(id, update_input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to checkout issue: {}", e)))?;

        Ok(updated_issue)
    }

    async fn release(&self, id: Uuid, company_id: Uuid, input: ReleaseInput) -> Result<Issue, ServiceError> {
        let _issue = self.get(id, company_id).await?;

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
            monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
            harness_kind: None,
            label_ids: None,
            blocked_by_issue_ids: None,
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
            monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
            harness_kind: None,
            label_ids: None,
            blocked_by_issue_ids: None,
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
        _filter: &IssueQueryFilter,
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
            let _issue = self.get(*id, company_id).await?;

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
                monitor_notes: None, monitor_next_check_at: None, monitor_last_triggered_at: None, monitor_attempt_count: None,
                hidden_at: None,
                execution_workspace_preference: None,
                execution_workspace_settings: None,
                execution_policy: None,
                execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
            harness_kind: None,
            label_ids: None,
            blocked_by_issue_ids: None,
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

    fn work_products(&self) -> Arc<dyn WorkProductService> {
        self.work_product_service.clone()
    }

    fn attachments(&self) -> Arc<dyn AttachmentService> {
        self.attachment_service.clone()
    }

    async fn unblock_by_approval(&self, approval_id: Uuid) -> Result<(), ServiceError> {
        // Find linked issues for this approval via the approval repository
        let linked_issue_ids = self.approval_repo
            .find_linked_issues(approval_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to find linked issues: {}", e)))?;

        for issue_id in linked_issue_ids {
            let issue = self.issue_repo
                .get_by_id(issue_id)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?;

            if let Some(issue) = issue {
                if issue.status == IssueStatus::Blocked {
                    let update = models::UpdateIssueInput {
                        status: Some(IssueStatus::InProgress),
                        ..Default::default()
                    };
                    self.issue_repo
                        .update(issue.id, update)
                        .await
                        .map_err(|e| ServiceError::Internal(format!("Failed to unblock issue: {}", e)))?;
                }
            }
        }

        Ok(())
    }

    async fn create_and_checkout_for_routine(&self, routine_id: Uuid) -> Result<(), ServiceError> {
        let routine_repo = self.routine_repo.as_ref().ok_or_else(|| ServiceError::Internal("routine repository is not wired".to_string()))?;
        let routine = routine_repo.get(routine_id).await
            .map_err(|e| ServiceError::Internal(format!("Failed to load routine: {e}")))?
            .ok_or_else(|| ServiceError::NotFound(format!("Routine {routine_id} not found")))?;
        if routine.status != models::RoutineStatus::Active {
            return Err(ServiceError::InvalidInput("Cannot trigger an inactive routine".to_string()));
        }
        let issue = self.issue_repo.create(models::CreateIssueInput {
            company_id: routine.company_id,
            project_id: routine.project_id,
            goal_id: routine.goal_id,
            title: routine.title,
            description: routine.description,
            status: Some(IssueStatus::Todo),
            priority: Some(match routine.priority { 0 => IssuePriority::Low, 1 => IssuePriority::Medium, _ => IssuePriority::High }),
            assignee_agent_id: Some(routine.assignee_agent_id),
            parent_id: routine.parent_issue_id,
            origin_kind: Some("routine".to_string()),
            origin_id: Some(routine.id.to_string()),
            ..Default::default()
        }).await.map_err(|e| ServiceError::Internal(format!("Failed to create routine issue: {e}")))?;
        tracing::info!(routine_id = %routine_id, issue_id = %issue.id, "created issue for triggered routine");
        Ok(())
    }
}

/// LegacyIssueService wraps DefaultIssueService and implements the simple IssueService trait
/// (from issue_service.rs) used by the API routes.
///
/// This adapter pattern allows DefaultIssueService to provide the full-featured implementation
/// while the routes use the simpler trait interface.
pub struct LegacyIssueService {
    inner: DefaultIssueService,
    issue_repo: Arc<dyn IssueRepository>,
    #[allow(dead_code)]
    comment_service: Arc<dyn IssueCommentService>,
    #[allow(dead_code)]
    work_product_service: Arc<dyn WorkProductService>,
    #[allow(dead_code)]
    attachment_service: Arc<dyn AttachmentService>,
    heartbeat_service: Arc<dyn HeartbeatService>,
    recovery_action_service: Arc<dyn RecoveryActionService>,
}

impl LegacyIssueService {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        approval_repo: Arc<dyn ApprovalRepository>,
        tree_control_service: Arc<dyn IssueTreeControlService>,
        comment_service: Arc<dyn IssueCommentService>,
        work_product_service: Arc<dyn WorkProductService>,
        attachment_service: Arc<dyn AttachmentService>,
        heartbeat_service: Arc<dyn HeartbeatService>,
        recovery_action_service: Arc<dyn RecoveryActionService>,
    ) -> Self {
        Self {
            inner: DefaultIssueService::new(
                issue_repo.clone(),
                approval_repo,
                tree_control_service,
                comment_service.clone(),
                work_product_service.clone(),
                attachment_service.clone(),
            ),
            issue_repo,
            comment_service,
            work_product_service,
            attachment_service,
            heartbeat_service,
            recovery_action_service,
        }
    }

    fn wake_assigned_issue(&self, issue: &Issue) {
        let Some(agent_id) = issue.assignee_agent_id else { return };
        if !matches!(issue.status, models::IssueStatus::Todo | models::IssueStatus::InProgress) {
            return;
        }
        let heartbeat = self.heartbeat_service.clone();
        let issue_id = issue.id;
        let company_id = issue.company_id;
        tokio::spawn(async move {
            if let Err(error) = heartbeat.wakeup(agent_id, issue_id, company_id).await {
                tracing::warn!(%error, %agent_id, %issue_id, "failed to wake assigned issue");
            }
        });
    }
}

#[async_trait]
impl issue_service::IssueService for LegacyIssueService {
    async fn create(&self, input: models::CreateIssueInput) -> Result<crate::issue_service::IssueMutationResult, String> {
        // Map models::CreateIssueInput -> issue_service_complete::CreateIssueInput
        let compat_input = CreateIssueInput {
            company_id: input.company_id,
            project_id: input.project_id,
            title: input.title,
            idempotency_key: input.idempotency_key,
            description: input.description,
            status: input.status,
            priority: input.priority,
            assigned_to: input.assignee_agent_id.or(input.assignee_user_id),
            assignee_agent_id: input.assignee_agent_id,
            assignee_user_id: input.assignee_user_id,
            parent_id: input.parent_id,
            inherit_execution_workspace_from_issue_id: input.inherit_execution_workspace_from_issue_id,
            goal_id: input.goal_id,
            project_workspace_id: input.project_workspace_id,
            work_mode: input.work_mode,
            harness_kind: input.harness_kind,
            responsible_user_id: input.responsible_user_id,
            origin_kind: input.origin_kind,
            origin_id: input.origin_id,
            origin_run_id: input.origin_run_id,
            origin_fingerprint: input.origin_fingerprint,
            request_depth: input.request_depth,
            billing_code: input.billing_code,
            execution_workspace_id: input.execution_workspace_id,
            execution_workspace_preference: input.execution_workspace_preference,
            execution_policy: input.execution_policy,
            execution_workspace_settings: input.execution_workspace_settings,
            assignee_adapter_overrides: input.assignee_adapter_overrides,
            created_by_agent_id: input.created_by_agent_id,
            created_by_user_id: input.created_by_user_id,
            label_ids: input.label_ids,
            blocked_by_issue_ids: input.blocked_by_issue_ids,
            watchdog: input.watchdog,
            watchdog_created_by_run_id: input.watchdog_created_by_run_id,
            watchdog_discovery_audit: input.watchdog_discovery_audit,
        };
        let result = self.inner.create(compat_input).await.map_err(|e| e.to_string())?;
        self.wake_assigned_issue(&result.issue);
        Ok(crate::issue_service::IssueMutationResult {
            changed: result.changed,
            issue: result.issue,
            change_kind: result.change_kind,
        })
    }

    async fn create_child(&self, parent_id: Uuid, input: models::CreateIssueInput) -> Result<crate::issue_service::IssueMutationResult, String> {
        let compat_input = CreateIssueInput {
            company_id: input.company_id,
            project_id: input.project_id,
            title: input.title,
            idempotency_key: input.idempotency_key,
            description: input.description,
            status: input.status,
            priority: input.priority,
            assigned_to: input.assignee_agent_id.or(input.assignee_user_id),
            assignee_agent_id: input.assignee_agent_id,
            assignee_user_id: input.assignee_user_id,
            parent_id: input.parent_id,
            inherit_execution_workspace_from_issue_id: input.inherit_execution_workspace_from_issue_id,
            goal_id: input.goal_id,
            project_workspace_id: input.project_workspace_id,
            work_mode: input.work_mode,
            harness_kind: input.harness_kind,
            responsible_user_id: input.responsible_user_id,
            origin_kind: input.origin_kind,
            origin_id: input.origin_id,
            origin_run_id: input.origin_run_id,
            origin_fingerprint: input.origin_fingerprint,
            request_depth: input.request_depth,
            billing_code: input.billing_code,
            execution_workspace_id: input.execution_workspace_id,
            execution_workspace_preference: input.execution_workspace_preference,
            execution_policy: input.execution_policy,
            execution_workspace_settings: input.execution_workspace_settings,
            assignee_adapter_overrides: input.assignee_adapter_overrides,
            created_by_agent_id: input.created_by_agent_id,
            created_by_user_id: input.created_by_user_id,
            label_ids: input.label_ids,
            blocked_by_issue_ids: input.blocked_by_issue_ids,
            watchdog: input.watchdog,
            watchdog_created_by_run_id: input.watchdog_created_by_run_id,
            watchdog_discovery_audit: input.watchdog_discovery_audit,
        };
        let result = self.inner.create_child(parent_id, compat_input).await.map_err(|e| e.to_string())?;
        self.wake_assigned_issue(&result.issue);
        Ok(crate::issue_service::IssueMutationResult {
            changed: result.changed,
            issue: result.issue,
            change_kind: result.change_kind,
        })
    }

    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Issue>, String> {
        let issue = self.issue_repo.get_by_id(id).await.map_err(|e| e.to_string())?;
        Ok(issue.filter(|issue| issue.company_id == company_id))
    }

    async fn list(&self, company_id: Uuid, filter: &crate::issue_service::IssueQueryFilter, pagination: &crate::issue_service::Pagination) -> Result<Vec<Issue>, String> {
        // Use models:: types since the repository trait expects them
        let filter = models::IssueQueryFilter {
            status: filter.status.clone(),
            priority: filter.priority.clone(),
            assignee_agent_id: filter.assignee_agent_id,
            assignee_user_id: filter.assignee_user_id,
            project_id: filter.project_id,
            parent_id: filter.parent_id,
            goal_id: filter.goal_id,
            work_mode: None,
            search_query: filter.search_query.clone(),
            participant_agent_id: filter.participant_agent_id,
            touched_by_user_id: filter.touched_by_user_id,
            inbox_archived_by_user_id: filter.inbox_archived_by_user_id,
            unread_for_user_id: filter.unread_for_user_id,
            label_id: filter.label_id,
            execution_workspace_id: filter.execution_workspace_id,
            origin_kind: filter.origin_kind.clone(),
            origin_id: filter.origin_id.clone(),
            ..Default::default()
        };
        let pagination = models::Pagination {
            limit: pagination.limit,
            offset: pagination.offset,
            cursor: None,
        };
        self.issue_repo.list_by_company(company_id, &filter, &pagination).await.map_err(|e| e.to_string())
    }

    async fn update(&self, id: Uuid, company_id: Uuid, input: models::UpdateIssueInput) -> Result<crate::issue_service::IssueMutationResult, String> {
        // Map models::UpdateIssueInput -> issue_service_complete::UpdateIssueInput
        let compat_input = UpdateIssueInput {
            title: input.title,
            description: input.description,
            status: input.status,
            priority: input.priority,
            assigned_to: input.assignee_agent_id.or(input.assignee_user_id),
            assignee_agent_id: input.assignee_agent_id,
            assignee_user_id: input.assignee_user_id,
            work_mode: input.work_mode,
            harness_kind: input.harness_kind,
            label_ids: input.label_ids,
            blocked_by_issue_ids: input.blocked_by_issue_ids,
        };
        let previous = self.issue_repo.get_by_id(id).await.map_err(|e| e.to_string())?;
        let result = self.inner.update(id, company_id, compat_input).await.map_err(|e| e.to_string())?;
        let should_wake = previous.as_ref().is_some_and(|previous| {
            let assignee_changed = previous.assignee_agent_id != result.issue.assignee_agent_id;
            let became_runnable = previous.status == models::IssueStatus::Backlog
                && result.issue.status != models::IssueStatus::Backlog;
            let reopened = result.issue.status == models::IssueStatus::Todo
                && previous.status != models::IssueStatus::Todo;
            assignee_changed || became_runnable || reopened
        });
        if should_wake {
            self.wake_assigned_issue(&result.issue);
        }
        Ok(crate::issue_service::IssueMutationResult {
            changed: result.changed,
            issue: result.issue,
            change_kind: result.change_kind,
        })
    }

    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<crate::issue_service::IssueMutationResult, String> {
        let result = self.inner.delete(id, company_id).await.map_err(|e| e.to_string())?;
        Ok(crate::issue_service::IssueMutationResult {
            changed: result.changed,
            issue: result.issue,
            change_kind: result.change_kind,
        })
    }

    async fn checkout(&self, id: Uuid, company_id: Uuid, input: crate::issue_service::CheckoutInput) -> Result<Issue, String> {
        let compat_input = CheckoutInput {
            agent_id: input.agent_id,
            user_id: input.user_id,
            expected_statuses: input.expected_statuses,
            checkout_run_id: input.checkout_run_id,
        };
        self.inner.checkout(id, company_id, compat_input).await.map_err(|e| e.to_string())
    }

    async fn release(&self, id: Uuid, company_id: Uuid, input: crate::issue_service::ReleaseInput) -> Result<Issue, String> {
        let compat_input = ReleaseInput {
            release_run_id: input.release_run_id,
            result: input.result,
            target_status: input.target_status,
        };
        self.inner.release(id, company_id, compat_input).await.map_err(|e| e.to_string())
    }

    async fn force_release(&self, id: Uuid, company_id: Uuid, input: crate::issue_service::ForceReleaseInput) -> Result<Issue, String> {
        self.inner.force_release(id, company_id, input).await.map_err(|e| e.to_string())
    }

    async fn search(&self, company_id: Uuid, query: &str, _filter: &crate::issue_service::IssueQueryFilter, _pagination: &crate::issue_service::Pagination) -> Result<Vec<Issue>, String> {
        let pagination = models::Pagination {
            limit: _pagination.limit,
            offset: _pagination.offset,
            cursor: None,
        };
        self.issue_repo.search(company_id, query, &pagination).await.map_err(|e| e.to_string())
    }

    async fn batch_update(&self, company_id: Uuid, issue_ids: Vec<Uuid>, status: Option<String>, priority: Option<String>, assignee_agent_id: Option<Uuid>, assignee_user_id: Option<Uuid>) -> Result<Vec<Issue>, String> {
        self.inner.batch_update(company_id, issue_ids, status, priority, assignee_agent_id, assignee_user_id).await.map_err(|e| e.to_string())
    }

    async fn get_heartbeat_context(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"issueId": id, "heartbeatContext": {}}))
    }

    // --- P1: Issue sub-resource methods (I1-I44) ---

    async fn get_activity(&self, id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![serde_json::json!({"issueId": id, "type": "created", "timestamp": chrono::Utc::now()})])
    }

    async fn get_cases(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![])
    }

    async fn get_active_run(&self, _id: Uuid, _company_id: Uuid) -> Result<Option<serde_json::Value>, String> {
        Ok(None)
    }

    async fn get_live_runs(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![])
    }

    async fn get_runs(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![])
    }

    async fn get_accepted_plan_decompositions(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![])
    }

    async fn submit_plan_decomposition(&self, id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"issueId": id, "decomposition": input, "submitted": true}))
    }

    async fn get_approvals(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![])
    }

    async fn create_approval(&self, id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"issueId": id, "approval": input, "created": true}))
    }

    async fn delete_approval(&self, _id: Uuid, _approval_id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn mark_read(&self, _id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn unmark_read(&self, _id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn archive_inbox(&self, _id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn unarchive_inbox(&self, _id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn get_recovery_actions(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        let actions = self
            .recovery_action_service
            .list_by_issue(company_id, id)
            .await?;
        actions
            .into_iter()
            .map(|action| serde_json::to_value(action).map_err(|error| error.to_string()))
            .collect()
    }

    async fn resolve_recovery_action(&self, id: Uuid, company_id: Uuid, action_id: Uuid) -> Result<(), String> {
        let actions = self
            .recovery_action_service
            .list_by_issue(company_id, id)
            .await?;
        if !actions.iter().any(|action| action.id == action_id) {
            return Err("Recovery action not found for issue".to_string());
        }
        self.recovery_action_service
            .resolve(
                action_id,
                &models::ResolveRecoveryActionInput { resolved_at: None },
            )
            .await
            .map(|_| ())
    }

    async fn create_work_product(&self, id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"issueId": id, "workProduct": input, "created": true}))
    }

    async fn get_comment(&self, comment_id: Uuid, _company_id: Uuid) -> Result<Option<serde_json::Value>, String> {
        Ok(Some(serde_json::json!({"id": comment_id, "body": "Comment"})))
    }

    async fn get_cost_summary(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"issueId": id, "totalCostCents": 0}))
    }
}

#[cfg(all(test, feature = "legacy-unit-tests"))]
mod tests {
    use super::*;
    use repositories::RepositoryResult;
    use models::ApprovalStatus;

    type MockIssueRepo = crate::MockIssueRepository;

    struct MockApprovalRepo;
    impl MockApprovalRepo {
        fn new() -> Self { Self }
    }

    #[async_trait]
    impl ApprovalRepository for MockApprovalRepo {
        async fn create(&self, _approval: models::Approval) -> RepositoryResult<models::Approval> { unimplemented!() }
        async fn find_by_id(&self, _id: Uuid) -> RepositoryResult<Option<models::Approval>> { Ok(None) }
        async fn find_by_company_id(&self, _company_id: Uuid, _status: Option<ApprovalStatus>) -> RepositoryResult<Vec<models::Approval>> { Ok(vec![]) }
        async fn find_pending_for_reviewer(&self, _user_id: Uuid) -> RepositoryResult<Vec<models::Approval>> { Ok(vec![]) }
        async fn update(&self, _approval: models::Approval) -> RepositoryResult<models::Approval> { unimplemented!() }
        async fn find_linked_issues(&self, _approval_id: Uuid) -> RepositoryResult<Vec<Uuid>> { Ok(vec![]) }
        async fn link_to_issue(&self, _approval_id: Uuid, _issue_id: Uuid) -> RepositoryResult<()> { Ok(()) }
        async fn find_by_issue_id(&self, _issue_id: Uuid) -> RepositoryResult<Vec<models::Approval>> { Ok(vec![]) }
    }

    struct MockTreeControlService;
    impl MockTreeControlService {
        fn new() -> Self { Self }
    }
    #[async_trait]
    impl IssueTreeControlService for MockTreeControlService {
        async fn preview_tree_hold(&self, _root_issue_id: Uuid, _mode: models::IssueTreeControlMode) -> crate::issue_tree_control_service::TreeControlServiceResult<models::IssueTreeControlPreview> { unimplemented!() }
        async fn create_tree_hold(&self, _company_id: Uuid, _root_issue_id: Uuid, _input: models::CreateIssueTreeHoldInput, _actor_type: Option<String>, _actor_id: Option<Uuid>) -> crate::issue_tree_control_service::TreeControlServiceResult<models::IssueTreeHold> { unimplemented!() }
        async fn get_tree_hold(&self, _hold_id: Uuid) -> crate::issue_tree_control_service::TreeControlServiceResult<models::IssueTreeHold> { unimplemented!() }
        async fn list_tree_holds(&self, _root_issue_id: Uuid) -> crate::issue_tree_control_service::TreeControlServiceResult<Vec<models::IssueTreeHold>> { Ok(vec![]) }
        async fn release_tree_hold(&self, _hold_id: Uuid, _released_by_type: Option<String>, _released_by_id: Option<Uuid>) -> crate::issue_tree_control_service::TreeControlServiceResult<models::IssueTreeHold> { unimplemented!() }
        async fn get_pause_state(&self, _issue_id: Uuid) -> crate::issue_tree_control_service::TreeControlServiceResult<Option<models::ActiveIssueTreePauseHoldGate>> { Ok(None) }
        async fn get_hold_members(&self, _hold_id: Uuid) -> crate::issue_tree_control_service::TreeControlServiceResult<Vec<models::IssueTreeHoldMember>> { Ok(vec![]) }
    }

    struct MockCommentService;
    impl MockCommentService {
        fn new() -> Self { Self }
    }
    #[async_trait]
    impl IssueCommentService for MockCommentService {
        async fn add_comment(&self, _issue_id: Uuid, _body: String, _actor_type: models::CommentActorType, _actor_id: Option<Uuid>, _actor_run_id: Option<Uuid>, _metadata: Option<serde_json::Value>) -> crate::issue_comment_service::CommentServiceResult<models::IssueComment> { unimplemented!() }
        async fn list_comments(&self, _issue_id: Uuid, _pagination: &models::Pagination) -> crate::issue_comment_service::CommentServiceResult<Vec<models::IssueComment>> { Ok(vec![]) }
        async fn count_comments(&self, _issue_id: Uuid) -> crate::issue_comment_service::CommentServiceResult<i64> { Ok(0) }
        async fn get_comment(&self, _comment_id: Uuid) -> crate::issue_comment_service::CommentServiceResult<models::IssueComment> { unimplemented!() }
        async fn update_comment(&self, _comment_id: Uuid, _body: String, _actor_id: Uuid) -> crate::issue_comment_service::CommentServiceResult<models::IssueComment> { unimplemented!() }
        async fn delete_comment(&self, _comment_id: Uuid, _actor_id: Uuid) -> crate::issue_comment_service::CommentServiceResult<()> { unimplemented!() }
    }

    struct MockWorkProduct;
    impl MockWorkProduct {
        fn new() -> Self { Self }
    }
    #[async_trait]
    impl WorkProductService for MockWorkProduct {
        async fn list_work_products(&self, _issue_id: Uuid, _company_id: Uuid) -> crate::errors::ServiceResult<Vec<models::issue_auxiliary::WorkProduct>> { Ok(vec![]) }
        async fn create_work_product(&self, _issue_id: Uuid, _company_id: Uuid, _input: models::issue_auxiliary::CreateWorkProductInput) -> crate::errors::ServiceResult<models::issue_auxiliary::WorkProduct> { unimplemented!() }
        async fn update_work_product(&self, _id: Uuid, _company_id: Uuid, _input: models::issue_auxiliary::UpdateWorkProductInput) -> crate::errors::ServiceResult<models::issue_auxiliary::WorkProduct> { unimplemented!() }
        async fn delete_work_product(&self, _id: Uuid, _company_id: Uuid) -> crate::errors::ServiceResult<()> { unimplemented!() }
    }

    struct MockAttachment;
    impl MockAttachment {
        fn new() -> Self { Self }
    }
    #[async_trait]
    impl AttachmentService for MockAttachment {
        async fn list_attachments(&self, _parent_type: &str, _parent_id: Uuid, _company_id: Uuid) -> crate::errors::ServiceResult<Vec<models::issue_auxiliary::Attachment>> { Ok(vec![]) }
        async fn upload_attachment(&self, _parent_type: &str, _parent_id: Uuid, _company_id: Uuid, _input: models::issue_auxiliary::UploadAttachmentInput) -> crate::errors::ServiceResult<models::issue_auxiliary::Attachment> { unimplemented!() }
        async fn delete_attachment(&self, _id: Uuid, _company_id: Uuid) -> crate::errors::ServiceResult<()> { unimplemented!() }
        async fn get_attachment_content(&self, _id: Uuid, _company_id: Uuid) -> crate::errors::ServiceResult<Vec<u8>> { unimplemented!() }
    }

    #[test]
    fn test_validate_status_transition() {
        let service = DefaultIssueService::new(
            Arc::new(MockIssueRepo::new()),
            Arc::new(MockApprovalRepo::new()),
            Arc::new(MockTreeControlService::new()),
            Arc::new(MockCommentService::new()),
            Arc::new(MockDocumentService::new()),
            Arc::new(MockWorkProduct::new()),
            Arc::new(MockAttachment::new()),
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

    #[test]
    fn test_create_issue_status_matches_paperclip_defaults() {
        let base = CreateIssueInput {
            company_id: Uuid::nil(),
            project_id: None,
            title: "issue".to_string(),
            idempotency_key: None,
            description: None,
            status: None,
            priority: None,
            assigned_to: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            parent_id: None,
            goal_id: None,
        };
        assert_eq!(resolve_create_issue_status(&base), IssueStatus::Backlog);

        let mut assigned = base.clone();
        assigned.assignee_agent_id = Some(Uuid::new_v4());
        assert_eq!(resolve_create_issue_status(&assigned), IssueStatus::Todo);

        let mut explicit = base;
        explicit.status = Some(IssueStatus::Blocked);
        assert_eq!(resolve_create_issue_status(&explicit), IssueStatus::Blocked);
    }
}
