/// Plugin Job Scheduler - 基于 tick 的插件定时作业调度器
///
/// 调度器是所有插件 cron 作业的中央协调器。它定期 tick（默认每 30 秒），
/// 查询 `plugin_jobs` 表中 `next_run_at` 已过期的作业，向适当的 worker 进程
/// 分发 `runJob` RPC 调用，在 `plugin_job_runs` 表中记录每次执行，并推进调度指针。
///
/// ## 职责
///
/// 1. **Tick 循环** - 基于 interval 的循环，每 `tick_interval_ms`（默认 30 秒）触发一次。
///    每次 tick 扫描到期作业并分发。
///
/// 2. **Cron 解析和下次运行计算** - 使用 cron 解析器计算每次运行后或新作业注册时的 `next_run_at`。
///
/// 3. **重叠防止** - 在分发作业前，调度器检查同一作业是否有正在运行的实例。如果有，跳过此次 tick。
///
/// 4. **作业运行记录** - 每次执行创建一个 `plugin_job_runs` 行：
///    `queued` → `running` → `succeeded` | `failed`。捕获持续时间和错误。
///
/// 5. **生命周期集成** - 调度器暴露 `register_plugin()` 和 `unregister_plugin()`，
///    以便宿主生命周期管理器在插件启动/停止时连接作业调度。

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::plugin_worker_manager::{PluginWorkerManager, WorkerError};

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

/// 调度器 tick 之间的默认间隔（30 秒）
const DEFAULT_TICK_INTERVAL_MS: u64 = 30_000;

/// runJob RPC 调用的默认超时（5 分钟）
const DEFAULT_JOB_TIMEOUT_MS: u64 = 5 * 60 * 1_000;

/// 所有插件的最大并发作业执行数
const DEFAULT_MAX_CONCURRENT_JOBS: usize = 10;

// ---------------------------------------------------------------------------
// 类型定义
// ---------------------------------------------------------------------------

/// 作业优先级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// 作业状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// 作业触发类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobTrigger {
    Scheduled,
    Manual,
    Retry,
}

/// 插件作业
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginJob {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub name: String,
    pub cron_schedule: String,
    pub enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 作业运行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginJobRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub plugin_id: Uuid,
    pub status: JobStatus,
    pub trigger: JobTrigger,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 手动触发作业的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerJobResult {
    pub run_id: Uuid,
    pub job_id: Uuid,
}

/// 调度器诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerDiagnostics {
    pub running: bool,
    pub active_job_count: usize,
    pub active_job_ids: Vec<Uuid>,
    pub tick_count: u64,
    pub last_tick_at: Option<DateTime<Utc>>,
}

/// 调度器选项
pub struct PluginJobSchedulerOptions {
    pub db: PgPool,
    pub worker_manager: Arc<PluginWorkerManager>,
    pub tick_interval_ms: Option<u64>,
    pub job_timeout_ms: Option<u64>,
    pub max_concurrent_jobs: Option<usize>,
}

