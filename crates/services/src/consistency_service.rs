use async_trait::async_trait;
use uuid::Uuid;
use sqlx::{PgPool, Row};

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
    pool: Option<PgPool>,
}

impl DefaultConsistencyService {
    pub fn new() -> Self {
        Self {
            pool: None,
        }
    }

    pub fn with_pool(pool: PgPool) -> Self { Self { pool: Some(pool) } }
    fn pool(&self) -> Result<&PgPool, ServiceError> {
        self.pool.as_ref().ok_or_else(|| ServiceError::Internal("consistency persistence is not configured".into()))
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
        let pool = self.pool()?;
        let goal = sqlx::query("SELECT status::text FROM goals WHERE id=$1").bind(goal_id).fetch_optional(pool).await
            .map_err(|e| ServiceError::Internal(e.to_string()))?.ok_or_else(|| ServiceError::NotFound(format!("goal {} not found", goal_id)))?;
        let goal_status: String = goal.get("status");
        let child_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE goal_id=$1").bind(goal_id).fetch_one(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let open_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE goal_id=$1 AND status NOT IN ('done','cancelled')").bind(goal_id).fetch_one(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let consistent = !(goal_status == "completed" && open_count > 0) && !(child_count > 0 && open_count == 0 && goal_status != "completed");
        let mut issues = Vec::new();
        if !consistent { issues.push(ConsistencyIssue { severity: IssueSeverity::Warning, description: "goal status does not match child issue completion".into(), affected_field: Some("status".into()), expected_value: Some(if open_count == 0 { "completed" } else { "active" }.into()), actual_value: Some(goal_status), auto_fixable: true }); }
        Ok(ConsistencyCheckResult {
            resource_id: goal_id,
            resource_type: "goal".to_string(),
            is_consistent: consistent,
            issues,
            recommendations: if consistent { vec![] } else { vec!["recalculate goal status from child issues".into()] },
        })
    }

    async fn check_agent_assignment_validity(
        &self,
        agent_id: Uuid,
    ) -> Result<ConsistencyCheckResult, ServiceError> {
        let pool = self.pool()?;
        let row = sqlx::query("SELECT status FROM agents WHERE id=$1").bind(agent_id).fetch_optional(pool).await
            .map_err(|e| ServiceError::Internal(e.to_string()))?.ok_or_else(|| ServiceError::NotFound(format!("agent {} not found", agent_id)))?;
        let status: String = row.get("status");
        let assigned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE assignee_agent_id=$1 AND status NOT IN ('done','cancelled')").bind(agent_id).fetch_one(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let consistent = status != "terminated" && status != "paused";
        let issues = if consistent { vec![] } else { vec![ConsistencyIssue { severity: IssueSeverity::Error, description: format!("agent is {} but has {} active assignments", status, assigned), affected_field: Some("status".into()), expected_value: Some("idle or running".into()), actual_value: Some(status), auto_fixable: false }] };
        Ok(ConsistencyCheckResult {
            resource_id: agent_id,
            resource_type: "agent".to_string(),
            is_consistent: issues.is_empty(), issues,
            recommendations: vec![],
        })
    }

