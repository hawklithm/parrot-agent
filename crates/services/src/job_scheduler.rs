//! 后台调度器统一管理
//!
//! 提供 JobScheduler 统一管理所有定时后台任务
//! 对应 paperclip: server/src/index.ts:931-1040 (heartbeatSchedulerInterval)
//!
//! 主要任务：
//! - RoutineCronTrigger: 定时触发 routine（每 30 秒）
//! - MonitorCheckJob: 检查 issue monitor 健康（每分钟）
//! - LeaseExpiryScanner: 扫描过期租约（每分钟）
//! - EnvironmentHealthProber: 探测环境健康（每 5 分钟）
//! - ConsistencyCheckJob: 一致性检查（每小时）

use crate::DefaultHeartbeatService;
use crate::RoutineExecutionService;
use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{self, Duration};
use uuid::Uuid;

/// 最大补发运行次数（对应 paperclip MAX_CATCH_UP_RUNS）
const MAX_CATCH_UP_RUNS: usize = 25;

/// Monitor 调度：单次检查失败后指数退避（基础 60s，封顶 24h）。
pub fn monitor_backoff_seconds(attempt: i32) -> i64 {
    // 先限制指数避免 2^attempt 溢出，再整体封顶到 24h
    let exp = attempt.max(0).min(20) as u32;
    (60i64 * 2i64.pow(exp)).min(86_400)
}

/// 环境健康：空闲超过该时长视为失活（30 分钟）。
pub const ENV_IDLE_TIMEOUT: ChronoDuration = ChronoDuration::minutes(30);

/// 判断环境是否因空闲而失活。
pub fn is_env_stale(last_used_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match last_used_at {
        Some(lu) => lu < now - ENV_IDLE_TIMEOUT,
        None => true,
    }
}

/// 运行卡住判定：started_at 早于 timeout 且仍在运行/排队。
pub fn is_run_stuck(
    started_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    timeout: ChronoDuration,
) -> bool {
    match started_at {
        Some(s) => s < now - timeout,
        None => false,
    }
}

/// 写入一条 activity_log 审计记录（best-effort，失败仅记录日志）。
async fn record_activity(
    pool: &PgPool,
    company_id: Uuid,
    event_type: &str,
    actor_type: &str,
    actor_id: Uuid,
    resource_type: &str,
    resource_id: Uuid,
    metadata: serde_json::Value,
) {
    let id = Uuid::new_v4();
    if let Err(e) = sqlx::query(
        "INSERT INTO activity_logs (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())",
    )
    .bind(id)
    .bind(company_id)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .execute(pool)
    .await
    {
        tracing::warn!(company_id = %company_id, error = %e, "Failed to record scheduler activity log");
    }
}

/// 任务执行记录
#[derive(Debug, Clone)]
pub struct JobExecutionRecord {
    pub id: String,
    pub job_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
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
    /// 固定间隔(秒)
    IntervalSeconds(u64),
    /// Cron 表达式(预留)
    CronExpression(String),
    /// 事件驱动(预留)
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
}

pub struct HeartbeatRecoveryJob {
    heartbeat: Arc<DefaultHeartbeatService>,
}

impl HeartbeatRecoveryJob {
    pub fn new(heartbeat: Arc<DefaultHeartbeatService>) -> Self {
        Self { heartbeat }
    }
}

#[async_trait]
impl ScheduledJob for HeartbeatRecoveryJob {
    fn job_name(&self) -> &str {
        "heartbeat_recovery"
    }
    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }
    async fn execute(&self) -> Result<String, String> {
        let orphaned = self
            .heartbeat
            .reconcile_orphaned_runs(300)
            .await
            .map_err(|e| e.to_string())?;
        let pending = self
            .heartbeat
            .reconcile_pending_issues()
            .await
            .map_err(|e| e.to_string())?;
        let dependency_wakes = self
            .heartbeat
            .reconcile_dependency_wakeups()
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "reconciled {orphaned} orphaned runs, {pending} pending issues, and {dependency_wakes} dependency wakes"
        ))
    }
}

/// Job Scheduler 主调度器
pub struct JobScheduler {
    jobs: Arc<RwLock<HashMap<String, Arc<dyn ScheduledJob>>>>,
    executions: Arc<RwLock<Vec<JobExecutionRecord>>>,
}

