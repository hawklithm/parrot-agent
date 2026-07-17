use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ServiceResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixStrategy {
    AutoFix,
    ManualReview,
    AlertOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InconsistencyReport {
    pub id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub expected_state: serde_json::Value,
    pub actual_state: serde_json::Value,
    pub detected_at: DateTime<Utc>,
    pub fix_strategy: FixStrategy,
    pub fixed: bool,
}

#[async_trait]
pub trait ConsistencyChecker: Send + Sync {
    async fn check(&self, company_id: Uuid) -> ServiceResult<Vec<InconsistencyReport>>;
    async fn fix(&self, report: InconsistencyReport) -> ServiceResult<bool>;
    fn resource_type(&self) -> String;
}

pub struct IssueStateChecker;

#[async_trait]
impl ConsistencyChecker for IssueStateChecker {
    async fn check(&self, _company_id: Uuid) -> ServiceResult<Vec<InconsistencyReport>> {
        let reports = vec![];

        // Check 1: Issue.status = in_progress but no active RoutineRun or HeartbeatRun
        // Check 2: Issue.checked_out_by = agent_id but agent is terminated
        // Check 3: Issue has active lease but status != in_progress

        Ok(reports)
    }

    async fn fix(&self, report: InconsistencyReport) -> ServiceResult<bool> {
        match report.fix_strategy {
            FixStrategy::AutoFix => {
                // Implement auto-fix logic
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn resource_type(&self) -> String {
        "issue".to_string()
    }
}

pub struct EnvironmentLeaseChecker;

#[async_trait]
impl ConsistencyChecker for EnvironmentLeaseChecker {
    async fn check(&self, _company_id: Uuid) -> ServiceResult<Vec<InconsistencyReport>> {
        let reports = vec![];

        // Check 1: Lease expired but not released
        // Check 2: Lease held by terminated agent
        // Check 3: Environment has lease but no corresponding workspace

        Ok(reports)
    }

    async fn fix(&self, report: InconsistencyReport) -> ServiceResult<bool> {
        match report.fix_strategy {
            FixStrategy::AutoFix => {
                // Implement auto-fix logic: release expired leases
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn resource_type(&self) -> String {
        "environment_lease".to_string()
    }
}

pub struct AgentStateChecker;

#[async_trait]
impl ConsistencyChecker for AgentStateChecker {
    async fn check(&self, _company_id: Uuid) -> ServiceResult<Vec<InconsistencyReport>> {
        let reports = vec![];

        // Check 1: Agent.status = employed but no active issue
        // Check 2: Agent has checked_out_issue but issue.checked_out_by != agent_id
        // Check 3: Agent terminated but still holds leases

        Ok(reports)
    }

    async fn fix(&self, report: InconsistencyReport) -> ServiceResult<bool> {
        match report.fix_strategy {
            FixStrategy::AutoFix => {
                // Implement auto-fix logic
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn resource_type(&self) -> String {
        "agent".to_string()
    }
}

pub struct CheckerRegistry {
    checkers: Vec<Box<dyn ConsistencyChecker>>,
}

impl CheckerRegistry {
    pub fn new() -> Self {
        let checkers: Vec<Box<dyn ConsistencyChecker>> = vec![
            Box::new(IssueStateChecker),
            Box::new(EnvironmentLeaseChecker),
            Box::new(AgentStateChecker),
        ];

        Self { checkers }
    }

    pub async fn run_all_checks(&self, company_id: Uuid) -> ServiceResult<Vec<InconsistencyReport>> {
        let mut all_reports = vec![];

        for checker in &self.checkers {
            let reports = checker.check(company_id).await?;
            all_reports.extend(reports);
        }

        Ok(all_reports)
    }

    pub async fn auto_fix_all(&self, reports: Vec<InconsistencyReport>) -> ServiceResult<Vec<InconsistencyReport>> {
        let mut fixed_reports = vec![];

        for report in reports {
            if report.fix_strategy == FixStrategy::AutoFix {
                for checker in &self.checkers {
                    if checker.resource_type() == report.resource_type {
                        let fixed = checker.fix(report.clone()).await?;
                        let mut updated_report = report.clone();
                        updated_report.fixed = fixed;
                        fixed_reports.push(updated_report);
                        break;
                    }
                }
            } else {
                fixed_reports.push(report);
            }
        }

        Ok(fixed_reports)
    }
}

impl Default for CheckerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
