use async_trait::async_trait;
use chrono::Utc;
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

use crate::decision_retention_runtime::PgDecisionRetentionRuntime;
use crate::decision_wakeup_service::DecisionWakeupService;
use crate::job_scheduler::{JobSchedule, ScheduledJob};

/// Periodic Paperclip-compatible retention sweep.
///
/// The job is intentionally independent from server startup wiring so that a
/// deployment can register it with the existing scheduler without coupling
/// the retention implementation to `crates/server`.
pub struct DecisionRetentionSweepJob {
    pool: PgPool,
    runtime: PgDecisionRetentionRuntime,
}

impl DecisionRetentionSweepJob {
    pub fn new(pool: PgPool) -> Self {
        Self {
            runtime: PgDecisionRetentionRuntime::new(pool.clone()),
            pool,
        }
    }

    pub fn with_wakeup(mut self, wakeup: Arc<dyn DecisionWakeupService>) -> Self {
        self.runtime = self.runtime.with_wakeup(wakeup);
        self
    }
}

#[async_trait]
impl ScheduledJob for DecisionRetentionSweepJob {
    fn job_name(&self) -> &str {
        "decision_retention_sweep"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        let company_rows = sqlx::query("SELECT id FROM companies")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| format!("failed to list companies for retention sweep: {error}"))?;
        let now = Utc::now();
        let mut archived = 0usize;
        for row in company_rows {
            let company_id: Uuid = row.try_get("id").map_err(|error| {
                format!("failed to read company id for retention sweep: {error}")
            })?;
            archived += self
                .runtime
                .auto_archive_company(company_id, now)
                .await
                .map_err(|error| format!("retention archive failed for {company_id}: {error}"))?;
        }
        let delivery = self
            .runtime
            .deliver_notifications(500)
            .await
            .map_err(|error| format!("retention notification delivery failed: {error}"))?;
        Ok(format!(
            "archived {archived} retention sources; delivered {} notifications to {} agents",
            delivery.delivered, delivery.notified_agents
        ))
    }
}