impl JobScheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(Vec::new())),
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

    /// 启动调度器主循环
    /// 对应 paperclip: server/src/index.ts:931-1040
    pub async fn start(self: Arc<Self>, interval_ms: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_millis(interval_ms));

            loop {
                interval.tick().await;

                let jobs = self.jobs.read().await;
                for (name, job) in jobs.iter() {
                    let job = Arc::clone(job);
                    let scheduler = Arc::clone(&self);
                    let name = name.clone();

                    tokio::spawn(async move {
                        let started_at = Utc::now();
                        let result = job.execute().await;
                        let completed_at = Utc::now();

                        let (status, error_message) = match result {
                            Ok(_) => (JobStatus::Running, None),
                            Err(e) => (JobStatus::Failed, Some(e)),
                        };

                        scheduler
                            .record_execution(JobExecutionRecord {
                                id: Uuid::new_v4().to_string(),
                                job_name: name,
                                started_at,
                                completed_at: Some(completed_at),
                                status,
                                error_message,
                            })
                            .await;
                    });
                }
            }
        })
    }

    /// 手动触发任务
    pub async fn trigger_job(&self, job_name: &str) -> Result<String, String> {
        let job = self.get_job(job_name).await;
        match job {
            Some(j) => j.execute().await,
            None => Err(format!("Job '{}' not found", job_name)),
        }
    }
}

impl Default for JobScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Routine Cron Trigger Job
// ============================================================================

/// Routine Cron 触发器（每 30 秒）
/// 完整迁移自 paperclip: server/src/services/routines.ts:2734-2816
pub struct RoutineCronTrigger {
    pool: PgPool,
    routine_execution_service: Arc<RoutineExecutionService>,
}

impl RoutineCronTrigger {
    pub fn new(pool: PgPool, routine_execution_service: Arc<RoutineExecutionService>) -> Self {
        Self {
            pool,
            routine_execution_service,
        }
    }

    /// 计算下一个 cron tick（时区感知）
    /// 对应 paperclip: server/src/services/routines.ts:209-227
    fn next_cron_tick_in_timezone(
        &self,
        cron_expr: &str,
        timezone: &str,
        after: &DateTime<Utc>,
    ) -> Result<DateTime<Utc>, String> {
        use chrono_tz::Tz;
        use cron::Schedule;
        use std::str::FromStr;

        let schedule =
            Schedule::from_str(cron_expr).map_err(|e| format!("Invalid cron expression: {}", e))?;

        let tz: Tz = timezone
            .parse()
            .map_err(|e| format!("Invalid timezone: {}", e))?;

        let after_in_tz = after.with_timezone(&tz);

        // 找到下一个匹配的时间点
        schedule
            .after(&after_in_tz)
            .next()
            .map(|dt| dt.with_timezone(&Utc))
            .ok_or_else(|| "No next cron tick found".to_string())
    }
}

#[async_trait]
impl ScheduledJob for RoutineCronTrigger {
    fn job_name(&self) -> &str {
        "routine_cron_trigger"
    }

    fn schedule(&self) -> JobSchedule {
        // 对应 paperclip config.heartbeatSchedulerIntervalMs (默认 30000ms)
        JobSchedule::IntervalSeconds(30)
    }

