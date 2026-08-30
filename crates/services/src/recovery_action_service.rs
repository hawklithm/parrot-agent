use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use models::{CreateRecoveryActionInput, Issue, RecoveryAction, ResolveRecoveryActionInput};
use repositories::{IssueRepository, RecoveryActionRepository, RepositoryError};

/// Recovery action service for managing issue recovery actions
#[async_trait]
pub trait RecoveryActionService: Send + Sync {
    /// Create a new recovery action for an issue
    async fn create(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        input: &CreateRecoveryActionInput,
    ) -> Result<RecoveryAction, String>;

    /// List recovery actions for an issue
    async fn list_by_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String>;

    /// List pending recovery actions
    async fn list_pending(
        &self,
        company_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RecoveryAction>, String>;

    /// Resolve a specific recovery action
    async fn resolve(
        &self,
        company_id: Uuid,
        source_issue_id: Uuid,
        action_id: Uuid,
        input: &ResolveRecoveryActionInput,
    ) -> Result<RecoveryAction, String>;

    /// Reconcile recovery actions for an issue and its ancestors.
    /// Compares current issue state vs expected recovery outcome.
    /// - If issue is fixed: resolve matching actions
    /// - If still failing: keep or re-trigger
    async fn reconcile_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String>;

    /// Resolve all active recovery actions for an issue (when issue is resolved)
    async fn resolve_active_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String>;
}

/// Default implementation of RecoveryActionService
pub struct DefaultRecoveryActionService {
    recovery_repo: Arc<dyn RecoveryActionRepository>,
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultRecoveryActionService {
    pub fn new(
        recovery_repo: Arc<dyn RecoveryActionRepository>,
        issue_repo: Arc<dyn IssueRepository>,
    ) -> Self {
        Self {
            recovery_repo,
            issue_repo,
        }
    }

    /// Determine if a recovery action should be resolved based on current issue state.
    /// The reconcile algorithm:
    /// 1. Get current issue status
    /// 2. Check if the action type matches a resolvable condition
    /// 3. If issue is in a terminal/healthy state, resolve matching actions
    fn should_resolve_action(&self, action: &RecoveryAction, issue: &Issue) -> bool {
        match action.kind.as_str() {
            // Blocked recovery: if issue is no longer blocked, resolve
            "unblock" | "issue_graph_liveness" => issue.status != models::IssueStatus::Blocked,

            // Stale execution recovery: if issue is no longer in_progress with stale lock, resolve
            "stale_execution" => {
                issue.status != models::IssueStatus::InProgress
                    || issue.execution_locked_at.is_none()
            }

            // Missing assignee recovery: if issue now has an assignee, resolve
            "missing_assignee" => {
                issue.assignee_agent_id.is_some() || issue.assignee_user_id.is_some()
            }

            // General catch-all: resolve if issue is in a terminal state
            "general" => {
                matches!(
                    issue.status,
                    models::IssueStatus::Done | models::IssueStatus::Cancelled
                )
            }

            // Unknown action type: don't auto-resolve
            _ => false,
        }
    }
}

#[async_trait]
impl RecoveryActionService for DefaultRecoveryActionService {
    async fn create(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        input: &CreateRecoveryActionInput,
    ) -> Result<RecoveryAction, String> {
        // Verify issue exists
        let issue = self
            .issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| format!("Failed to verify issue: {}", e))?
            .ok_or_else(|| format!("Issue {} not found", issue_id))?;

        if issue.company_id != company_id {
            return Err(format!("Issue {} does not belong to company {}", issue_id, company_id));
        }

        if let Some(recovery_issue_id) = input.recovery_issue_id {
            let recovery_issue = self
                .issue_repo
                .get_by_id(recovery_issue_id)
                .await
                .map_err(|e| format!("Failed to verify recovery issue: {}", e))?
                .ok_or_else(|| format!("Recovery issue {} not found", recovery_issue_id))?;
            if recovery_issue.company_id != company_id {
                return Err(format!(
                    "Recovery issue {} does not belong to company {}",
                    recovery_issue_id, company_id
                ));
            }
        }