// ---------------------------------------------------------------------------
// 错误类型
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("job not found: {0}")]
    JobNotFound(Uuid),

    #[error("job already running: {0}")]
    JobAlreadyRunning(Uuid),

    #[error("job not enabled: {0}")]
    JobNotEnabled(Uuid),

    #[error("invalid cron schedule: {0}")]
    InvalidCronSchedule(String),

    #[error("max concurrent jobs reached: {0}")]
    MaxConcurrentJobsReached(usize),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("worker error: {0}")]
    Worker(#[from] WorkerError),

    #[error("scheduler not running")]
    NotRunning,
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;

// ---------------------------------------------------------------------------
// Plugin Job Scheduler
// ---------------------------------------------------------------------------

/// 插件作业调度器
pub struct PluginJobScheduler {
    db: PgPool,
    worker_manager: Arc<PluginWorkerManager>,
    tick_interval_ms: u64,
    job_timeout_ms: u64,
    max_concurrent_jobs: usize,

    // 状态
    running: Arc<RwLock<bool>>,
    active_jobs: Arc<Mutex<HashSet<Uuid>>>,
    tick_count: Arc<RwLock<u64>>,
    last_tick_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    tick_in_progress: Arc<Mutex<bool>>,

    // 取消令牌
    shutdown_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<()>>>>,
}

impl PluginJobScheduler {
    /// 创建新的调度器实例
    pub fn new(options: PluginJobSchedulerOptions) -> Self {
        Self {
            db: options.db,
            worker_manager: options.worker_manager,
            tick_interval_ms: options.tick_interval_ms.unwrap_or(DEFAULT_TICK_INTERVAL_MS),
            job_timeout_ms: options.job_timeout_ms.unwrap_or(DEFAULT_JOB_TIMEOUT_MS),
            max_concurrent_jobs: options
                .max_concurrent_jobs
                .unwrap_or(DEFAULT_MAX_CONCURRENT_JOBS),
            running: Arc::new(RwLock::new(false)),
            active_jobs: Arc::new(Mutex::new(HashSet::new())),
            tick_count: Arc::new(RwLock::new(0)),
            last_tick_at: Arc::new(RwLock::new(None)),
            tick_in_progress: Arc::new(Mutex::new(false)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// 启动调度器 tick 循环
    ///
    /// 安全多次调用 - 后续调用是 no-op
    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            debug!("plugin_job_scheduler: already running");
            return;
        }

        *running = true;
        drop(running);

        info!(
            "plugin_job_scheduler: starting with tick_interval={}ms",
            self.tick_interval_ms
        );

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        // 启动 tick 循环
        let self_clone = self.clone_for_task();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_millis(self_clone.tick_interval_ms));

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if let Err(e) = self_clone.tick().await {
                            error!("plugin_job_scheduler: tick error: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("plugin_job_scheduler: shutdown signal received");
                        break;
                    }
                }
            }

            *self_clone.running.write().await = false;
            info!("plugin_job_scheduler: stopped");
        });
    }

    /// 停止调度器 tick 循环
    ///
    /// 正在运行的作业不会被取消 - 它们被允许自然完成。tick 循环只是停止触发。
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        if !*running {
            debug!("plugin_job_scheduler: already stopped");
            return;
        }

        info!("plugin_job_scheduler: stopping");

        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }

        *running = false;
    }

    /// 执行单次调度器 tick
    ///
    /// 查询到期作业并分发它们
    pub async fn tick(&self) -> SchedulerResult<()> {
        // 防止重叠 tick（如果 tick 耗时超过间隔）
        let mut tick_in_progress = self.tick_in_progress.lock().await;
        if *tick_in_progress {
            debug!("plugin_job_scheduler: skipping tick - previous tick still in progress");
            return Ok(());
        }

        *tick_in_progress = true;
        drop(tick_in_progress);

        let tick_start = std::time::Instant::now();

        // 更新 tick 计数
        let mut tick_count = self.tick_count.write().await;
        *tick_count += 1;
        let current_tick = *tick_count;
        drop(tick_count);

        *self.last_tick_at.write().await = Some(Utc::now());

        debug!("plugin_job_scheduler: tick #{}", current_tick);

        // 查询到期的作业
        let due_jobs = self.fetch_due_jobs().await?;

        if !due_jobs.is_empty() {
            info!(
                "plugin_job_scheduler: found {} due jobs",
                due_jobs.len()
            );
        }

        // 分发作业
        for job in due_jobs {
            // 检查是否已达到最大并发限制
            let active_count = self.active_jobs.lock().await.len();
            if active_count >= self.max_concurrent_jobs {
                warn!(
                    "plugin_job_scheduler: max concurrent jobs reached ({}), skipping job {}",
                    self.max_concurrent_jobs, job.id
                );
                continue;
            }

            // 检查作业是否已在运行中
            if self.active_jobs.lock().await.contains(&job.id) {
                debug!(
                    "plugin_job_scheduler: job {} already running, skipping",
                    job.id
                );
                continue;
            }

            // 分发作业
            if let Err(e) = self.dispatch_job(job.clone(), JobTrigger::Scheduled).await {
                error!(
                    "plugin_job_scheduler: failed to dispatch job {}: {}",
                    job.id, e
                );
            }
        }

        *self.tick_in_progress.lock().await = false;

        let elapsed = tick_start.elapsed();
        debug!(
            "plugin_job_scheduler: tick #{} completed in {:?}",
            current_tick, elapsed
        );

        Ok(())
    }

    /// 注册插件到调度器
    ///
    /// 为所有缺少 next_run_at 的活动作业计算它。
    /// 通常在插件的 worker 进程启动后调用。
    pub async fn register_plugin(&self, plugin_id: Uuid) -> SchedulerResult<()> {
        info!(
            "plugin_job_scheduler: registering plugin {}",
            plugin_id
        );

        // 查询该插件的所有启用的作业
        let jobs = sqlx::query_as::<_, PluginJob>(
            r#"
            SELECT id, plugin_id, name, cron_schedule, enabled, 
                   next_run_at, last_run_at, created_at, updated_at
            FROM plugin_jobs
            WHERE plugin_id = $1 AND enabled = true
            "#,
        )
        .bind(plugin_id)
        .fetch_all(&self.db)
        .await?;

        // 为缺少 next_run_at 的作业计算它
        for job in jobs {
            if job.next_run_at.is_none() {
                if let Ok(next_run) = Self::calculate_next_run(&job.cron_schedule, None) {
                    sqlx::query(
                        r#"
                        UPDATE plugin_jobs
                        SET next_run_at = $1, updated_at = NOW()
                        WHERE id = $2
                        "#,
                    )
                    .bind(next_run)
                    .bind(job.id)
                    .execute(&self.db)
                    .await?;

                    debug!(
                        "plugin_job_scheduler: set next_run_at for job {} to {}",
                        job.id, next_run
                    );
                }
            }
        }

        Ok(())
    }

    /// 从调度器注销插件
    ///
    /// 取消该插件的所有正在运行的作业并移除跟踪状态。
    pub async fn unregister_plugin(&self, plugin_id: Uuid) -> SchedulerResult<()> {
        info!(
            "plugin_job_scheduler: unregistering plugin {}",
            plugin_id
        );

        // 移除该插件的所有活动作业
        let mut active_jobs = self.active_jobs.lock().await;
        let jobs_to_remove: Vec<_> = active_jobs
            .iter()
            .copied()
            .collect();

        for job_id in jobs_to_remove {
            // 查询作业的 plugin_id
            let plugin_id_opt: Option<(Uuid,)> = sqlx::query_as(
                r#"
                SELECT plugin_id FROM plugin_jobs WHERE id = $1
                "#,
            )
            .bind(job_id)
            .fetch_optional(&self.db)
            .await?;

            if let Some((job_plugin_id,)) = plugin_id_opt {
                if job_plugin_id == plugin_id {
                    active_jobs.remove(&job_id);
                    debug!("plugin_job_scheduler: removed active job {}", job_id);
                }
            }
        }

        Ok(())
    }

    /// 手动触发特定作业（在 cron 调度之外）
    ///
    /// 创建一个 `trigger: "manual"` 的运行并立即分发，遵守重叠防止检查。
    pub async fn trigger_job(
        &self,
        job_id: Uuid,
        trigger: Option<JobTrigger>,
    ) -> SchedulerResult<TriggerJobResult> {
        let trigger = trigger.unwrap_or(JobTrigger::Manual);

        // 查询作业
        let job: Option<PluginJob> = sqlx::query_as(
            r#"
            SELECT id, plugin_id, name, cron_schedule, enabled,
                   next_run_at, last_run_at, created_at, updated_at
            FROM plugin_jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.db)
        .await?;

        let job = job.ok_or(SchedulerError::JobNotFound(job_id))?;

        if !job.enabled {
            return Err(SchedulerError::JobNotEnabled(job_id));
        }

        // 检查是否已在运行中
        if self.active_jobs.lock().await.contains(&job_id) {
            return Err(SchedulerError::JobAlreadyRunning(job_id));
        }

        // 分发作业
        let run_id = self.dispatch_job(job, trigger).await?;

        Ok(TriggerJobResult { run_id, job_id })
    }

    /// 获取调度器诊断信息
    pub async fn diagnostics(&self) -> SchedulerDiagnostics {
        let active_jobs = self.active_jobs.lock().await;
        SchedulerDiagnostics {
            running: *self.running.read().await,
            active_job_count: active_jobs.len(),
            active_job_ids: active_jobs.iter().copied().collect(),
            tick_count: *self.tick_count.read().await,
            last_tick_at: *self.last_tick_at.read().await,
        }
    }

    // -----------------------------------------------------------------------
    // 内部方法
    // -----------------------------------------------------------------------

    /// 查询到期的作业
    async fn fetch_due_jobs(&self) -> SchedulerResult<Vec<PluginJob>> {
        let now = Utc::now();
        let jobs = sqlx::query_as::<_, PluginJob>(
            r#"
            SELECT id, plugin_id, name, cron_schedule, enabled,
                   next_run_at, last_run_at, created_at, updated_at
            FROM plugin_jobs
            WHERE enabled = true
              AND next_run_at IS NOT NULL
              AND next_run_at <= $1
            ORDER BY next_run_at ASC
            LIMIT 100
            "#,
        )
        .bind(now)
        .fetch_all(&self.db)
        .await?;

        Ok(jobs)
    }

    /// 分发作业到 worker 进程
    async fn dispatch_job(&self, job: PluginJob, trigger: JobTrigger) -> SchedulerResult<Uuid> {
        let run_id = Uuid::new_v4();
        let job_id = job.id;
        let plugin_id = job.plugin_id;

        info!(
            "plugin_job_scheduler: dispatching job {} (run_id={})",
            job_id, run_id
        );

        // 标记作业为活动
        self.active_jobs.lock().await.insert(job_id);

        // 创建作业运行记录
        sqlx::query(
            r#"
            INSERT INTO plugin_job_runs (id, job_id, plugin_id, status, trigger, created_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            "#,
        )
        .bind(run_id)
        .bind(job_id)
        .bind(plugin_id)
        .bind("queued")
        .bind(match trigger {
            JobTrigger::Scheduled => "scheduled",
            JobTrigger::Manual => "manual",
            JobTrigger::Retry => "retry",
        })
        .execute(&self.db)
        .await?;

        // 异步执行作业
        let self_clone = self.clone_for_task();
        let job_clone = job.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone.execute_job(run_id, job_clone).await {
                error!(
                    "plugin_job_scheduler: job execution failed for run_id={}: {}",
                    run_id, e
                );
            }
        });

        Ok(run_id)
    }

    /// 执行作业
    async fn execute_job(&self, run_id: Uuid, job: PluginJob) -> SchedulerResult<()> {
        let job_id = job.id;
        let plugin_id = job.plugin_id;
        let start_time = Utc::now();

        info!(
            "plugin_job_scheduler: executing job {} (run_id={})",
            job_id, run_id
        );

        // 更新运行状态为 running
        sqlx::query(
            r#"
            UPDATE plugin_job_runs
            SET status = 'running', started_at = $1
            WHERE id = $2
            "#,
        )
        .bind(start_time)
        .bind(run_id)
        .execute(&self.db)
        .await?;

        // 调用 worker 的 runJob RPC
        let params = serde_json::json!({
            "jobId": job_id,
            "runId": run_id,
            "jobName": job.name,
        });

        let result = self
            .worker_manager
            .call(plugin_id, "runJob".to_string(), params, Some(self.job_timeout_ms))
            .await;

        let end_time = Utc::now();
        let duration_ms = (end_time - start_time).num_milliseconds();

        match result {
            Ok(_) => {
                // 作业成功
                sqlx::query(
                    r#"
                    UPDATE plugin_job_runs
                    SET status = 'succeeded', finished_at = $1, duration_ms = $2
                    WHERE id = $3
                    "#,
                )
                .bind(end_time)
                .bind(duration_ms)
                .bind(run_id)
                .execute(&self.db)
                .await?;

                info!(
                    "plugin_job_scheduler: job {} completed successfully (run_id={}, duration={}ms)",
                    job_id, run_id, duration_ms
                );
            }
            Err(e) => {
                // 作业失败
                let error_msg = e.to_string();
                sqlx::query(
                    r#"
                    UPDATE plugin_job_runs
                    SET status = 'failed', finished_at = $1, duration_ms = $2, error = $3
                    WHERE id = $4
                    "#,
                )
                .bind(end_time)
                .bind(duration_ms)
                .bind(&error_msg)
                .bind(run_id)
                .execute(&self.db)
                .await?;

                error!(
                    "plugin_job_scheduler: job {} failed (run_id={}): {}",
                    job_id, run_id, error_msg
                );
            }
        }

        // 更新作业的 last_run_at 和 next_run_at
        if let Ok(next_run) = Self::calculate_next_run(&job.cron_schedule, Some(end_time)) {
            sqlx::query(
                r#"
                UPDATE plugin_jobs
                SET last_run_at = $1, next_run_at = $2, updated_at = NOW()
                WHERE id = $3
                "#,
            )
            .bind(end_time)
            .bind(next_run)
            .bind(job_id)
            .execute(&self.db)
            .await?;

            debug!(
                "plugin_job_scheduler: updated next_run_at for job {} to {}",
                job_id, next_run
            );
        }

        // 从活动作业集合中移除
        self.active_jobs.lock().await.remove(&job_id);

        Ok(())
    }

    /// 计算下次运行时间
    fn calculate_next_run(
        cron_schedule: &str,
        after: Option<DateTime<Utc>>,
    ) -> Result<DateTime<Utc>, SchedulerError> {
        let schedule = Schedule::from_str(cron_schedule)
            .map_err(|_| SchedulerError::InvalidCronSchedule(cron_schedule.to_string()))?;

        let after = after.unwrap_or_else(Utc::now);
        let next = schedule
            .after(&after)
            .next()
            .ok_or_else(|| SchedulerError::InvalidCronSchedule(cron_schedule.to_string()))?;

        Ok(next)
    }

    /// 克隆用于异步任务
    fn clone_for_task(&self) -> Self {
        Self {
            db: self.db.clone(),
            worker_manager: Arc::clone(&self.worker_manager),
            tick_interval_ms: self.tick_interval_ms,
            job_timeout_ms: self.job_timeout_ms,
            max_concurrent_jobs: self.max_concurrent_jobs,
            running: Arc::clone(&self.running),
            active_jobs: Arc::clone(&self.active_jobs),
            tick_count: Arc::clone(&self.tick_count),
            last_tick_at: Arc::clone(&self.last_tick_at),
            tick_in_progress: Arc::clone(&self.tick_in_progress),
            shutdown_tx: Arc::clone(&self.shutdown_tx),
        }
    }
}

