use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::ServiceError;

/// Consistency service for cross-module data integrity checks
#[async_trait]
pub trait ConsistencyService: Send + Sync {
    /// Check goal progress consistency against child issues
    async fn check_goal_progress_consistency(
        &self,
        goal_id: Uuid,
    ) -> Result<ConsistencyCheckResult, ServiceError>;

    /// Check agent assignment validity (agent exists, not terminated, has capacity)
    async fn check_agent_assignment_validity(
        &self,
        agent_id: Uuid,
    ) -> Result<ConsistencyCheckResult, ServiceError>;

    /// Check environment lease expiration and cleanup stale leases
    async fn check_environment_lease_expiration(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<ExpiredLease>, ServiceError>;

    /// Detect orphaned resources (issues without parent, environments without agent)
    async fn detect_orphaned_resources(
        &self,
        company_id: Uuid,
    ) -> Result<OrphanedResourcesReport, ServiceError>;

    /// Run full consistency check across all modules
    async fn run_full_consistency_check(
        &self,
        company_id: Uuid,
    ) -> Result<FullConsistencyReport, ServiceError>;

    /// Repair inconsistencies (auto-fix where possible)
    async fn repair_inconsistencies(
        &self,
        company_id: Uuid,
        dry_run: bool,
    ) -> Result<RepairReport, ServiceError>;
}

/// Consistency check result
#[derive(Debug, Clone)]
pub struct ConsistencyCheckResult {
    pub resource_id: Uuid,
    pub resource_type: String,
    pub is_consistent: bool,
    pub issues: Vec<ConsistencyIssue>,
    pub recommendations: Vec<String>,
}

/// Consistency issue
#[derive(Debug, Clone)]
pub struct ConsistencyIssue {
    pub severity: IssueSeverity,
    pub description: String,
    pub affected_field: Option<String>,
    pub expected_value: Option<String>,
    pub actual_value: Option<String>,
    pub auto_fixable: bool,
}

/// Issue severity
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Expired lease record
#[derive(Debug, Clone)]
pub struct ExpiredLease {
    pub lease_id: Uuid,
    pub environment_id: Uuid,
    pub agent_id: Uuid,
    pub expired_at: chrono::DateTime<chrono::Utc>,
    pub cleanup_required: bool,
}

/// Orphaned resources report
#[derive(Debug, Clone)]
pub struct OrphanedResourcesReport {
    pub company_id: Uuid,
    pub orphaned_issues: Vec<OrphanedResource>,
    pub orphaned_environments: Vec<OrphanedResource>,
    pub orphaned_goals: Vec<OrphanedResource>,
    pub total_count: usize,
}

/// Orphaned resource
#[derive(Debug, Clone)]
pub struct OrphanedResource {
    pub resource_id: Uuid,
    pub resource_type: String,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
}

/// Full consistency report
#[derive(Debug, Clone)]
pub struct FullConsistencyReport {
    pub company_id: Uuid,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub goal_checks: Vec<ConsistencyCheckResult>,
    pub agent_checks: Vec<ConsistencyCheckResult>,
    pub environment_checks: Vec<ConsistencyCheckResult>,
    pub orphaned_resources: OrphanedResourcesReport,
    pub total_issues: usize,
    pub critical_issues: usize,
}

/// Repair report
#[derive(Debug, Clone)]
pub struct RepairReport {
    pub company_id: Uuid,
    pub dry_run: bool,
    pub repaired_count: usize,
    pub failed_count: usize,
    pub repairs: Vec<RepairAction>,
}

/// Repair action
#[derive(Debug, Clone)]
pub struct RepairAction {
    pub resource_id: Uuid,
    pub resource_type: String,
    pub action_type: String,
    pub description: String,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Default implementation of ConsistencyService
pub struct DefaultConsistencyService {
    // In production: inject repositories for all modules
    _marker: std::marker::PhantomData<()>,
}

impl DefaultConsistencyService {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl Default for DefaultConsistencyService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConsistencyService for DefaultConsistencyService {
    async fn check_goal_progress_consistency(
        &self,
        goal_id: Uuid,
    ) -> Result<ConsistencyCheckResult, ServiceError> {
        // Placeholder: In production, query goal and child issues, recalculate progress
        Ok(ConsistencyCheckResult {
            resource_id: goal_id,
            resource_type: "goal".to_string(),
            is_consistent: true,
            issues: vec![],
            recommendations: vec![],
        })
    }

    async fn check_agent_assignment_validity(
        &self,
        agent_id: Uuid,
    ) -> Result<ConsistencyCheckResult, ServiceError> {
        // Placeholder: In production, check agent status, assigned issues count
        Ok(ConsistencyCheckResult {
            resource_id: agent_id,
            resource_type: "agent".to_string(),
            is_consistent: true,
            issues: vec![],
            recommendations: vec![],
        })
    }

    async fn check_environment_lease_expiration(
        &self,
        _company_id: Uuid,
    ) -> Result<Vec<ExpiredLease>, ServiceError> {
        // Placeholder: In production, query leases with expired_at < now
        Ok(vec![])
    }

    async fn detect_orphaned_resources(
        &self,
        company_id: Uuid,
    ) -> Result<OrphanedResourcesReport, ServiceError> {
        // Placeholder: In production, query resources without valid parents/owners
        Ok(OrphanedResourcesReport {
            company_id,
            orphaned_issues: vec![],
            orphaned_environments: vec![],
            orphaned_goals: vec![],
            total_count: 0,
        })
    }

    async fn run_full_consistency_check(
        &self,
        company_id: Uuid,
    ) -> Result<FullConsistencyReport, ServiceError> {
        let checked_at = chrono::Utc::now();

        // Run all checks
        let orphaned_resources = self.detect_orphaned_resources(company_id).await?;

        Ok(FullConsistencyReport {
            company_id,
            checked_at,
            goal_checks: vec![],
            agent_checks: vec![],
            environment_checks: vec![],
            orphaned_resources,
            total_issues: 0,
            critical_issues: 0,
        })
    }

    async fn repair_inconsistencies(
        &self,
        company_id: Uuid,
        dry_run: bool,
    ) -> Result<RepairReport, ServiceError> {
        // Placeholder: In production, run checks and auto-fix issues
        Ok(RepairReport {
            company_id,
            dry_run,
            repaired_count: 0,
            failed_count: 0,
            repairs: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_goal_progress_consistency() {
        let service = DefaultConsistencyService::new();
        let result = service
            .check_goal_progress_consistency(Uuid::new_v4())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_full_consistency_check() {
        let service = DefaultConsistencyService::new();
        let result = service
            .run_full_consistency_check(Uuid::new_v4())
            .await;
        assert!(result.is_ok());
        let report = result.unwrap();
        assert_eq!(report.total_issues, 0);
    }

    #[tokio::test]
    async fn test_repair_inconsistencies_dry_run() {
        let service = DefaultConsistencyService::new();
        let result = service
            .repair_inconsistencies(Uuid::new_v4(), true)
            .await;
        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.dry_run);
    }
}