    /// 执行定时触发逻辑
    /// 完整对应 paperclip: server/src/services/routines.ts:2734-2816
    async fn execute(&self) -> Result<String, String> {
        let now = Utc::now();

        // 1. 查询到期的触发器
        // 对应 paperclip L2735-2753
        let due_triggers = sqlx::query(
            r#"
            SELECT 
                rt.id as trigger_id,
                rt.routine_id,
                rt.cron_expression,
                rt.timezone,
                rt.next_run_at,
                r.id as routine_id_check,
                r.company_id,
                r.status as routine_status,
                r.catch_up_policy,
                r.project_id,
                p.paused_at as project_paused_at
            FROM routine_triggers rt
            INNER JOIN routines r ON rt.routine_id = r.id
            LEFT JOIN projects p ON r.project_id = p.id
            WHERE rt.kind = 'schedule'
              AND rt.enabled = true
              AND r.status = 'active'
              AND rt.next_run_at IS NOT NULL
              AND rt.next_run_at <= $1
            ORDER BY rt.next_run_at ASC, rt.created_at ASC
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query due triggers: {}", e))?;

        let mut triggered = 0;

        for trigger in due_triggers {
            let trigger_id: Uuid = trigger
                .try_get("trigger_id")
                .map_err(|e| format!("Failed to get trigger_id: {}", e))?;
            let routine_id: Uuid = trigger
                .try_get("routine_id")
                .map_err(|e| format!("Failed to get routine_id: {}", e))?;
            let cron_expr: Option<String> = trigger
                .try_get("cron_expression")
                .map_err(|e| format!("Failed to get cron_expression: {}", e))?;
            let timezone: Option<String> = trigger
                .try_get("timezone")
                .map_err(|e| format!("Failed to get timezone: {}", e))?;
            let next_run_at: Option<DateTime<Utc>> = trigger
                .try_get("next_run_at")
                .map_err(|e| format!("Failed to get next_run_at: {}", e))?;
            let company_id: Uuid = trigger
                .try_get("company_id")
                .map_err(|e| format!("Failed to get company_id: {}", e))?;
            let catch_up_policy: Option<String> = trigger
                .try_get("catch_up_policy")
                .map_err(|e| format!("Failed to get catch_up_policy: {}", e))?;
            let project_paused_at: Option<DateTime<Utc>> = trigger
                .try_get("project_paused_at")
                .map_err(|e| format!("Failed to get project_paused_at: {}", e))?;

            let Some(next_run_at) = next_run_at else {
                continue;
            };
            let Some(ref cron_expr) = cron_expr else {
                continue;
            };
            let Some(ref timezone) = timezone else {
                continue;
            };

            // 2. 检查项目是否暂停（对应 paperclip L2759-2763）
            let project_paused = project_paused_at.is_some();

            // 3. 计算下一次运行时间和补发次数
            // 对应 paperclip L2765-2776
            let mut run_count = 1;
            let mut claimed_next_run_at = self
                .next_cron_tick_in_timezone(cron_expr, timezone, &now)
                .map_err(|e| {
                    tracing::warn!(
                        trigger_id = %trigger_id,
                        error = %e,
                        "Failed to calculate next cron tick"
                    );
                    e
                })?;

            // 4. 处理 catch-up policy（补发错过的运行）
            if !project_paused && catch_up_policy.as_deref() == Some("enqueue_missed_with_cap") {
                let mut cursor = next_run_at;
                run_count = 0;

                while cursor <= now && run_count < MAX_CATCH_UP_RUNS {
                    run_count += 1;
                    match self.next_cron_tick_in_timezone(cron_expr, timezone, &cursor) {
                        Ok(next) => {
                            claimed_next_run_at = next;
                            cursor = next;
                        }
                        Err(e) => {
                            tracing::warn!(
                                trigger_id = %trigger_id,
                                error = %e,
                                "Failed to calculate catch-up tick"
                            );
                            break;
                        }
                    }
                }
            }

            // 5. 乐观锁：认领触发器（对应 paperclip L2778-2793）
            let claimed = sqlx::query(
                r#"
                UPDATE routine_triggers
                SET next_run_at = $1, updated_at = NOW()
                WHERE id = $2 
                  AND enabled = true 
                  AND next_run_at = $3
                RETURNING id
                "#,
            )
            .bind(claimed_next_run_at)
            .bind(trigger_id)
            .bind(next_run_at)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| format!("Failed to claim trigger: {}", e))?;

            if claimed.is_none() {
                // 另一个 worker 已经认领了这个触发器
                continue;
            }

            // 6. 如果项目暂停，记录跳过的运行（对应 paperclip L2795-2802）
            if project_paused {
                let _ = sqlx::query(
                    r#"
                    INSERT INTO routine_runs 
                    (company_id, routine_id, trigger_id, source, status, triggered_at, 
                     failure_reason, completed_at, routine_revision_id)
                    VALUES ($1, $2, $3, 'schedule', 'skipped', NOW(), 'paused', NOW(), NULL)
                    "#,
                )
                .bind(company_id)
                .bind(routine_id)
                .bind(trigger_id)
                .execute(&self.pool)
                .await;

                continue;
            }

            // 7. 调度运行（可能多次，处理 catch-up）
            // 对应 paperclip L2805-2812
            for _ in 0..run_count {
                match self
                    .routine_execution_service
                    .dispatch_routine_run(crate::DispatchRoutineRunInput {
                        routine_id,
                        trigger_id: Some(trigger_id),
                        source: crate::RoutineRunSource::Schedule,
                        payload: None,
                        variables: None,
                        idempotency_key: None,
                        project_id: None,
                        assignee_agent_id: None,
                        actor_user_id: None,
                        actor_agent_id: None,
                    })
                    .await
                {
                    Ok(_) => triggered += 1,
                    Err(e) => {
                        tracing::error!(
                            routine_id = %routine_id,
                            trigger_id = %trigger_id,
                            error = %e,
                            "Failed to dispatch routine run"
                        );
                    }
                }
            }
        }

        Ok(format!("Triggered {} routines", triggered))
    }
}