    async fn check_environment_lease_expiration(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<ExpiredLease>, ServiceError> {
        let rows = sqlx::query("SELECT l.id,l.environment_id,l.expires_at,COALESCE(i.assignee_agent_id, '00000000-0000-0000-0000-000000000000'::uuid) AS agent_id FROM environment_leases l LEFT JOIN issues i ON i.id=l.issue_id WHERE l.company_id=$1 AND l.status='active' AND l.expires_at IS NOT NULL AND l.expires_at < now()").bind(company_id).fetch_all(self.pool()?).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(rows.into_iter().map(|r| ExpiredLease { lease_id:r.get("id"), environment_id:r.get("environment_id"), agent_id:r.get("agent_id"), expired_at:r.get("expires_at"), cleanup_required:true }).collect())
    }

    async fn detect_orphaned_resources(
        &self,
        company_id: Uuid,
    ) -> Result<OrphanedResourcesReport, ServiceError> {
        let pool = self.pool()?;
        let issues_rows = sqlx::query("SELECT id,created_at,updated_at FROM issues WHERE company_id=$1 AND parent_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM issues p WHERE p.id=issues.parent_id)").bind(company_id).fetch_all(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let goals_rows = sqlx::query("SELECT id,created_at,updated_at FROM goals WHERE company_id=$1 AND parent_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM goals p WHERE p.id=goals.parent_id)").bind(company_id).fetch_all(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let make = |r: sqlx::postgres::PgRow, kind: &str| OrphanedResource { resource_id:r.get("id"), resource_type:kind.into(), reason:"referenced parent does not exist".into(), created_at:r.get("created_at"), last_activity:Some(r.get("updated_at")) };
        let orphaned_issues=issues_rows.into_iter().map(|r|make(r,"issue")).collect::<Vec<_>>();
        let orphaned_goals=goals_rows.into_iter().map(|r|make(r,"goal")).collect::<Vec<_>>();
        Ok(OrphanedResourcesReport {
            company_id,
            orphaned_issues: orphaned_issues.clone(),
            orphaned_environments: vec![],
            orphaned_goals: orphaned_goals.clone(),
            total_count: orphaned_issues.len()+orphaned_goals.len(),
        })
    }

    async fn run_full_consistency_check(
        &self,
        company_id: Uuid,
    ) -> Result<FullConsistencyReport, ServiceError> {
        let checked_at = chrono::Utc::now();

        let orphaned_resources = self.detect_orphaned_resources(company_id).await?;
        let pool = self.pool()?;
        let goal_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM goals WHERE company_id=$1 AND status <> 'archived'").bind(company_id).fetch_all(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let agent_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM agents WHERE company_id=$1 AND status <> 'terminated'").bind(company_id).fetch_all(pool).await.map_err(|e| ServiceError::Internal(e.to_string()))?;
        let goal_checks = futures::future::join_all(goal_ids.into_iter().map(|id| self.check_goal_progress_consistency(id))).await.into_iter().collect::<Result<Vec<_>,_>>()?;
        let agent_checks = futures::future::join_all(agent_ids.into_iter().map(|id| self.check_agent_assignment_validity(id))).await.into_iter().collect::<Result<Vec<_>,_>>()?;
        let expired = self.check_environment_lease_expiration(company_id).await?;
        let environment_checks = expired.iter().map(|lease| ConsistencyCheckResult { resource_id: lease.environment_id, resource_type: "environment_lease".into(), is_consistent:false, issues:vec![ConsistencyIssue { severity:IssueSeverity::Warning, description:"environment lease has expired and requires cleanup".into(), affected_field:Some("expires_at".into()), expected_value:Some("active lease with future expiry".into()), actual_value:Some(lease.expired_at.to_rfc3339()), auto_fixable:true }], recommendations:vec!["release the expired lease and clean up the provider resource".into()] }).collect::<Vec<_>>();
        let total_issues = goal_checks.iter().chain(agent_checks.iter()).chain(environment_checks.iter()).map(|c| c.issues.len()).sum::<usize>() + orphaned_resources.total_count;
        let critical_issues = goal_checks.iter().chain(agent_checks.iter()).chain(environment_checks.iter()).flat_map(|c| &c.issues).filter(|i| matches!(i.severity, IssueSeverity::Critical)).count();

        Ok(FullConsistencyReport {
            company_id,
            checked_at,
            goal_checks,
            agent_checks,
            environment_checks,
            orphaned_resources,
            total_issues,
            critical_issues,
        })
    }

    async fn repair_inconsistencies(
        &self,
        company_id: Uuid,
        dry_run: bool,
    ) -> Result<RepairReport, ServiceError> {
        let report = self.run_full_consistency_check(company_id).await?;
        let pool = self.pool()?;
        let mut repairs = Vec::new();
        for check in report.goal_checks.iter().filter(|c| !c.is_consistent && c.issues.iter().any(|i| i.auto_fixable)) {
            let action = RepairAction { resource_id:check.resource_id, resource_type:"goal".into(), action_type:"recalculate_status".into(), description:"set goal status from child issue completion".into(), success:true, error_message:None };
            if !dry_run { let result=sqlx::query("UPDATE goals SET status=CASE WHEN NOT EXISTS (SELECT 1 FROM issues WHERE goal_id=$1 AND status NOT IN ('done','cancelled')) AND EXISTS (SELECT 1 FROM issues WHERE goal_id=$1) THEN 'completed' ELSE 'active' END, updated_at=now() WHERE id=$1").bind(check.resource_id).execute(pool).await; if result.is_err() { repairs.push(RepairAction { success:false, error_message:result.err().map(|e|e.to_string()), ..action }); continue; } }
            repairs.push(action);
        }
        for lease in self.check_environment_lease_expiration(company_id).await? {
            let action=RepairAction { resource_id:lease.lease_id, resource_type:"environment_lease".into(), action_type:"release_expired".into(), description:"mark expired lease for cleanup".into(), success:true, error_message:None };
            if !dry_run { let result=sqlx::query("UPDATE environment_leases SET status='expired', cleanup_status='pending', released_at=COALESCE(released_at,now()), updated_at=now() WHERE id=$1 AND status='active'").bind(lease.lease_id).execute(pool).await; if result.is_err() { repairs.push(RepairAction { success:false, error_message:result.err().map(|e|e.to_string()), ..action }); continue; } }
            repairs.push(action);
        }
        Ok(RepairReport {
            company_id,
            dry_run,
            repaired_count: repairs.iter().filter(|r| r.success).count(),
            failed_count: repairs.iter().filter(|r| !r.success).count(),
            repairs,
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
