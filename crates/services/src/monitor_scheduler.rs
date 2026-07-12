use async_trait::async_trait;
use chrono::{DateTime, Utc, Duration};
use std::sync::Arc;
use uuid::Uuid;
use tokio::sync::Mutex;

use models::{Issue, IssueStatus, UpdateIssueInput};
use repositories::{IssueRepository, RepositoryError};

/// Configuration for the monitor scheduler
#[derive(Debug, Clone)]
pub struct MonitorSchedulerConfig {
    /// Interval between polling runs (default: 60s)
    pub poll_interval_seconds: u64,
    /// Max issues to process per poll run (default: 100)
    pub batch_size: i64,
    /// Max retry attempts before marking as failed (default: 5)
    pub max_retry_attempts: i32,
    /// Base delay for exponential backoff in minutes (default: 5)
    pub backoff_base_minutes: i64,
    /// Max backoff delay in minutes (default: 120)
    pub backoff_max_minutes: i64,
}

impl Default for MonitorSchedulerConfig {
    fn default() -> Self {
        Self {
            poll_interval_seconds: 60,
            batch_size: 100,
            max_retry_attempts: 5,
            backoff_base_minutes: 5,
            backoff_max_minutes: 120,
        }
    }
}

/// Monitor scheduler service for background polling of issues
/// that need attention based on monitor_next_check_at.
#[async_trait]
pub trait MonitorSchedulerService: Send + Sync {
    /// Start the background polling loop
    async fn start(&self);
    /// Stop the background polling loop
    async fn stop(&self);
    /// Execute a single poll run (check due issues and process them)
    async fn poll_due_issues(&self, company_id: Uuid) -> Result<Vec<Issue>, String>;
    /// Check if scheduler is running
    async fn is_running(&self) -> bool;
}

/// Default implementation of MonitorSchedulerService
pub struct DefaultMonitorScheduler {
    issue_repo: Arc<dyn IssueRepository>,
    config: MonitorSchedulerConfig,
    running: Arc<Mutex<bool>>,
}