// ============================================================================
// Monitor Check Job
// ============================================================================

/// Monitor 定时检查器（每分钟）
/// 对应 paperclip: 检查 issues 中 monitor_next_check_at < NOW()
pub struct MonitorCheckJob {
    pool: PgPool,
}

impl MonitorCheckJob {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for MonitorCheckJob {
    fn job_name(&self) -> &str {
        "monitor_check"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        let now = Utc::now();

        // 查询需要检查的 monitor issues
        let due_monitors = sqlx::query(
            r#"
            SELECT id, company_id, monitor_attempt_count
            FROM issues
            WHERE monitor_next_check_at IS NOT NULL
              AND monitor_next_check_at <= $1
              AND deleted_at IS NULL
            LIMIT 100
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query due monitors: {}", e))?;

        let mut checked = 0usize;
        let mut stuck = 0usize;
        const MAX_ATTEMPTS: i32 = 10;

        for row in &due_monitors {
            let id: Uuid = row.try_get("id").map_err(|e| e.to_string())?;
            let company_id: Uuid = row.try_get("company_id").map_err(|e| e.to_string())?;
            let attempt: i32 = row.try_get("monitor_attempt_count").unwrap_or(0);
            let next_attempt = attempt + 1;

            if next_attempt >= MAX_ATTEMPTS {
                // 卡住：停止调度并记录，避免无限重试
                sqlx::query(
                    r#"
                    UPDATE issues
                    SET monitor_last_triggered_at = $2,
                        monitor_attempt_count = $3,
                        monitor_next_check_at = NULL,
                        monitor_notes = 'exceeded max monitor attempts'
                    WHERE id = $1
                    "#,
                )
                .bind(id)
                .bind(now)
                .bind(next_attempt)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;

                record_activity(
                    &self.pool,
                    company_id,
                    "monitor_stuck",
                    "system",
                    Uuid::nil(),
                    "issue",
                    id,
                    serde_json::json!({ "attempts": next_attempt }),
                )
                .await;
                stuck += 1;
            } else {
                // 真实检查：推进调度并写入结果
                let next_check = now + ChronoDuration::seconds(monitor_backoff_seconds(attempt));
                sqlx::query(
                    r#"
                    UPDATE issues
                    SET monitor_last_triggered_at = $2,
                        monitor_attempt_count = $3,
                        monitor_next_check_at = $4
                    WHERE id = $1
                    "#,
                )
                .bind(id)
                .bind(now)
                .bind(next_attempt)
                .bind(next_check)
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;

                record_activity(
                    &self.pool,
                    company_id,
                    "monitor_check",
                    "system",
                    Uuid::nil(),
                    "issue",
                    id,
                    serde_json::json!({ "attempts": next_attempt, "next_check_at": next_check }),
                )
                .await;
            }
            checked += 1;
        }

        Ok(format!("Checked {} monitors ({} stuck)", checked, stuck))
    }
}

// ============================================================================
// Lease Expiry Scanner
// ============================================================================

/// 租约过期扫描器（每分钟）
/// 清理过期的 environment leases
pub struct LeaseExpiryScanner {
    pool: PgPool,
}

impl LeaseExpiryScanner {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for LeaseExpiryScanner {
    fn job_name(&self) -> &str {
        "lease_expiry_scanner"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        let now = Utc::now();

        // 清理过期的租约
        let result = sqlx::query(
            r#"
            DELETE FROM environment_leases
            WHERE expires_at < $1
            RETURNING id
            "#,
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to clean expired leases: {}", e))?;

        Ok(format!("Cleaned {} expired leases", result.len()))
    }
}

// ============================================================================
// Environment Health Prober
// ============================================================================

/// 环境健康探测器（每 5 分钟）
/// 探测运行中的环境健康状态
pub struct EnvironmentHealthProber {
    pool: PgPool,
}

impl EnvironmentHealthProber {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for EnvironmentHealthProber {
    fn job_name(&self) -> &str {
        "environment_health_prober"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(300)
    }

