//! Parrot Agent server entry point.
//!
//! Builds the dependency graph (repositories -> services -> AppState),
//! runs migrations, and serves the Axum router produced by `api::create_router`.

mod systemd_notify;

use std::sync::Arc;

use api::create_router;
use axum::Router;
use repositories::{
    budget_repository::{PgBudgetIncidentRepository, PgBudgetPolicyRepository},
    company_repository::CompanyRepository,
    cost_event_repository::PgCostEventRepository,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

async fn ensure_local_trusted_principal(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let configured_user_id = std::env::var("LOCAL_TRUSTED_USER_ID")
        .ok()
        .and_then(|value| Uuid::parse_str(&value).ok());
    let user_exists = if let Some(user_id) = configured_user_id {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM auth_users WHERE id = $1)")
            .bind(user_id)
            .fetch_one(pool)
            .await?
    } else {
        false
    };

    let user_id = if user_exists {
        configured_user_id.expect("configured local user id must be present when it exists")
    } else {
        let existing_user = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM auth_users ORDER BY created_at ASC, id ASC LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        match existing_user {
            Some(user_id) => user_id,
            None => {
                sqlx::query_scalar::<_, Uuid>(
                    "INSERT INTO auth_users (id, email, name, email_verified) VALUES ($1, $2, $3, true) RETURNING id",
                )
                .bind(Uuid::new_v4())
                .bind("local@parrot-agent.local")
                .bind("Board")
                .fetch_one(pool)
                .await?
            }
        }
    };

    let company_ids =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM companies ORDER BY created_at ASC, id ASC")
            .fetch_all(pool)
            .await?;
    // Paperclip's local trusted actor is an instance-level Board principal;
    // it is deliberately not bound to one company. Resource routes resolve
    // the target company from their path or the resource itself.
    let company_id = Uuid::nil();

    sqlx::query(
        "INSERT INTO instance_user_roles (user_id, role) VALUES ($1, 'instance_admin') ON CONFLICT (user_id, role) DO NOTHING",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    for company_id in &company_ids {
        sqlx::query(
            "INSERT INTO company_memberships (company_id, principal_type, principal_id, status, membership_role) VALUES ($1, 'user'::principal_type, $2, 'active'::company_membership_status, 'owner'::membership_role) ON CONFLICT (company_id, principal_type, principal_id) DO NOTHING",
        )
        .bind(company_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    }

    std::env::set_var("LOCAL_TRUSTED_USER_ID", user_id.to_string());
    std::env::set_var("LOCAL_TRUSTED_COMPANY_ID", company_id.to_string());
    tracing::info!(%user_id, %company_id, "local trusted board principal is ready");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置panic hook，记录崩溃信息到日志和stderr
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        eprintln!("\n{}", "=".repeat(80));
        eprintln!("💥 APPLICATION PANIC 💥");
        eprintln!("{}", "=".repeat(80));
        eprintln!("Panic occurred: {:?}", panic_info);
        eprintln!("\nBacktrace:\n{}", backtrace);
        eprintln!("{}\n", "=".repeat(80));

        // 也尝试写入tracing日志（如果已初始化）
        tracing::error!(
            panic_info = ?panic_info,
            backtrace = %backtrace,
            "APPLICATION PANIC - Service crashed"
        );
    }));

    // 加载 .env 文件（优先级：环境变量 > .env）
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5433/parrot_agent_dev".to_string()
    });

    tracing::info!("connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    tracing::info!("running migrations...");
    sqlx::migrate!("../../migrations").run(&pool).await?;

    let deployment_mode = std::env::var("DEPLOYMENT_MODE").ok();
    if deployment_mode.as_deref() == Some("local_trusted")
        || (deployment_mode.is_none() && cfg!(debug_assertions))
    {
        ensure_local_trusted_principal(&pool).await?;
    }

    // 初始化并启动 Job Scheduler
    tracing::info!("initializing job scheduler...");
    let job_scheduler = Arc::new(services::JobScheduler::new().with_pool(pool.clone()));

    // RoutineExecutionService dispatches scheduled triggers through RoutineService
    // (single source of truth for run creation: concurrency policy, idempotency,
    // dispatch fingerprint) — see services::routine_execution_service.
    let routine_repo: Arc<dyn repositories::RoutineRepository> =
        Arc::new(repositories::routine_repository::PostgresRoutineRepository::new(pool.clone()));
    let scheduler_budget_service: Arc<dyn services::BudgetService> =
        Arc::new(services::DefaultBudgetService::new(
            Arc::new(PgCostEventRepository::new(pool.clone())),
            Arc::new(PgBudgetPolicyRepository::new(pool.clone())),
            Arc::new(PgBudgetIncidentRepository::new(pool.clone())),
            Arc::new(CompanyRepository::new(pool.clone())),
        ));
    let scheduler_heartbeat = Arc::new(
        services::DefaultHeartbeatService::new(pool.clone())
            .with_budget_service(scheduler_budget_service),
    );
    let routine_execution_service = Arc::new(services::RoutineExecutionService::new(Arc::new(
        services::RoutineServiceImpl::new(routine_repo),
    )));
    // Share one scheduler heartbeat runtime with recovery and decision-retention
    // notification jobs so archive notifications use the real wake path.

    // 注册后台任务
    job_scheduler
        .register(Arc::new(services::RoutineCronTrigger::new(
            pool.clone(),
            routine_execution_service,
        )))
        .await;

    job_scheduler
        .register(Arc::new(services::MonitorCheckJob::new(pool.clone())))
        .await;
    job_scheduler
        .register(Arc::new(services::RecoveryActionRetryJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::DecisionTrainingCommentScrubJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::SecretMaterialBackfillJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::SecretProposalExpirationJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::LeaseExpiryScanner::new(pool.clone())))
        .await;
    job_scheduler
        .register(Arc::new(services::EnvironmentHealthProber::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::StuckRunDetector::new(pool.clone())))
        .await;
    job_scheduler
        .register(Arc::new(services::ConsistencyCheckJob::new(pool.clone())))
        .await;
    // 后台任务链：status-card scheduler tick + summary-slot 终态 finalizer
    // （对应 paperclip heartbeatSchedulerInterval 中的 status-card tick）
    job_scheduler
        .register(Arc::new(services::StatusCardSchedulerJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::SummarySlotFinalizerJob::new(
            pool.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(services::HeartbeatRecoveryJob::new(
            scheduler_heartbeat.clone(),
        )))
        .await;
    job_scheduler
        .register(Arc::new(
            services::SchedulerExecutionHistoryCleanupJob::new(job_scheduler.clone()),
        ))
        .await;
    job_scheduler
        .register(Arc::new(services::SchedulerLeaseRepairJob::new(
            job_scheduler.clone(),
        )))
        .await;
    let decision_wakeup = Arc::new(
        services::decision_wakeup_service::DefaultDecisionWakeupService::new(true)
            .with_heartbeat_service(scheduler_heartbeat),
    );
    job_scheduler
        .register(Arc::new(
            services::decision_retention_sweep_job::DecisionRetentionSweepJob::new(pool.clone())
                .with_wakeup(decision_wakeup),
        ))
        .await;

    // 启动调度器（30 秒间隔，对应 paperclip 的 heartbeatSchedulerIntervalMs）
    let _scheduler_handle = job_scheduler.clone().start(30000).await;
    tracing::info!("job scheduler started with 30s interval");

    let plugin_worker_manager = Arc::new(services::PluginWorkerManager::new());
    let plugin_job_scheduler = Arc::new(services::PluginJobScheduler::new(
        services::PluginJobSchedulerOptions {
            db: pool.clone(),
            worker_manager: plugin_worker_manager,
            tick_interval_ms: None,
            job_timeout_ms: None,
            max_concurrent_jobs: None,
        },
    ));
    plugin_job_scheduler.start().await;
    tracing::info!("plugin job scheduler started");

    let mut state = parrot_server::build_app_state(pool.clone()).await?;
    state.scheduler = Some(job_scheduler.clone());

    // Task watchdogs evaluate stopped subtrees and reopen their review issues.
    // Registered after `build_app_state` because the watchdog service is
    // constructed there; Paperclip runs the equivalent step inside its periodic
    // heartbeat reconciliation loop.
    job_scheduler
        .register(Arc::new(services::TaskWatchdogJob::new(
            pool.clone(),
            state.watchdog_service.clone(),
        )))
        .await;

    let app: Router = create_router(state);

    let config = services::config::Config::load(None).unwrap_or_default();
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.server.port);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on http://{}", addr);

    // Enforcement from config: bind_mode=Lan rejects LAN mode when
    // deployment_mode is LocalTrusted — the local trusted principal is only
    // safe on loopback; otherwise require explicit authentication mode.
    if matches!(config.server.bind_mode, services::config::BindMode::Lan)
        && matches!(
            config.server.deployment_mode,
            services::config::DeploymentMode::LocalTrusted
        )
    {
        return Err("bind_mode=Lan requires DeploymentMode=Authenticated".into());
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    systemd_notify::notify_ready();
    // §8.1: Graceful shutdown — drain in-flight requests on SIGTERM / Ctrl+C.
    let _signal_task = tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut terminate =
                signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
            let mut interrupt =
                signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");
            tokio::select! {
                _ = terminate.recv() => {},
                _ = interrupt.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for Ctrl+C");
        }
        systemd_notify::notify_stopping("draining connections");
    });

    axum::serve(listener, app).await?;
    drop(_signal_task);

    tracing::info!("shutdown complete");
    Ok(())
}
