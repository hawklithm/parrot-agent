//! Verify Job Scheduler
//!
//! 验证 JobScheduler 是否正常工作的示例程序
//! 运行: cargo run --example verify_scheduler

use services::{JobScheduler, RoutineCronTrigger, RoutineExecutionService};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .init();

    tracing::info!("Starting Job Scheduler verification...");

    // 连接数据库
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/parrot".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Connected to database");

    // 创建 scheduler
    let scheduler = Arc::new(JobScheduler::new());

    // 创建 RoutineCronTrigger
    let routine_service = Arc::new(RoutineExecutionService::new(pool.clone()));
    let routine_trigger = RoutineCronTrigger::new(pool.clone(), routine_service);
    
    scheduler.register(Arc::new(routine_trigger)).await;

    tracing::info!("Registered RoutineCronTrigger job");

    // 启动 scheduler (每 5 秒检查一次，方便观察)
    tracing::info!("Starting scheduler...");
    let handle = scheduler.clone().start(5000).await;

    // 运行 60 秒
    tracing::info!("Scheduler is running. Will run for 60 seconds...");
    tracing::info!("Check the database for routine_runs and issues:");
    tracing::info!("  psql -d parrot -c \"SELECT id, routine_id, source, status, triggered_at FROM routine_runs ORDER BY triggered_at DESC LIMIT 10;\"");
    
    sleep(Duration::from_secs(60)).await;

    // 停止 scheduler
    tracing::info!("Stopping scheduler...");
    handle.abort();

    // 打印统计信息
    let records = scheduler.get_recent_executions(20).await;
    tracing::info!("Execution history ({} records):", records.len());
    for record in records.iter() {
        tracing::info!(
            "  {} - {} - {:?} - error: {:?}",
            record.job_name,
            record.started_at.format("%H:%M:%S"),
            record.status,
            record.error_message
        );
    }

    tracing::info!("Verification complete");

    Ok(())
}