        if input.cause.trim().is_empty()
            || input.fingerprint.trim().is_empty()
            || input.next_action.trim().is_empty()
        {
            return Err("Recovery action cause, fingerprint, and nextAction are required".to_string());
        }
        if let Some(owner_type) = input.owner_type.as_deref() {
            if !matches!(owner_type, "agent" | "user" | "board" | "system") {
                return Err(format!("Unsupported recovery action owner type: {owner_type}"));
            }
        }

        self.recovery_repo
            .create(company_id, issue_id, input)
            .await
            .map_err(|e| format!("Failed to create recovery action: {}", e))
    }

    async fn list_by_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String> {
        self.recovery_repo
            .list_by_issue(company_id, issue_id)
            .await
            .map_err(|e| format!("Failed to list recovery actions: {}", e))
    }

    async fn list_pending(
        &self,
        company_id: Uuid,
        limit: i64,
    ) -> Result<Vec<RecoveryAction>, String> {
        self.recovery_repo
            .list_pending(company_id, limit)
            .await
            .map_err(|e| format!("Failed to list pending recovery actions: {}", e))
    }

    async fn resolve(
        &self,
        company_id: Uuid,
        source_issue_id: Uuid,
        action_id: Uuid,
        input: &ResolveRecoveryActionInput,
    ) -> Result<RecoveryAction, String> {
        if !matches!(input.status.as_str(), "resolved" | "cancelled") {
            return Err("Recovery action status must be resolved or cancelled".to_string());
        }
        if !matches!(
            input.outcome.as_str(),
            "restored"
                | "handed_back"
                | "owner_completed"
                | "delegated"
                | "false_positive"
                | "blocked"
                | "escalated"
                | "cancelled"
        ) {
            return Err(format!("Unsupported recovery action outcome: {}", input.outcome));
        }
        self.recovery_repo
            .resolve(company_id, source_issue_id, action_id, input)
            .await
            .map_err(|e| format!("Failed to resolve recovery action: {}", e))
    }

    async fn reconcile_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String> {
        // Get the issue
        let issue = self
            .issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| format!("Failed to get issue: {}", e))?
            .ok_or_else(|| format!("Issue {} not found", issue_id))?;

        // Get pending recovery actions for this issue and ancestors
        let pending_actions = self
            .recovery_repo
            .reconcile_for_issue_and_ancestors(company_id, issue_id)
            .await
            .map_err(|e| format!("Failed to get pending recovery actions: {}", e))?;

        // Filter: only resolve actions whose conditions are met
        let mut resolved = Vec::new();
        for action in &pending_actions {
            if self.should_resolve_action(action, &issue) {
                let resolved_action = self
                    .recovery_repo
                    .resolve(
                        action.company_id,
                        action.source_issue_id,
                        action.id,
                        &ResolveRecoveryActionInput {
                            status: "resolved".to_string(),
                            outcome: "restored".to_string(),
                            resolution_note: Some("Recovery condition is no longer present".to_string()),
                            resolved_at: None,
                        },
                    )
                    .await
                    .map_err(|e| format!("Failed to resolve action {}: {}", action.id, e))?;
                resolved.push(resolved_action);
            }
        }

        Ok(resolved)
    }

    async fn resolve_active_for_issue(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, String> {
        self.recovery_repo
        .resolve_active_for_issue(company_id, issue_id)
            .await
            .map_err(|e| format!("Failed to resolve active recovery actions: {}", e))
    }
}

#[cfg(all(test, feature = "legacy-unit-tests"))]
mod tests {
    use super::*;
    use models::issue::IssueStatus;

