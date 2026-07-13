//! 后台调度器统一管理
//!
//! 提供 JobScheduler 统一管理所有定时后台任务
//! 对应 cross-module-integration-tasks.md §6 后台调度器统一管理

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{self, Duration, Instant};
use uuid::Uuid;
use crate::WatchdogService;
use repositories::CompanyRepository;

/// 任务执行记录
#[derive(Debug, Clone)]
pub struct JobExecutionRecord {
    pub id: String,
    pub job_name: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: JobStatus,
    pub error_message: Option<String>,
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Idle,
    Running,
    Failed,
    Disabled,
}

/// 任务调度配置
#[derive(Debug, Clone)]
pub enum JobSchedule {
    /// 固定间隔（秒）
    IntervalSeconds(u64),
    /// Cron 表达式（预留）
    CronExpression(String),
    /// 事件驱动（预留）
    OnEvent,
}

/// 后台任务 trait
#[async_trait]
pub trait ScheduledJob: Send + Sync {
    /// 任务名称
    fn job_name(&self) -> &str;

    /// 调度配置
    fn schedule(&self) -> JobSchedule;

    /// 执行任务
    async fn execute(&self) -> Result<String, String>;

    /// 健康检查
    async fn health_check(&self) -> bool {
        true
    }
}

/// 任务注册表
pub struct JobRegistry {
    jobs: RwLock<HashMap<String, Arc<dyn ScheduledJob>>>,
    executions: RwLock<Vec<JobExecutionRecord>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            executions: RwLock::new(Vec::new()),
        }
    }

    pub async fn register(&self, job: Arc<dyn ScheduledJob>) {
        let name = job.job_name().to_string();
        self.jobs.write().await.insert(name, job);
    }

    pub async fn unregister(&self, name: &str) {
        self.jobs.write().await.remove(name);
    }

    pub async fn list_jobs(&self) -> Vec<String> {
        self.jobs.read().await.keys().cloned().collect()
    }

    pub async fn get_job(&self, name: &str) -> Option<Arc<dyn ScheduledJob>> {
        self.jobs.read().await.get(name).cloned()
    }

    pub async fn record_execution(&self, record: JobExecutionRecord) {
        self.executions.write().await.push(record);
    }

    pub async fn get_recent_executions(&self, limit: usize) -> Vec<JobExecutionRecord> {
        let executions = self.executions.read().await;
        executions.iter().rev().take(limit).cloned().collect()
    }
}

/// 后台调度器
pub struct JobScheduler {
    registry: Arc<JobRegistry>,
    handles: RwLock<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl JobScheduler {
    pub fn new(registry: Arc<JobRegistry>) -> Self {
        Self {
            registry,
            handles: RwLock::new(HashMap::new()),
        }
    }

    /// 启动所有已注册任务
    pub async fn start(&self) {
        let jobs = self.registry.list_jobs().await;
        for job_name in jobs {
            self.start_job(&job_name).await;
        }
    }

    /// 启动单个任务
    pub async fn start_job(&self, job_name: &str) {
        let job = self.registry.get_job(job_name).await;
        if let Some(job) = job {
            let registry = self.registry.clone();
            let name = job_name.to_string();
            let handle = tokio::spawn(async move {
                let schedule = job.schedule();
                let interval = match schedule {
                    JobSchedule::IntervalSeconds(secs) => Duration::from_secs(secs),
                    JobSchedule::CronExpression(_) => Duration::from_secs(300), // fallback
                    JobSchedule::OnEvent => return, // not supported yet
                };

                let mut ticker = time::interval(interval);
                ticker.tick().await; // skip first immediate tick

                loop {
                    ticker.tick().await;

                    let record = JobExecutionRecord {
                        id: Uuid::new_v4().to_string(),
                        job_name: name.clone(),
                        started_at: chrono::Utc::now(),
                        completed_at: None,
                        status: JobStatus::Running,
                        error_message: None,
                    };

                    let result = job.execute().await;
                    let mut completed = record;
                    completed.completed_at = Some(chrono::Utc::now());

                    match result {
                        Ok(_) => {
                            completed.status = JobStatus::Idle;
                        }
                        Err(e) => {
                            completed.status = JobStatus::Failed;
                            completed.error_message = Some(e);
                        }
                    }

                    registry.record_execution(completed).await;
                }
            });

            self.handles.write().await.insert(job_name.to_string(), handle);
        }
    }

    /// 停止所有任务
    pub async fn stop(&self) {
        let handles = self.handles.read().await;
        for (_, handle) in handles.iter() {
            handle.abort();
        }
    }

    /// 暂停单个任务
    pub async fn pause_job(&self, _job_name: &str) {
        // TODO: implement pause via flag
    }

    /// 恢复单个任务
    pub async fn resume_job(&self, _job_name: &str) {
        // TODO: implement resume via flag
    }

    /// 手动触发单个任务
    pub async fn trigger_job(&self, job_name: &str) -> Result<String, String> {
        let job = self.registry.get_job(job_name).await;
        match job {
            Some(j) => j.execute().await,
            None => Err(format!("Job '{}' not found", job_name)),
        }
    }
}

// ============================================================================
// 预定义后台任务
// ============================================================================

/// Monitor 定时检查器（每分钟）
pub struct MonitorCheckJob;

#[async_trait]
impl ScheduledJob for MonitorCheckJob {
    fn job_name(&self) -> &str {
        "monitor_check"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        // TODO: Check issues with monitor_next_check_at < NOW()
        Ok("Monitor check completed".to_string())
    }
}

/// 租约过期扫描器（每分钟）
pub struct LeaseExpiryScanner;

#[async_trait]
impl ScheduledJob for LeaseExpiryScanner {
    fn job_name(&self) -> &str {
        "lease_expiry_scanner"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        // TODO: Scan for expired environment leases
        Ok("Lease expiry scan completed".to_string())
    }
}

/// 环境健康探测器（每5分钟）
pub struct EnvironmentHealthProbe;

#[async_trait]
impl ScheduledJob for EnvironmentHealthProbe {
    fn job_name(&self) -> &str {
        "environment_health_probe"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(300)
    }

