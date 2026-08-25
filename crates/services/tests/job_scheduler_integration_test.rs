//! Job Scheduler 集成测试
//!
//! 验证后台调度器的核心功能：
//! 1. RoutineCronTrigger 正确触发 routine
//! 2. Catch-up policy 正常工作
//! 3. 并发策略（skip_if_active, coalesce）正确处理
//! 4. 乐观锁防止重复触发

use services::{
    JobScheduler, RoutineCronTrigger, RoutineExecutionService, RoutineServiceImpl, ScheduledJob,
};
use repositories::RoutineRepository;
use std::sync::Arc;
use sqlx::PgPool;
use chrono::Utc;
use tokio::sync::Mutex as AsyncMutex;

// 三个调度器测试共享 routine_triggers/routines/routine_runs 表且各自 DELETE ALL 清理，
// 必须串行执行以避免并行竞态（一个测试的清理误删另一个测试正在派发的触发器）。
static TEST_LOCK: std::sync::LazyLock<AsyncMutex<()>> =
    std::sync::LazyLock::new(|| AsyncMutex::new(()));

#[tokio::test]
async fn test_routine_cron_trigger_basic() {
    let _guard = TEST_LOCK.lock().await;

    // 使用回归数据库（与 parity 测试一致）；缺失则跳过而非失败。
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string()
    });

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("skipping test_routine_cron_trigger_basic: no DATABASE_URL reachable");
            return;
        }
    };
    
    // 清理 routine 相关表（company/agent 用随机 UUID，不删除以免触碰共享库其他数据外键）
    sqlx::query("DELETE FROM routine_runs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routine_triggers").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routines").execute(&pool).await.unwrap();
    
    // 创建测试数据
    let company_id = uuid::Uuid::new_v4();
    let issue_prefix = format!("T{}", &company_id.simple().to_string()[..6]);

    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'Test Company', $2)"
    )
    .bind(company_id)
    .bind(&issue_prefix)
    .execute(&pool)
    .await
    .unwrap();
    
    let agent_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Test Agent')"
    )
    .bind(agent_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();
    
    let routine_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO routines 
        (id, company_id, agent_id, assignee_agent_id, name, title, status, concurrency_policy, catch_up_policy)
        VALUES ($1, $2, $3, $3, 'Test Routine', 'Test Routine', 'active', 'coalesce_if_active', 'skip_missed')
        "#
    )
    .bind(routine_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .unwrap();
    
    let trigger_id = uuid::Uuid::new_v4();
    let now = Utc::now();
    let past = now - chrono::Duration::minutes(5); // 5 分钟前应该触发
    
    sqlx::query(
        r#"
        INSERT INTO routine_triggers
        (id, routine_id, company_id, kind, trigger_type, status, enabled, cron_expression, timezone, next_run_at)
        VALUES ($1, $2, $4, 'schedule', 'cron', 'active', true, '*/5 * * * *', 'UTC', $3)
        "#
    )
    .bind(trigger_id)
    .bind(routine_id)
    .bind(past)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    // 创建服务（经 RoutineService 单一路径派发，复用并发策略/idempotency/fingerprint）
    let routine_repo: Arc<dyn RoutineRepository> =
        Arc::new(repositories::routine_repository::PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_service = Arc::new(RoutineExecutionService::new(Arc::new(
        RoutineServiceImpl::new(routine_repo),
    )));
    let cron_trigger = RoutineCronTrigger::new(pool.clone(), routine_execution_service);
    
    // 执行触发
    let result = cron_trigger.execute().await;
    
    // 验证结果
    assert!(result.is_ok(), "Cron trigger should execute successfully: {:?}", result);
    let message = result.unwrap();
    assert!(message.contains("Triggered 1 routines"), "Should trigger 1 routine, got: {}", message);
    
    // 验证 routine_runs 表中有记录
    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_runs WHERE routine_id = $1"
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    
    assert_eq!(run_count, 1, "Should have 1 routine run");
    // 验证派发的 run 经过 RoutineService 单一路径：状态为 queued（always_enqueue 且无活跃 run），
    // 且写入了 dispatch_fingerprint（并发策略/idempotency 模型的一部分）。
    let (status, dispatch_fingerprint): (String, Option<String>) = sqlx::query_as(
        "SELECT status::text, dispatch_fingerprint FROM routine_runs WHERE routine_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "queued", "scheduled run via fire_routine should be queued");
    assert!(
        dispatch_fingerprint.is_some(),
        "scheduled run should carry a dispatch_fingerprint (dispatch path aligned)"
    );
    
    // 验证 trigger 的 next_run_at 已更新
    let updated_next_run: chrono::DateTime<Utc> = sqlx::query_scalar(
        "SELECT next_run_at FROM routine_triggers WHERE id = $1"
    )
    .bind(trigger_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    
    assert!(updated_next_run > now, "next_run_at should be updated to future");
    
    // 清理
    pool.close().await;
}

#[tokio::test]
async fn test_routine_catch_up_policy() {
    let _guard = TEST_LOCK.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string());

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping test_routine_catch_up_policy: no database available");
            return;
        }
    };

    // 清理 routine 相关表（company/agent 用随机 UUID，不删除以免触碰共享库其他数据外键）
    sqlx::query("DELETE FROM routine_runs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routine_triggers").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routines").execute(&pool).await.unwrap();

    let company_id = uuid::Uuid::new_v4();
    let issue_prefix = format!("T{}", &company_id.simple().to_string()[..6]);
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'Test Company', $2)",
    )
    .bind(company_id)
    .bind(&issue_prefix)
    .execute(&pool)
    .await
    .unwrap();

    let agent_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Test Agent')",
    )
    .bind(agent_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    let routine_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO routines
        (id, company_id, agent_id, assignee_agent_id, name, title, status, concurrency_policy, catch_up_policy)
        VALUES ($1, $2, $3, $3, 'Catch-up Test Routine', 'Catch-up Test Routine', 'active', 'coalesce_if_active', 'run_missed')
        "#,
    )
    .bind(routine_id)
    .bind(company_id)
    .bind(agent_id)
    .execute(&pool)
    .await
    .unwrap();

    let trigger_id = uuid::Uuid::new_v4();
    let now = Utc::now();
    let past = now - chrono::Duration::hours(2); // 2 小时前（每 5 分钟一次 ≈ 24 次错过）

    sqlx::query(
        r#"
        INSERT INTO routine_triggers
        (id, routine_id, company_id, kind, trigger_type, status, enabled, cron_expression, timezone, next_run_at)
        VALUES ($1, $2, $4, 'schedule', 'cron', 'active', true, '*/5 * * * *', 'UTC', $3)
        "#,
    )
    .bind(trigger_id)
    .bind(routine_id)
    .bind(past)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    // 执行触发
    let routine_repo: Arc<dyn RoutineRepository> =
        Arc::new(repositories::routine_repository::PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_service = Arc::new(RoutineExecutionService::new(Arc::new(
        RoutineServiceImpl::new(routine_repo),
    )));
    let cron_trigger = RoutineCronTrigger::new(pool.clone(), routine_execution_service);
    let result = cron_trigger.execute().await;

    assert!(result.is_ok(), "Catch-up trigger should succeed: {:?}", result);

    // 验证补发了多个运行（最多 25 次）
    let run_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_runs WHERE routine_id = $1"
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // 2 小时 / 5 分钟 ≈ 24 次，应该全部补发（上限 25）
    assert!(run_count >= 20 && run_count <= 25, "Should catch up multiple runs (got {})", run_count);

    pool.close().await;
}


