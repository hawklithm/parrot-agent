use async_trait::async_trait;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};
use std::sync::Arc;

use models::{Issue, UpdateIssueInput};
use repositories::IssueRepository;
use crate::errors::ServiceError;

/// Lock timeout configuration
#[derive(Debug, Clone)]
pub struct ExecutionLockConfig {
    /// Maximum time a lock can be held before automatic release
    pub lock_timeout_seconds: i64,
    /// Interval for zombie lock cleanup (seconds)
    pub cleanup_interval_seconds: i64,
}

impl Default for ExecutionLockConfig {
    fn default() -> Self {
        Self {
            lock_timeout_seconds: 3600,   // 1 hour
            cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

/// Execution Lock result
#[derive(Debug, Clone)]
pub struct ExecutionLockResult {
    pub issue: Issue,
    pub lock_acquired: bool,
    pub previous_run_id: Option<Uuid>,
}

/// Issue Execution Lock Service
/// Manages distributed execution locks for issues using PostgreSQL advisory locks
#[async_trait]
pub trait IssueExecutionLockService: Send + Sync {
    /// Acquire execution lock for an issue
    /// Returns the updated issue and whether the lock was newly acquired
    async fn acquire_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        execution_run_id: Uuid,
    ) -> Result<ExecutionLockResult, ServiceError>;

    /// Release execution lock
    async fn release_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        execution_run_id: Uuid,
    ) -> Result<Issue, ServiceError>;

    /// Check if issue is locked
    async fn is_locked(&self, issue_id: Uuid, company_id: Uuid) -> Result<bool, ServiceError>;

    /// Get the current lock holder
    async fn get_lock_holder(&self, issue_id: Uuid, company_id: Uuid) -> Result<Option<Uuid>, ServiceError>;

    /// Force release a lock (admin operation)
    async fn force_release_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        admin_user_id: Uuid,
    ) -> Result<Issue, ServiceError>;

    /// Detect and cleanup zombie locks (locks that have timed out)
    async fn cleanup_zombie_locks(&self, company_id: Uuid) -> Result<Vec<Uuid>, ServiceError>;

    /// Get lock diagnostics for an issue
    async fn get_lock_diagnostics(&self, issue_id: Uuid, company_id: Uuid) -> Result<ExecutionLockDiagnostics, ServiceError>;
}

/// Execution lock diagnostics
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionLockDiagnostics {
    pub is_locked: bool,
    pub locked_at: Option<DateTime<Utc>>,
    pub execution_run_id: Option<Uuid>,
    pub lock_age_seconds: Option<i64>,
    pub is_expired: bool,
    pub locked_duration: Option<String>,
}

/// Default implementation using database-level fields
pub struct DefaultIssueExecutionLockService {
    issue_repo: Arc<dyn IssueRepository>,
    config: ExecutionLockConfig,
}

impl DefaultIssueExecutionLockService {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        config: ExecutionLockConfig,
    ) -> Self {
        Self {
            issue_repo,
            config,
        }
    }

    /// Verify the issue belongs to the company
    async fn verify_company_access(
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
            return Err(ServiceError::Forbidden("Access denied to issue from different company".to_string()));
        }

        Ok(issue)
    }

    /// Check if the lock is expired based on locked_at timestamp
    fn is_lock_expired(&self, locked_at: &DateTime<Utc>) -> bool {
        let now = Utc::now();
        now.signed_duration_since(*locked_at) > Duration::seconds(self.config.lock_timeout_seconds)
    }
}