    async fn execute(&self) -> Result<String, String> {
        let now = Utc::now();

        // 查询活跃的环境
        let active_envs = sqlx::query(
            r#"
            SELECT id, company_id, status, last_used_at
            FROM execution_workspaces
            WHERE status IN ('running', 'starting')
              AND deleted_at IS NULL
            LIMIT 100
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query active environments: {}", e))?;

        let mut probed = 0usize;
        let mut flagged = 0usize;

        for row in &active_envs {
            let id: Uuid = row.try_get("id").map_err(|e| e.to_string())?;
            let company_id: Uuid = row.try_get("company_id").map_err(|e| e.to_string())?;
            let status: String = row.try_get("status").map_err(|e| e.to_string())?;
            let last_used_at: Option<DateTime<Utc>> =
                row.try_get("last_used_at").map_err(|e| e.to_string())?;

            probed += 1;

            if is_env_stale(last_used_at, now) {
                // 真实探测失败：标记可回收并触发恢复（记录审计）
                sqlx::query(
                    r#"
                    UPDATE execution_workspaces
                    SET cleanup_eligible_at = $2,
                        cleanup_reason = 'health_idle_stale',
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(id)
                .bind(now + ChronoDuration::minutes(5))
                .execute(&self.pool)
                .await
                .map_err(|e| e.to_string())?;

                record_activity(
                    &self.pool,
                    company_id,
                    "environment_health_failed",
                    "system",
                    Uuid::nil(),
                    "environment",
                    id,
                    serde_json::json!({ "status": status, "last_used_at": last_used_at }),
                )
                .await;
                flagged += 1;
            } else {
                // 健康：刷新 updated_at 作为探测心跳
                sqlx::query("UPDATE execution_workspaces SET updated_at = NOW() WHERE id = $1")
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }

        Ok(format!(
            "Probed {} environments ({} flagged for recovery)",
            probed, flagged
        ))
    }
}

// ============================================================================
// Stuck Run Detector
// ============================================================================

/// 卡住运行检测器（每 2 分钟）
/// 检测 heartbeat_runs 中仍在 running/queued 但已超过超时阈值的运行，
/// 将其标记为 timed_out（取消），并写入恢复审计记录（P0.5）。
pub struct StuckRunDetector {
    pool: PgPool,
}

impl StuckRunDetector {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 运行被视为卡住的超时阈值（30 分钟）。
    pub const STUCK_TIMEOUT: ChronoDuration = ChronoDuration::minutes(30);
}

#[async_trait]
impl ScheduledJob for StuckRunDetector {
    fn job_name(&self) -> &str {
        "stuck_run_detector"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(120)
    }

    async fn execute(&self) -> Result<String, String> {
        let now = Utc::now();
        let timeout = now - Self::STUCK_TIMEOUT;

        let stuck_runs = sqlx::query(
            r#"
            SELECT id, company_id, agent_id, status
            FROM heartbeat_runs
            WHERE status IN ('running', 'queued')
              AND started_at IS NOT NULL
              AND started_at < $1
            LIMIT 200
            "#,
        )
        .bind(timeout)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query stuck runs: {}", e))?;

        let mut recovered = 0usize;
        for row in &stuck_runs {
            let id: Uuid = row.try_get("id").map_err(|e| e.to_string())?;
            let company_id: Uuid = row.try_get("company_id").map_err(|e| e.to_string())?;
            let agent_id: Uuid = row.try_get("agent_id").map_err(|e| e.to_string())?;

            sqlx::query(
                r#"
                UPDATE heartbeat_runs
                SET status = 'timed_out',
                    error = 'stuck run detected by watchdog',
                    finished_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;

            record_activity(
                &self.pool,
                company_id,
                "run_recovery",
                "system",
                Uuid::nil(),
                "agent",
                agent_id,
                serde_json::json!({ "run_id": id.to_string(), "action": "timed_out" }),
            )
            .await;
            recovered += 1;
        }

        Ok(format!("Recovered {} stuck runs", recovered))
    }
}

// ============================================================================
// Consistency Check Job
// ============================================================================

/// 状态一致性检查器（每小时）
/// 检查并修复数据一致性问题
pub struct ConsistencyCheckJob {
    pool: PgPool,
}

impl ConsistencyCheckJob {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for ConsistencyCheckJob {
    fn job_name(&self) -> &str {
        "consistency_check"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(3600)
    }

    async fn execute(&self) -> Result<String, String> {
        let mut checks = Vec::new();

        // 检查 1: 孤儿 routine_runs
        let orphaned_runs = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM routine_runs rr
            LEFT JOIN routines r ON rr.routine_id = r.id
            WHERE r.id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to check orphaned runs: {}", e))?;

        if orphaned_runs > 0 {
            checks.push(format!("Found {} orphaned runs", orphaned_runs));
        }

        // 检查 2: 悬挂的 issue 执行
        let dangling_issues = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM issues
            WHERE origin_kind = 'routine_execution'
              AND origin_run_id IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM routine_runs 
                  WHERE id = issues.origin_run_id
              )
              AND deleted_at IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| format!("Failed to check dangling issues: {}", e))?;

        if dangling_issues > 0 {
            checks.push(format!("Found {} dangling issues", dangling_issues));
        }

        if checks.is_empty() {
            Ok("All consistency checks passed".to_string())
        } else {
            Ok(format!("Consistency issues: {}", checks.join(", ")))
        }
    }
}

// ============================================================================
// Status Card Scheduler Job（对应 paperclip index.ts status-card scheduler tick）
// ============================================================================

/// Status Card 调度器（每 30 秒）。
/// 扫描 next_eval_at 到期的卡片触发后台 refresh，并顺带做 stalled-generation
/// finalization（对应 paperclip tickDueStatusCards + finalizeStatusCardsForStalledGeneration）。
pub struct StatusCardSchedulerJob {
    pool: PgPool,
}

impl StatusCardSchedulerJob {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for StatusCardSchedulerJob {
    fn job_name(&self) -> &str {
        "status_card_scheduler"
    }

    fn schedule(&self) -> JobSchedule {
        // 对应 paperclip config.heartbeatSchedulerIntervalMs (默认 30000ms)
        JobSchedule::IntervalSeconds(30)
    }

    async fn execute(&self) -> Result<String, String> {
        let worker = crate::status_card_worker::StatusCardWorker::new(self.pool.clone());
        let now = chrono::Utc::now();
        let (evaluated, enqueued) = worker.tick_due_status_cards(&now).await?;
        let finalized = worker.finalize_stalled_generations().await?;
        Ok(format!(
            "Status-card tick: evaluated={} enqueued={} finalized={}",
            evaluated, enqueued, finalized
        ))
    }
}

// ============================================================================
// Summary Slot Finalizer Job（对应 paperclip finalizeSummarySlotsForTerminalIssue）
// ============================================================================

/// Summary Slot 终态 finalizer（每 60 秒）。
/// 将 generating_issue 已到终态（done/cancelled）的 slot 置为 failed。
pub struct SummarySlotFinalizerJob {
    pool: PgPool,
}

impl SummarySlotFinalizerJob {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScheduledJob for SummarySlotFinalizerJob {
    fn job_name(&self) -> &str {
        "summary_slot_finalizer"
    }

    fn schedule(&self) -> JobSchedule {
        JobSchedule::IntervalSeconds(60)
    }

    async fn execute(&self) -> Result<String, String> {
        let worker = crate::summary_slot_worker::SummarySlotWorker::new(self.pool.clone());
        let finalized = worker.finalize_terminal_issues().await?;
        Ok(format!("Summary-slot finalizer: finalized={}", finalized))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn monitor_backoff_grows_exponentially_and_caps() {
        assert_eq!(monitor_backoff_seconds(0), 60);
        assert_eq!(monitor_backoff_seconds(1), 120);
        assert_eq!(monitor_backoff_seconds(3), 480);
        // 封顶 24h
        assert_eq!(monitor_backoff_seconds(100), 86_400);
    }

    #[test]
    fn env_stale_when_idle_or_missing() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        // 30 分钟前使用 -> 失活
        let old = now - ChronoDuration::minutes(31);
        assert!(is_env_stale(Some(old), now));
        // 刚刚使用 -> 健康
        let fresh = now - ChronoDuration::minutes(5);
        assert!(!is_env_stale(Some(fresh), now));
        // 无记录 -> 失活
        assert!(is_env_stale(None, now));
    }

    #[test]
    fn run_stuck_when_started_before_timeout() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let timeout = ChronoDuration::minutes(30);
        let started_old = now - ChronoDuration::minutes(31);
        assert!(is_run_stuck(Some(started_old), now, timeout));
        let started_recent = now - ChronoDuration::minutes(10);
        assert!(!is_run_stuck(Some(started_recent), now, timeout));
        // 未开始 -> 不卡住
        assert!(!is_run_stuck(None, now, timeout));
    }
}