// ---------------------------------------------------------------------------
// sqlx FromRow 实现
// ---------------------------------------------------------------------------

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for PluginJob {
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            plugin_id: row.try_get("plugin_id")?,
            name: row.try_get("name")?,
            cron_schedule: row.try_get("cron_schedule")?,
            enabled: row.try_get("enabled")?,
            next_run_at: row.try_get("next_run_at")?,
            last_run_at: row.try_get("last_run_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for PluginJobRun {
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        let status_str: String = row.try_get("status")?;
        let status = match status_str.as_str() {
            "pending" => JobStatus::Pending,
            "running" => JobStatus::Running,
            "succeeded" => JobStatus::Succeeded,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Pending,
        };

        let trigger_str: String = row.try_get("trigger")?;
        let trigger = match trigger_str.as_str() {
            "scheduled" => JobTrigger::Scheduled,
            "manual" => JobTrigger::Manual,
            "retry" => JobTrigger::Retry,
            _ => JobTrigger::Manual,
        };

        Ok(Self {
            id: row.try_get("id")?,
            job_id: row.try_get("job_id")?,
            plugin_id: row.try_get("plugin_id")?,
            status,
            trigger,
            started_at: row.try_get("started_at")?,
            finished_at: row.try_get("finished_at")?,
            duration_ms: row.try_get("duration_ms")?,
            error: row.try_get("error")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_schedule_parsing() {
        // 每分钟
        let result = PluginJobScheduler::calculate_next_run("* * * * *", None);
        assert!(result.is_ok());

        // 每天午夜
        let result = PluginJobScheduler::calculate_next_run("0 0 * * *", None);
        assert!(result.is_ok());

        // 无效的 cron
        let result = PluginJobScheduler::calculate_next_run("invalid", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_job_priority_ordering() {
        assert!(JobPriority::Critical > JobPriority::High);
        assert!(JobPriority::High > JobPriority::Normal);
        assert!(JobPriority::Normal > JobPriority::Low);
    }
}