    #[test]
    fn test_should_resolve_action() {
        let service = DefaultRecoveryActionService::new(
            Arc::new(MockRecoveryRepo::new()),
            Arc::new(MockIssueRepo::new()),
        );

        // Create a resolved issue (Done)
        let done_issue = models::Issue {
            status: IssueStatus::Done,
            execution_locked_at: None,
            assignee_agent_id: Some(Uuid::new_v4()),
            ..Default::default()
        };

        // Create blocked issue
        let blocked_issue = models::Issue {
            status: IssueStatus::Blocked,
            execution_locked_at: None,
            assignee_agent_id: None,
            ..Default::default()
        };

        // Create unblock action for done issue -> should resolve
        let unblock_action = RecoveryAction {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            issue_id: Uuid::nil(),
            source_issue_id: Uuid::nil(),
            recovery_issue_id: None,
            kind: "unblock".to_string(),
            status: "active".to_string(),
            owner_type: "agent".to_string(),
            owner_agent_id: None,
            owner_user_id: None,
            previous_owner_agent_id: None,
            return_owner_agent_id: None,
            cause: "blocked".to_string(),
            fingerprint: "test".to_string(),
            evidence: serde_json::json!({}),
            next_action: "unblock".to_string(),
            wake_policy: None,
            monitor_policy: None,
            attempt_count: 0,
            max_attempts: None,
            timeout_at: None,
            last_attempt_at: None,
            outcome: None,
            resolution_note: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(service.should_resolve_action(&unblock_action, &done_issue));
        assert!(!service.should_resolve_action(&unblock_action, &blocked_issue));
    }
}

// Mock implementations for testing
#[allow(dead_code)]
struct MockRecoveryRepo;
#[allow(dead_code)]
impl MockRecoveryRepo {
    fn new() -> Self {
        Self
    }
}

#[allow(dead_code)]
struct MockIssueRepo;
#[allow(dead_code)]
impl MockIssueRepo {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RecoveryActionRepository for MockRecoveryRepo {
    async fn create(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
        _input: &CreateRecoveryActionInput,
    ) -> Result<RecoveryAction, RepositoryError> {
        unimplemented!()
    }
    async fn list_by_issue(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, RepositoryError> {
        unimplemented!()
    }
    async fn list_pending(
        &self,
        _company_id: Uuid,
        _limit: i64,
    ) -> Result<Vec<RecoveryAction>, RepositoryError> {
        unimplemented!()
    }
    async fn resolve(
        &self,
        _company_id: Uuid,
        _source_issue_id: Uuid,
        _action_id: Uuid,
        _input: &ResolveRecoveryActionInput,
    ) -> Result<RecoveryAction, RepositoryError> {
        unimplemented!()
    }
    async fn reconcile_for_issue_and_ancestors(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, RepositoryError> {
        unimplemented!()
    }
    async fn resolve_active_for_issue(
        &self,
        _company_id: Uuid,
        _issue_id: Uuid,
    ) -> Result<Vec<RecoveryAction>, RepositoryError> {
        unimplemented!()
    }
}

#[async_trait]
impl IssueRepository for MockIssueRepo {
    async fn get_by_id(&self, _id: Uuid) -> Result<Option<Issue>, RepositoryError> {
        Ok(None)
    }
    async fn list_by_company(
        &self,
        _company_id: Uuid,
        _filter: &models::IssueQueryFilter,
        _pagination: &models::Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn count_by_company(
        &self,
        _company_id: Uuid,
        _filter: &models::IssueQueryFilter,
    ) -> Result<i64, RepositoryError> {
        unimplemented!()
    }
    async fn create(&self, _input: models::CreateIssueInput) -> Result<Issue, RepositoryError> {
        unimplemented!()
    }
    async fn update(
        &self,
        _id: Uuid,
        _input: models::UpdateIssueInput,
    ) -> Result<Issue, RepositoryError> {
        unimplemented!()
    }
    async fn delete(&self, _id: Uuid) -> Result<(), RepositoryError> {
        unimplemented!()
    }
    async fn search(
        &self,
        _company_id: Uuid,
        _query: &str,
        _pagination: &models::Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn get_by_identifier(&self, _identifier: &str) -> Result<Option<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn list_by_parent(
        &self,
        _parent_id: Uuid,
        _pagination: &models::Pagination,
    ) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn get_by_ids(&self, _ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
    async fn list_ancestors(&self, _issue_id: Uuid) -> Result<Vec<Issue>, RepositoryError> {
        unimplemented!()
    }
}