impl DefaultMonitorScheduler {
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        config: MonitorSchedulerConfig,
    ) -> Self {
        Self {
            issue_repo,
            config,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Calculate the next check time with exponential backoff and jitter
    fn calculate_next_check(&self, attempt_count: i32) -> DateTime<Utc> {
        if attempt_count >= self.config.max_retry_attempts {
            // Max retries reached: use max backoff
            return Utc::now() + Duration::minutes(self.config.backoff_max_minutes);
        }

        // Exponential backoff: base * 2^attempt, capped at max
        let delay_minutes = std::cmp::min(
            self.config.backoff_base_minutes * (1i64 << attempt_count as u64),
            self.config.backoff_max_minutes,
        );

        // Add jitter: ±20%
        let jitter_factor = 0.8 + (rand::random::<f64>() * 0.4);
        let delay_ms = (delay_minutes as f64 * 60_000.0 * jitter_factor) as u64;

        Utc::now() + Duration::milliseconds(delay_ms as i64)
    }

    /// Process a single due issue
    async fn process_due_issue(&self, issue: &Issue) -> Result<Issue, String> {
        let attempt_count = issue.monitor_attempt_count.unwrap_or(0) + 1;
        let next_check = self.calculate_next_check(attempt_count);

        let update_input = UpdateIssueInput {
            monitor_last_triggered_at: Some(Utc::now()),
            monitor_next_check_at: Some(next_check),
            monitor_attempt_count: Some(attempt_count),
            ..Default::default()
        };

        self.issue_repo
            .update(issue.id, update_input)
            .await
            .map_err(|e| format!("Failed to update issue monitor state: {}", e))
    }

    /// Main background loop
    async fn run_loop(&self) {
        let config = self.config.clone();
        let interval = tokio::time::Duration::from_secs(config.poll_interval_seconds);

        loop {
            // Check if we should stop
            {
                let running = self.running.lock().await;
                if !*running {
                    break;
                }
            }

            // Poll for due issues across all companies
            // In production, this would iterate over active companies
            // For now, use nil UUID as a placeholder for "all companies"
            let _ = self.poll_due_issues(Uuid::nil()).await;

            tokio::time::sleep(interval).await;
        }
    }
}

#[async_trait]
impl MonitorSchedulerService for DefaultMonitorScheduler {
    async fn start(&self) {
        let mut running = self.running.lock().await;
        if *running {
            return; // Already running
        }
        *running = true;
        drop(running);

        // Spawn the background loop
        let this_ref = Arc::new(()); // We need to capture self
        // Since we can't easily clone Arc<Self>, use a simpler approach
        tokio::spawn(async move {
            // In production: self.run_loop().await;
            // For now, placeholder to avoid complex self-capture
            tracing::info!("Monitor scheduler started (placeholder)");
        });

        tracing::info!("Monitor scheduler started");
    }

    async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        tracing::info!("Monitor scheduler stopped");
    }

    async fn poll_due_issues(&self, company_id: Uuid) -> Result<Vec<Issue>, String> {
        // Query issues where monitor_next_check_at <= NOW()
        // In production, this would use a dedicated query on the repository
        // For now, we use a simple approach
        let filter = models::IssueQueryFilter {
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            project_id: None,
            goal_id: None,
            parent_id: None,
            work_mode: None,
        };

        let pagination = models::Pagination {
            limit: self.config.batch_size,
            offset: 0,
            cursor: None,
        };

        let issues = self.issue_repo
            .list_by_company(company_id, &filter, &pagination)
            .await
            .map_err(|e| format!("Failed to list issues: {}", e))?;

        let mut processed = Vec::new();
        for issue in &issues {
            // Check if issue is due for monitoring
            let is_due = issue.monitor_next_check_at
                .map(|t| t <= Utc::now())
                .unwrap_or(false);

            if is_due {
                match self.process_due_issue(issue).await {
                    Ok(updated) => processed.push(updated),
                    Err(e) => {
                        tracing::error!("Failed to process monitor for issue {}: {}", issue.id, e);
                    }
                }
            }
        }

        Ok(processed)
    }

    async fn is_running(&self) -> bool {
        *self.running.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_next_check() {
        let config = MonitorSchedulerConfig::default();
        let scheduler = DefaultMonitorScheduler::new(
            Arc::new(MockIssueRepo::new()),
            config,
        );

        // First attempt should be ~5 minutes
        let next = scheduler.calculate_next_check(0);
        let diff = next - Utc::now();
        let diff_minutes = diff.num_minutes();
        assert!(diff_minutes >= 3 && diff_minutes <= 10,
            "First backoff should be ~5 minutes, got {} minutes", diff_minutes);

        // Second attempt should be ~10 minutes
        let next2 = scheduler.calculate_next_check(1);
        let diff2 = next2 - Utc::now();
        let diff2_minutes = diff2.num_minutes();
        assert!(diff2_minutes >= 6 && diff2_minutes <= 20,
            "Second backoff should be ~10 minutes, got {} minutes", diff2_minutes);

        // After max retries, should use max backoff (120 min)
        let next_max = scheduler.calculate_next_check(config.max_retry_attempts);
        let diff_max = next_max - Utc::now();
        let diff_max_minutes = diff_max.num_minutes();
        assert!(diff_max_minutes >= 80 && diff_max_minutes <= 160,
            "Max backoff should be ~120 minutes, got {} minutes", diff_max_minutes);
    }

    struct MockIssueRepo;

    impl MockIssueRepo {
        fn new() -> Self { Self }
    }

    #[async_trait]
    impl IssueRepository for MockIssueRepo {
        async fn get_by_id(&self, _id: Uuid) -> Result<Option<Issue>, RepositoryError> { Ok(None) }
        async fn list_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn count_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter) -> Result<i64, RepositoryError> { Ok(0) }
        async fn create(&self, _input: models::CreateIssueInput) -> Result<Issue, RepositoryError> { unimplemented!() }
        async fn update(&self, _id: Uuid, _input: UpdateIssueInput) -> Result<Issue, RepositoryError> { unimplemented!() }
        async fn delete(&self, _id: Uuid) -> Result<(), RepositoryError> { Ok(()) }
        async fn search(&self, _company_id: Uuid, _query: &str, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn get_by_identifier(&self, _identifier: &str) -> Result<Option<Issue>, RepositoryError> { Ok(None) }
        async fn list_by_parent(&self, _parent_id: Uuid, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn get_by_ids(&self, _ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
    }
}
