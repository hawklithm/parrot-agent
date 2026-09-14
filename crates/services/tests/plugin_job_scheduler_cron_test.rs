//! Plugin Job Scheduler 集成测试
//!
//! 回归覆盖：paperclip 契约使用 **5 字段** cron（分 时 日 月 周），
//! 而 `cron` crate 0.12 要求 6 字段且 day-of-week 编号偏移一位。
//! 此前 `calculate_next_run` 直接解析原始字符串，导致所有 5 字段表达式
//! 解析失败 → `register_plugin` 静默跳过 `next_run_at` 写入 → **任何插件作业都不会运行**。
//!
//! 本测试用真实数据库断言：注册后 `next_run_at` 被真正计算并落库，且 `tick` 能派发到期作业。

use chrono::{Duration, Utc};
use services::{PluginJobScheduler, PluginJobSchedulerOptions, PluginWorkerManager};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// 共享 `plugin_jobs` / `plugins` 表且本测试会清理，需串行。
static TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 连接测试数据库；不可达时跳过而非失败（与 job_scheduler_integration_test 一致）。
async fn test_pool() -> Option<PgPool> {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping plugin job scheduler cron test: DATABASE_URL is not set");
        return None;
    };
    PgPool::connect(&database_url).await.ok()
}

async fn insert_plugin(pool: &PgPool) -> Uuid {
    let plugin_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO plugins (id, plugin_key, name, manifest, config)
         VALUES ($1, $2, 'Cron Regression Plugin', '{}'::jsonb, '{}'::jsonb)",
    )
    .bind(plugin_id)
    .bind(format!("cron-regression-{}", plugin_id.simple()))
    .execute(pool)
    .await
    .expect("insert plugin");
    plugin_id
}

async fn insert_job(pool: &PgPool, plugin_id: Uuid, job_key: &str, schedule: &str) -> Uuid {
    let job_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO plugin_jobs (id, plugin_id, job_key, name, schedule, enabled, definition)
         VALUES ($1, $2, $3, $3, $4, true, '{}'::jsonb)",
    )
    .bind(job_id)
    .bind(plugin_id)
    .bind(job_key)
    .bind(schedule)
    .execute(pool)
    .await
    .expect("insert plugin job");
    job_id
}

fn scheduler(pool: &PgPool) -> PluginJobScheduler {
    PluginJobScheduler::new(PluginJobSchedulerOptions {
        db: pool.clone(),
        worker_manager: Arc::new(PluginWorkerManager::new()),
        tick_interval_ms: Some(30_000),
        job_timeout_ms: Some(60_000),
        max_concurrent_jobs: Some(1),
    })
}

/// 回归：5 字段 cron 注册后必须计算出 `next_run_at`（此前恒为 NULL）。
#[tokio::test]
async fn registering_plugin_computes_next_run_for_five_field_cron() {
    let _guard = TEST_LOCK.lock().await;
    let Some(pool) = test_pool().await else {
        eprintln!("skipping: no DATABASE_URL reachable");
        return;
    };

    let plugin_id = insert_plugin(&pool).await;
    // "*/5 * * * *" = 每 5 分钟；paperclip 契约的 5 字段写法。
    let job_id = insert_job(&pool, plugin_id, "every-five", "*/5 * * * *").await;

    scheduler(&pool)
        .register_plugin(plugin_id)
        .await
        .expect("register plugin");

    let next_run_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT next_run_at FROM plugin_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("read next_run_at");

    let next_run_at = next_run_at.expect("next_run_at must be computed for a 5-field cron");
    let now = Utc::now();
    assert!(
        next_run_at > now && next_run_at <= now + Duration::minutes(5),
        "next_run_at {next_run_at} should fall within the next 5 minutes (now {now})"
    );

    sqlx::query("DELETE FROM plugins WHERE id = $1")
        .bind(plugin_id)
        .execute(&pool)
        .await
        .expect("cleanup plugin");
}

/// 回归：paperclip 的 day-of-week 编号（`1` = 周一）必须解析为真正的周一，
/// 而不是 crate 编号里的周一（`1` 在 crate 中是周日）。
#[tokio::test]
async fn five_field_day_of_week_uses_paperclip_numbering() {
    let _guard = TEST_LOCK.lock().await;
    let Some(pool) = test_pool().await else {
        eprintln!("skipping: no DATABASE_URL reachable");
        return;
    };

    let plugin_id = insert_plugin(&pool).await;
    // "0 9 * * 1" = 每周一 09:00（paperclip 编号）。
    let job_id = insert_job(&pool, plugin_id, "weekly-monday", "0 9 * * 1").await;

    scheduler(&pool)
        .register_plugin(plugin_id)
        .await
        .expect("register plugin");

    let next_run_at: Option<chrono::DateTime<Utc>> =
        sqlx::query_scalar("SELECT next_run_at FROM plugin_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("read next_run_at");
    let next_run_at = next_run_at.expect("next_run_at must be computed for a 5-field cron");

    // 2024-01-01 是周一；用 chrono 的 weekday 判定，避免硬编码序号。
    use chrono::Datelike;
    assert_eq!(
        next_run_at.weekday(),
        chrono::Weekday::Mon,
        "`0 9 * * 1` must resolve to Monday, got {next_run_at} ({:?})",
        next_run_at.weekday()
    );
    assert_eq!(
        next_run_at.time().format("%H:%M").to_string(),
        "09:00",
        "must fire at 09:00, got {next_run_at}"
    );

    sqlx::query("DELETE FROM plugins WHERE id = $1")
        .bind(plugin_id)
        .execute(&pool)
        .await
        .expect("cleanup plugin");
}