#[tokio::test]
async fn test_routine_project_paused() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:admin123@127.0.0.1:5433/parrot_agent_compile".to_string());
    let _guard = TEST_LOCK.lock().await;


    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping test_routine_project_paused: no database available");
            return;
        }
    };

    // 清理 routine 相关表（company/agent 用随机 UUID，不删除以免触碰共享库其他数据外键）
    sqlx::query("DELETE FROM routine_runs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routine_triggers").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM routines").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM projects").execute(&pool).await.unwrap();

    let company_id = uuid::Uuid::new_v4();
    let issue_prefix = format!("T{}", &company_id.simple().to_string()[..6]);
    sqlx::query(
        "INSERT INTO companies (id, name, issue_prefix) VALUES ($1, 'Test Company', $2)",
    )
    .bind(company_id)
    .bind(&issue_prefix)
    .execute(&pool)
    .await
    .unwrap();

    let agent_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO agents (id, company_id, name) VALUES ($1, $2, 'Test Agent')",
    )
    .bind(agent_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    let project_id = uuid::Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO projects (id, company_id, name, paused_at) VALUES ($1, $2, 'Paused Project', $3)",
    )
    .bind(project_id)
    .bind(company_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let routine_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO routines
        (id, company_id, agent_id, assignee_agent_id, name, title, status, project_id, concurrency_policy, catch_up_policy)
        VALUES ($1, $2, $3, $3, 'Paused Project Routine', 'Paused Project Routine', 'active', $4, 'coalesce_if_active', 'skip_missed')
        "#,
    )
    .bind(routine_id)
    .bind(company_id)
    .bind(agent_id)
    .bind(project_id)
    .execute(&pool)
    .await
    .unwrap();

    let trigger_id = uuid::Uuid::new_v4();
    let past = now - chrono::Duration::minutes(5);
    sqlx::query(
        r#"
        INSERT INTO routine_triggers
        (id, routine_id, company_id, kind, trigger_type, status, enabled, cron_expression, timezone, next_run_at)
        VALUES ($1, $2, $4, 'schedule', 'cron', 'active', true, '*/5 * * * *', 'UTC', $3)
        "#,
    )
    .bind(trigger_id)
    .bind(routine_id)
    .bind(past)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();

    // 执行触发
    let routine_repo: Arc<dyn RoutineRepository> =
        Arc::new(repositories::routine_repository::PostgresRoutineRepository::new(pool.clone()));
    let routine_execution_service = Arc::new(RoutineExecutionService::new(Arc::new(
        RoutineServiceImpl::new(routine_repo),
    )));
    let cron_trigger = RoutineCronTrigger::new(pool.clone(), routine_execution_service);
    let result = cron_trigger.execute().await;

    assert!(result.is_ok(), "Should skip paused project: {:?}", result);

    // 验证创建了 skipped 运行记录
    let skipped_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routine_runs WHERE routine_id = $1 AND status = 'skipped' AND failure_reason = 'paused'",
    )
    .bind(routine_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(skipped_count, 1, "Should have 1 skipped run for paused project");

    // 验证没有创建 issue
    let issue_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM issues WHERE origin_kind = 'routine_execution' AND origin_id = $1::text",
    )
    .bind(routine_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(issue_count, 0, "Should not create issue for paused project");

    pool.close().await;
}

#[test]
fn test_job_scheduler_basic() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    rt.block_on(async {
        let scheduler = JobScheduler::new();
        
        // 验证初始状态
        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 0, "Scheduler should start empty");
        
        // 验证注册功能
        // TODO: 需要创建一个测试用的 ScheduledJob 实现
    });
}