#[async_trait]
impl IssueExecutionLockService for DefaultIssueExecutionLockService {
    async fn acquire_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        execution_run_id: Uuid,
    ) -> Result<ExecutionLockResult, ServiceError> {
        let issue = self.verify_company_access(issue_id, company_id).await?;

        // Check if already locked
        if let Some(locked_at) = issue.execution_locked_at {
            if !self.is_lock_expired(&locked_at) {
                // Lock is still valid — conflict
                return Err(ServiceError::Conflict(format!(
                    "Issue {} is already locked for execution (run_id={}, locked_at={})",
                    issue_id,
                    issue.execution_run_id.map(|id| id.to_string()).unwrap_or_default(),
                    locked_at
                )));
            }
            // Lock is expired — we can override it
            tracing::warn!(
                "Overriding expired execution lock for issue={}, previous_run_id={:?}, expired_at={}",
                issue_id, issue.execution_run_id, locked_at
            );
        }

        let previous_run_id = issue.execution_run_id;

        // Acquire lock by setting execution_locked_at and execution_run_id
        let update = UpdateIssueInput {
            title: None,
            description: None,
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            monitor_next_check_at: None,
            monitor_last_triggered_at: None,
            monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: Some(Utc::now()),
            execution_run_id: Some(execution_run_id),
        };

        let updated_issue = self.issue_repo
            .update(issue_id, update)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to acquire execution lock: {}", e)))?;

        Ok(ExecutionLockResult {
            issue: updated_issue,
            lock_acquired: true,
            previous_run_id,
        })
    }

    async fn release_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        execution_run_id: Uuid,
    ) -> Result<Issue, ServiceError> {
        let issue = self.verify_company_access(issue_id, company_id).await?;

        // Verify the caller owns the lock
        if issue.execution_run_id != Some(execution_run_id) {
            return Err(ServiceError::Forbidden(format!(
                "Cannot release lock: run_id {} does not match current lock holder {:?}",
                execution_run_id, issue.execution_run_id
            )));
        }

        // Release lock
        let update = UpdateIssueInput {
            title: None,
            description: None,
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            monitor_next_check_at: None,
            monitor_last_triggered_at: None,
            monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
        };

        self.issue_repo
            .update(issue_id, update)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to release execution lock: {}", e)))
    }

    async fn is_locked(&self, issue_id: Uuid, company_id: Uuid) -> Result<bool, ServiceError> {
        let issue = self.verify_company_access(issue_id, company_id).await?;
        match issue.execution_locked_at {
            Some(locked_at) => Ok(!self.is_lock_expired(&locked_at)),
            None => Ok(false),
        }
    }

    async fn get_lock_holder(&self, issue_id: Uuid, company_id: Uuid) -> Result<Option<Uuid>, ServiceError> {
        let issue = self.verify_company_access(issue_id, company_id).await?;
        if let Some(locked_at) = issue.execution_locked_at {
            if !self.is_lock_expired(&locked_at) {
                return Ok(issue.execution_run_id);
            }
        }
        Ok(None)
    }

    async fn force_release_lock(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        _admin_user_id: Uuid,
    ) -> Result<Issue, ServiceError> {
        let _issue = self.verify_company_access(issue_id, company_id).await?;

        let update = UpdateIssueInput {
            title: None,
            description: None,
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            work_mode: None,
            responsible_user_id: None,
            source_trust: None,
            monitor_scheduled_by: None,
            monitor_notes: None,
            monitor_next_check_at: None,
            monitor_last_triggered_at: None,
            monitor_attempt_count: None,
            hidden_at: None,
            execution_workspace_preference: None,
            execution_workspace_settings: None,
            execution_policy: None,
            execution_state: None,
            execution_locked_at: None,
            execution_run_id: None,
        };

        self.issue_repo
            .update(issue_id, update)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to force release lock: {}", e)))
    }

    async fn cleanup_zombie_locks(&self, company_id: Uuid) -> Result<Vec<Uuid>, ServiceError> {
        // List issues with active locks and check for expired ones
        // In production: query with filter execution_locked_at IS NOT NULL
        // For now, we return empty — this would be driven by a background scheduler
        tracing::info!("Zombie lock cleanup triggered for company_id={}", company_id);

        // TODO: Implement paginated query of locked issues and release expired ones
        // This requires a repository method to list issues with active locks
        Ok(vec![])
    }

    async fn get_lock_diagnostics(&self, issue_id: Uuid, company_id: Uuid) -> Result<ExecutionLockDiagnostics, ServiceError> {
        let issue = self.verify_company_access(issue_id, company_id).await?;

        let is_locked = match issue.execution_locked_at {
            Some(locked_at) => !self.is_lock_expired(&locked_at),
            None => false,
        };

        let lock_age = issue.execution_locked_at.map(|locked_at| {
            Utc::now().signed_duration_since(locked_at)
        });

        Ok(ExecutionLockDiagnostics {
            is_locked,
            locked_at: issue.execution_locked_at,
            execution_run_id: issue.execution_run_id,
            lock_age_seconds: lock_age.map(|d| d.num_seconds()),
            is_expired: match issue.execution_locked_at {
                Some(locked_at) => self.is_lock_expired(&locked_at),
                None => false,
            },
            locked_duration: lock_age.map(|d| format!("{}s", d.num_seconds())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_mock_issue(company_id: Uuid, locked: bool) -> Issue {
        Issue {
            id: Uuid::new_v4(),
            company_id,
            project_id: None,
            project_workspace_id: None,
            goal_id: None,
            parent_id: None,
            title: "Test Issue".to_string(),
            description: None,
            status: models::IssueStatus::Todo,
            work_mode: models::IssueWorkMode::Normal,
            priority: models::IssuePriority::Medium,
            assignee_agent_id: None,
            assignee_user_id: None,
            assigned_to: None,
            checkout_run_id: None,
            execution_run_id: if locked { Some(Uuid::new_v4()) } else { None },
            execution_agent_name_key: None,
            execution_locked_at: if locked { Some(Utc::now() - Duration::minutes(5)) } else { None },
            created_by_agent_id: None,
            created_by_user_id: None,
            responsible_user_id: None,
            issue_number: None,
            identifier: None,
            origin_kind: None,
            origin_id: None,
            origin_run_id: None,
            origin_fingerprint: None,
            request_depth: 0,
            billing_code: None,
            execution_policy: None,
            execution_state: None,
            monitor_next_check_at: None,
            monitor_last_triggered_at: None,
            monitor_attempt_count: None,
            monitor_notes: None,
            monitor_scheduled_by: None,
            execution_workspace_id: None,
            execution_workspace_preference: None,
            started_at: None,
            completed_at: None,
            cancelled_at: None,
            hidden_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_lock_expired_check() {
        let config = ExecutionLockConfig::default();
        let issue_repo = Arc::new(crate::MockIssueRepository::new());
        let service = DefaultIssueExecutionLockService::new(issue_repo, config);

        let fresh_lock = Utc::now() - Duration::minutes(30);
        assert!(!service.is_lock_expired(&fresh_lock));

        let old_lock = Utc::now() - Duration::hours(2);
        assert!(service.is_lock_expired(&old_lock));
    }
}