    async fn execute(&self) -> Result<String, String> {
        // TODO: Probe environment health
        Ok("Environment health probe completed".to_string())
    }
}

/// Routine Cron 触发器（每分钟）
pub struct RoutineCronTrigger;

#[async_trait]
impl ScheduledJob for RoutineCronTrigger {
    fn job_name(&self) -> &str {
        "routine_cron_trigger"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        // TODO: Check for routines due for cron trigger
        Ok("Routine cron trigger check completed".to_string())
    }
}

/// 状态一致性检查器（每小时）
pub struct ConsistencyCheckJob;

#[async_trait]
impl ScheduledJob for ConsistencyCheckJob {
    fn job_name(&self) -> &str {
        "consistency_check"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(3600)
    }

    async fn execute(&self) -> Result<String, String> {
        // TODO: Run consistency checks
        Ok("Consistency check completed".to_string())
    }
}

// ============================================================================
// Task Watchdog 定时评估器
// ============================================================================

/// Watchdog 定时评估任务（每5分钟）
///
/// Periodically evaluates all active watchdogs across companies.
/// Requires a CompanyRepository to discover which companies have active watchdogs.
pub struct WatchdogEvaluationJob {
    watchdog_service: Arc<dyn WatchdogService>,
    company_repo: Arc<CompanyRepository>,
}

impl WatchdogEvaluationJob {
    pub fn new(
        watchdog_service: Arc<dyn WatchdogService>,
        company_repo: Arc<CompanyRepository>,
    ) -> Self {
        Self {
            watchdog_service,
            company_repo,
        }
    }
}

#[async_trait]
impl ScheduledJob for WatchdogEvaluationJob {
    fn job_name(&self) -> &str {
        "watchdog_evaluation"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(300) // 每5分钟
    }

    async fn execute(&self) -> Result<String, String> {
        // Load all companies and evaluate watchdogs for each
        let companies = self.company_repo.list(1000, 0).await
            .map_err(|e| format!("Failed to list companies: {}", e))?;

        let mut total_evaluated = 0usize;
        let mut errors = Vec::new();

        for company in &companies {
            match self.watchdog_service.evaluate_all(company.id).await {
                Ok(count) => total_evaluated += count,
                Err(e) => errors.push(format!("Company {}: {}", company.id, e)),
            }
        }

        if errors.is_empty() {
            Ok(format!("Watchdog evaluation completed: {} watchdogs evaluated across {} companies", total_evaluated, companies.len()))
        } else {
            Err(format!("Watchdog evaluation completed with errors: {} evaluated, {} companies, errors: {}", total_evaluated, companies.len(), errors.join("; ")))
        }
    }
}
