//! Parrot Agent server entry point.
//!
//! Builds the dependency graph (repositories -> services -> AppState),
//! runs migrations, and serves the Axum router produced by `api::create_router`.

use std::sync::Arc;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use access::DefaultAccessService;
use api::app_state::AppState;
use api::create_router;
use models::event_bus::EventBus;
use repositories::{
    agent_api_key_repository::PgAgentApiKeyRepository,
    approval_repository::PostgresApprovalRepository,
    case_issue_link_repository::CaseIssueLinkRepository,
    case_repository::CaseRepository,
    company_repository::CompanyRepository,
    environment_repository::EnvironmentRepository,
    execution_workspace_repository::ExecutionWorkspaceRepository,
    goal_repository::GoalRepository,
    pg_agent_repository::PgAgentRepository,
    pg_case_issue_link_repository::PgCaseIssueLinkRepository,
    pg_case_repository::{PgCaseEventRepository, PgCaseRepository},
    pg_config_revision_repository::PgConfigRevisionRepository,
    pg_issue_comment_repository::PgIssueCommentRepository,
    pg_issue_repository::PgIssueRepository,
    pg_issue_tree_control_repository::PgIssueTreeHoldRepository,
    pipeline_case_repository::PipelineCaseRepository,
    pipeline_repository::PipelineRepository,
    pipeline_stage_repository::PipelineStageRepository,
    pipeline_transition_repository::PipelineTransitionRepository,
    project_repository::ProjectRepository,
    routine_repository::RoutineRepository,
    routine_revision_repository::RoutineRevisionRepository,
    routine_trigger_repository::RoutineTriggerRepository,
    secret_provider_config_repository::SecretProviderConfigRepository,
    secret_repository::UserSecretDefinitionRepository,
    task_watchdog_repository::{
        AgentWakeupRequestRepository, HeartbeatRunRepository, IssueThreadInteractionRepository,
        IssueWatchdogRepository,
    },
    user_secret_repository::UserSecretRepository,
    PgCompanySkillRepository, PgSecretProviderConfigRepository, PgSkillCatalogRepository,
    PgSkillCommentRepository, PgSkillFileRepository, PgSkillStarRepository,
    PgSkillTestInputRepository, PgSkillTestRunRepository, PgSkillTestRunTemplateRepository,
    PgSkillVersionRepository,
};
use services::event_listeners::{
    ApprovalApprovedToIssueUnblockListener, CompleteIssueServiceAdapter,
    RoutineTriggeredToIssueCreationListener,
};
use services::{
    issue_comment_service::IssueCommentServiceImpl,
    // Namespaced impls (avoid root-level name collisions)
    issue_tree_control_service::IssueTreeControlServiceImpl,
    openclaw_service::OpenClawServiceImpl,
    user_secret_definition_service::UserSecretDefinitionServiceImpl,
    // Traits (re-exported from crate root)
    AgentService,
    ApprovalService,
    AttachmentService,
    BuiltInAgentService,
    CaseService,
    CompanyService,
    ConfigRevisionService,
    ConfigRevisionServiceImpl,
    CustomImageSetupService,
    // Real service impls
    DefaultAgentService,
    DefaultApprovalService,
    DefaultBuiltInAgentService,
    DefaultEnvironmentRuntimeService,
    DefaultGoalService,
    DefaultHeartbeatService,
    DefaultInstanceSettingsService,
    DefaultIssueService,
    DefaultLowTrustService,
    DefaultOrgChartService,
    DefaultPipelineService,
    DefaultSkillRegistryServiceImpl,
    DefaultWatchdogService,
    EnvironmentDiagnosticsService,
    EnvironmentRuntimeService,
    EnvironmentService,
    GoalService,
    InMemoryEventBus,
    InMemorySseService,
    InstanceSettingsService,
    InviteResourceService,
    InviteService,
    InviteServiceImpl,
    IssueCommentService,
    IssueDiagnosticsService,
    IssueService,
    IssueTreeControlService,
    LowTrustService,
    OpenClawService,
    OrgChartService,
    PipelineService,
    ProjectService,
    RoutineAnnotationService,
    RoutineService,
    RoutineServiceImpl,
    SecretProviderConfigService,
    SecretRemoteImportService,
    SseService,
    UserDirectoryService,
    UserSecretDefinitionService,
    UserSecretService,
    UserSecretServiceImpl,
    WatchdogService,
    WorkProductService,
};

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

    // 创建 RoutineExecutionService
    let routine_execution_service = Arc::new(services::RoutineExecutionService::new(pool.clone()));
    // Share one scheduler heartbeat runtime with recovery and decision-retention
    // notification jobs so archive notifications use the real wake path.
    let scheduler_heartbeat = Arc::new(services::DefaultHeartbeatService::new(pool.clone()));

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
        .register(Arc::new(services::SchedulerExecutionHistoryCleanupJob::new(
            job_scheduler.clone(),
        )))
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

    let state = build_app_state(pool.clone()).await?;

    let app: Router = create_router(state);

    let config = services::config::Config::load(None).unwrap_or_default();
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.server.port);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Construct the full application state by wiring repositories -> services.
///
/// The server wires PostgreSQL/local-storage implementations in production;
/// mock implementations are retained for unit tests only.
///
/// **Recently upgraded from Mock:**
/// - `secret-provider-config` → `DefaultSecretProviderConfigServiceImpl`
async fn build_app_state(pool: PgPool) -> Result<AppState, Box<dyn std::error::Error>> {
    // --- Repositories ---
    let agent_repo = PgAgentRepository::new(pool.clone());
    let agent_api_key_repo = PgAgentApiKeyRepository::new(pool.clone());
    let config_revision_repo: Arc<PgConfigRevisionRepository> =
        Arc::new(PgConfigRevisionRepository::new(pool.clone()));
    let issue_repo: Arc<PgIssueRepository> = Arc::new(PgIssueRepository::new(pool.clone()));
    let issue_comment_repo: Arc<PgIssueCommentRepository> =
        Arc::new(PgIssueCommentRepository::new(pool.clone()));
    let tree_hold_repo: Arc<PgIssueTreeHoldRepository> =
        Arc::new(PgIssueTreeHoldRepository::new(pool.clone()));
    let approval_repo: Arc<PostgresApprovalRepository> =
        Arc::new(PostgresApprovalRepository::new(pool.clone()));
    let company_repo = CompanyRepository::new(pool.clone());
    let company_repo_for_services = CompanyRepository::new(pool.clone());
    let project_repo = ProjectRepository::new(pool.clone());
    let goal_repo: Arc<dyn GoalRepository> = Arc::new(
        repositories::goal_repository::PostgresGoalRepository::new(pool.clone()),
    );
    let environment_repo: Arc<dyn EnvironmentRepository> =
        Arc::new(repositories::environment_repository::PgEnvironmentRepository::new(pool.clone()));
    let case_repo: Arc<dyn CaseRepository> = Arc::new(PgCaseRepository::new(pool.clone()));
    let case_event_repo: Arc<dyn repositories::CaseEventRepository> =
        Arc::new(PgCaseEventRepository::new(pool.clone()));
    let case_issue_link_repo: Arc<dyn CaseIssueLinkRepository> =
        Arc::new(PgCaseIssueLinkRepository::new(pool.clone()));
    let cost_event_repo: Arc<repositories::cost_event_repository::PgCostEventRepository> =
        Arc::new(repositories::cost_event_repository::PgCostEventRepository::new(pool.clone()));
    let budget_policy_repo: Arc<repositories::budget_repository::PgBudgetPolicyRepository> =
        Arc::new(repositories::budget_repository::PgBudgetPolicyRepository::new(pool.clone()));
    let budget_incident_repo: Arc<repositories::budget_repository::PgBudgetIncidentRepository> =
        Arc::new(repositories::budget_repository::PgBudgetIncidentRepository::new(pool.clone()));
    let finance_event_repo: Arc<repositories::finance_event_repository::PgFinanceEventRepository> =
        Arc::new(
            repositories::finance_event_repository::PgFinanceEventRepository::new(pool.clone()),
        );
    let activity_log_repo: Arc<repositories::activity_log_repository::PgActivityLogRepository> =
        Arc::new(repositories::activity_log_repository::PgActivityLogRepository::new(pool.clone()));
    let pipeline_repo: Arc<dyn PipelineRepository> =
        Arc::new(repositories::pipeline_repository::PostgresPipelineRepository::new(pool.clone()));
    let pipeline_stage_repo: Arc<dyn PipelineStageRepository> = Arc::new(
        repositories::pipeline_stage_repository::PostgresPipelineStageRepository::new(pool.clone()),
    );
    let pipeline_transition_repo: Arc<dyn PipelineTransitionRepository> = Arc::new(
        repositories::pipeline_transition_repository::PostgresPipelineTransitionRepository::new(
            pool.clone(),
        ),
    );
    let pipeline_case_repo: Arc<dyn PipelineCaseRepository> = Arc::new(
        repositories::pipeline_case_repository::PostgresPipelineCaseRepository::new(pool.clone()),
    );
    let routine_repo: Arc<dyn RoutineRepository> =
        Arc::new(repositories::routine_repository::PostgresRoutineRepository::new(pool.clone()));
    let _routine_trigger_repo: Arc<dyn RoutineTriggerRepository> = Arc::new(
        repositories::routine_trigger_repository::PostgresRoutineTriggerRepository::new(
            pool.clone(),
        ),
    );
    let _routine_revision_repo: Arc<dyn RoutineRevisionRepository> = Arc::new(
        repositories::routine_revision_repository::PostgresRoutineRevisionRepository::new(
            pool.clone(),
        ),
    );
    let _secret_provider_config_repo: Arc<dyn SecretProviderConfigRepository> = Arc::new(
        repositories::secret_provider_config_repository::PostgresSecretProviderConfigRepository::new(pool.clone()),
    );
    let user_secret_repo: Arc<dyn UserSecretRepository> = Arc::new(
        repositories::user_secret_repository::PostgresUserSecretRepository::new(pool.clone()),
    );
    let user_secret_definition_repo: Arc<dyn UserSecretDefinitionRepository> = Arc::new(
        repositories::secret_repository::PgUserSecretDefinitionRepository::new(pool.clone()),
    );
    let _exec_workspace_repo: Arc<dyn ExecutionWorkspaceRepository> = Arc::new(
        repositories::execution_workspace_repository::PgExecutionWorkspaceRepository::new(
            pool.clone(),
        ),
    );
    let watchdog_repo: Arc<dyn IssueWatchdogRepository> = Arc::new(
        repositories::task_watchdog_repository::PostgresIssueWatchdogRepository::new(pool.clone()),
    );
    let heartbeat_repo: Arc<dyn HeartbeatRunRepository> = Arc::new(
        repositories::task_watchdog_repository::PostgresHeartbeatRunRepository::new(pool.clone()),
    );
    let wakeup_repo: Arc<dyn AgentWakeupRequestRepository> = Arc::new(
        repositories::task_watchdog_repository::PostgresAgentWakeupRequestRepository::new(
            pool.clone(),
        ),
    );
    let interaction_repo: Arc<dyn IssueThreadInteractionRepository> = Arc::new(
        repositories::task_watchdog_repository::PostgresIssueThreadInteractionRepository::new(
            pool.clone(),
        ),
    );

    // --- Services ---
    let agent_service: Arc<dyn AgentService> = Arc::new(
        DefaultAgentService::new(
            agent_repo.clone(),
            Arc::new(agent_api_key_repo.clone()),
            pool.clone(),
        )
        .with_heartbeat_pool(pool.clone())
        .with_config_revision_repo(config_revision_repo.clone())
        .with_cost_event_repo(cost_event_repo.clone())
        .with_activity_log_repo(activity_log_repo.clone()),
    );
    let access_service: Arc<dyn access::AccessService> =
        Arc::new(DefaultAccessService::with_pool(pool.clone()));
    let config_revision_service: Arc<dyn ConfigRevisionService> = Arc::new(
        ConfigRevisionServiceImpl::new(Arc::new(agent_repo.clone()), config_revision_repo.clone()),
    );
    let built_in_agent_service: Arc<dyn BuiltInAgentService> = Arc::new(
        DefaultBuiltInAgentService::new(Arc::new(agent_repo.clone())),
    );
    let adapter_registry: Arc<services::server_adapter::AdapterRegistry> =
        Arc::new(services::create_default_server_adapter_registry());
    let server_adapter_registry = Arc::new(services::create_default_server_adapter_registry());
    let environment_runtime_service: Arc<dyn EnvironmentRuntimeService> =
        Arc::new(DefaultEnvironmentRuntimeService::with_pool(pool.clone()));
    let issue_comment_service: Arc<dyn IssueCommentService> = Arc::new(
        IssueCommentServiceImpl::new(issue_comment_repo.clone(), issue_repo.clone()),
    );
    let issue_tree_control_service: Arc<dyn IssueTreeControlService> = Arc::new(
        IssueTreeControlServiceImpl::new(tree_hold_repo.clone(), issue_repo.clone()),
    );
    let org_chart_service: Arc<dyn OrgChartService> =
        Arc::new(DefaultOrgChartService::new(pool.clone()));
    let issue_diagnostics_service: Arc<dyn IssueDiagnosticsService> = Arc::new(
        services::issue_diagnostics_service::DefaultIssueDiagnosticsService::new(
            issue_repo.clone(),
        )
        .with_wakeup_repo(wakeup_repo.clone()),
    );
    let low_trust_service: Arc<dyn LowTrustService> =
        Arc::new(DefaultLowTrustService::new(issue_repo.clone()));
    let company_service: Arc<CompanyService> = Arc::new(CompanyService::new(company_repo));
    let project_service: Arc<ProjectService> = Arc::new(ProjectService::new(project_repo));
    let routine_service: Arc<dyn RoutineService> =
        Arc::new(RoutineServiceImpl::new(routine_repo.clone()));
    let goal_service: Arc<dyn GoalService> = Arc::new(DefaultGoalService::new(goal_repo));
    let environment_service: Arc<dyn EnvironmentService> =
        Arc::new(services::environment_service::DefaultEnvironmentService::new(environment_repo));
    let pipeline_service: Arc<dyn PipelineService> = Arc::new(DefaultPipelineService::new(
        pipeline_repo,
        pipeline_case_repo,
        pipeline_stage_repo,
        pipeline_transition_repo,
    ));
    let skill_registry_service: Arc<dyn services::skill_registry_service::SkillRegistryService> =
        Arc::new(DefaultSkillRegistryServiceImpl::new(
            std::env::var("LOCAL_TRUSTED_USER_ID")
                .ok()
                .and_then(|id| uuid::Uuid::parse_str(&id).ok()),
            Arc::new(PgSkillCatalogRepository::new(pool.clone())),
            Arc::new(PgCompanySkillRepository::new(pool.clone())),
            Arc::new(PgSkillVersionRepository::new(pool.clone())),
            Arc::new(PgSkillTestInputRepository::new(pool.clone())),
            Arc::new(PgSkillTestRunTemplateRepository::new(pool.clone())),
            Arc::new(PgSkillTestRunRepository::new(pool.clone())),
            Arc::new(PgSkillStarRepository::new(pool.clone())),
            Arc::new(PgSkillCommentRepository::new(pool.clone())),
            Arc::new(PgSkillFileRepository::new(pool.clone())),
        ));
    // P1.3: 公司级 Skill 策略（平台安全层 + 公司策略层）
    let skill_policy_service: Arc<dyn services::SkillPolicyService> =
        Arc::new(services::DefaultSkillPolicyService::new(Arc::new(
            repositories::PgCompanySkillPolicyRepository::new(pool.clone()),
        )));
    // P1.4: Teams Catalog（文件系统 catalog + 事务性安装）
    let teams_catalog_service: Arc<dyn services::TeamsCatalogService> =
        Arc::new(services::DefaultTeamsCatalogService::new(pool.clone()));
    let sse_service: Arc<dyn SseService> = InMemorySseService::new();
    let invite_service: Arc<dyn InviteService> =
        Arc::new(InviteServiceImpl::with_pool(pool.clone()));
    let openclaw_service: Arc<dyn OpenClawService> =
        Arc::new(OpenClawServiceImpl::with_pool(pool.clone()));
    let user_directory_service: Arc<dyn UserDirectoryService> = Arc::new(
        services::user_directory_service::UserDirectoryServiceImpl::with_pool(pool.clone()),
    );
    let custom_image_setup_service: Arc<dyn CustomImageSetupService> = Arc::new(
        services::custom_image_setup_service::PgCustomImageSetupService::new(pool.clone()),
    );
    let secret_provider_config_repo: Arc<PgSecretProviderConfigRepository> =
        Arc::new(PgSecretProviderConfigRepository::new(pool.clone()));
    let secret_provider_config_service: Arc<dyn SecretProviderConfigService> = Arc::new(
        services::DefaultSecretProviderConfigServiceImpl::new(secret_provider_config_repo),
    );
    let secret_remote_import_service: Arc<dyn SecretRemoteImportService> = Arc::new(
        services::secret_remote_import_service::ProviderSecretRemoteImportService::new(
            pool.clone(),
        ),
    );
    let environment_diagnostics_service: Arc<dyn EnvironmentDiagnosticsService> = Arc::new(
        services::environment_diagnostics_service::PgEnvironmentDiagnosticsService::new(
            pool.clone(),
        ),
    );
    let invite_resource_service: Arc<dyn InviteResourceService> =
        Arc::new(services::invite_resource_service::PgInviteResourceService::new(pool.clone()));
    let routine_annotation_service: Arc<dyn RoutineAnnotationService> = Arc::new(
        services::routine_annotation_service::PgRoutineAnnotationService::new(pool.clone()),
    );
    let work_product_service: Arc<dyn WorkProductService> = Arc::new(
        services::work_product_service::PgWorkProductService::new(pool.clone()),
    );
    let attachment_service: Arc<dyn AttachmentService> = Arc::new(
        services::attachment_service::LocalAttachmentService::new(pool.clone()),
    );
    let user_secret_definition_service: Arc<dyn UserSecretDefinitionService> =
        Arc::new(UserSecretDefinitionServiceImpl::with_pool(pool.clone()));
    let user_secret_service: Arc<dyn UserSecretService> = Arc::new(UserSecretServiceImpl::new(
        user_secret_repo,
        user_secret_definition_repo,
    ));
    let case_service: Arc<dyn CaseService> = Arc::new(services::case_service::PgCaseService::new(
        pool.clone(),
        case_repo,
        case_event_repo,
        case_issue_link_repo,
    ));
    let event_bus: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new(1024));
    let event_issue_service: Arc<dyn services::issue_service_complete::IssueService> = Arc::new(
        DefaultIssueService::new(
            issue_repo.clone(),
            approval_repo.clone(),
            issue_tree_control_service.clone(),
            issue_comment_service.clone(),
            work_product_service.clone(),
            attachment_service.clone(),
        )
        .with_routine_repo(routine_repo.clone()),
    );
    let event_issue_adapter = Arc::new(CompleteIssueServiceAdapter::new(event_issue_service));
    event_bus
        .subscribe(Box::new(ApprovalApprovedToIssueUnblockListener::new(
            event_issue_adapter.clone(),
        )))
        .await
        .map_err(|error| format!("failed to subscribe approval listener: {error}"))?;
    event_bus
        .subscribe(Box::new(RoutineTriggeredToIssueCreationListener::new(
            event_issue_adapter,
        )))
        .await
        .map_err(|error| format!("failed to subscribe routine listener: {error}"))?;
    // 创建 Approval Executor
    let approval_executor: Arc<dyn services::approval_execution::ApprovalExecutor> =
        Arc::new(services::approval_execution::DefaultApprovalExecutor::new(
            pool.clone(),
            agent_service.clone(),
            Arc::new(agent_repo.clone()),
            budget_policy_repo.clone(),
        ));

    let approval_service: Arc<dyn ApprovalService> = Arc::new(
        DefaultApprovalService::new(approval_repo.clone(), issue_repo.clone())
            .with_event_bus(event_bus.clone())
            .with_approval_executor(approval_executor)
            .with_adapter_registry(adapter_registry.clone())
            .with_agent_repository(Arc::new(PgAgentRepository::new(pool.clone())))
            .with_activity_repository(Arc::new(
                repositories::activity_log_repository::PgActivityLogRepository::new(pool.clone()),
            )),
    );
    let watchdog_service: Arc<dyn WatchdogService> = Arc::new(DefaultWatchdogService::new(
        issue_repo.clone(),
        watchdog_repo,
        heartbeat_repo,
        wakeup_repo,
        interaction_repo,
    ));

    // Create cost service before heartbeat service (heartbeat needs it for cost event tracking)
    let cost_service: Arc<dyn services::CostService> = Arc::new(
        services::DefaultCostService::new(
            cost_event_repo.clone() as Arc<dyn repositories::CostEventRepository>,
            Arc::new(agent_repo.clone()) as Arc<dyn repositories::AgentRepository>,
            Arc::new(company_repo_for_services.clone()),
        )
        .with_adapter_registry(adapter_registry.clone()),
    );

    let heartbeat_coordinator = Arc::new(
        DefaultHeartbeatService::new(pool.clone())
            .with_sse_service(sse_service.clone())
            .with_cost_service(cost_service.clone()),
    );
    let heartbeat_service: Arc<dyn services::HeartbeatService> = heartbeat_coordinator.clone();
    let recovery_action_repository = Arc::new(
        repositories::PgRecoveryActionRepository::new(pool.clone()),
    );
    let recovery_action_service: Arc<dyn services::RecoveryActionService> = Arc::new(
        services::DefaultRecoveryActionService::new(
            recovery_action_repository,
            issue_repo.clone(),
        ),
    );
    // Label service
    let label_repo: Arc<repositories::label_repository::PgLabelRepository> = Arc::new(
        repositories::label_repository::PgLabelRepository::new(pool.clone()),
    );
    let label_service: Arc<dyn services::LabelService> =
        Arc::new(services::DefaultLabelService::new(label_repo));

    // Instance settings service (in-memory implementation)
    let instance_settings_service: Arc<dyn InstanceSettingsService> =
        Arc::new(DefaultInstanceSettingsService::with_pool_and_watchdog(
            pool.clone(),
            watchdog_service.clone(),
        ));

    // Adapt the complete service to the route-facing legacy trait while preserving the
    // shared repository instances used by approvals and issue sub-resources.
    let issue_service: Arc<dyn IssueService> = Arc::new(services::LegacyIssueService::new(
        issue_repo.clone(),
        approval_repo.clone(),
        issue_tree_control_service.clone(),
        issue_comment_service.clone(),
        work_product_service.clone(),
        attachment_service.clone(),
        heartbeat_service.clone(),
        recovery_action_service,
    ));

    match heartbeat_coordinator.reconcile_pending_issues().await {
        Ok(count) if count > 0 => tracing::info!(count, "reconciled pending assigned issues"),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "failed to reconcile pending assigned issues"),
    }

    let company_portability_service = Arc::new(services::DefaultCompanyPortabilityService::new(
        pool.clone(),
    ));

    // 初始化适配器注册状态管理
    let adapter_registry_state = Arc::new(
        services::AdapterRegistryState::new()
            .map_err(|e| format!("Failed to initialize adapter registry state: {}", e))?,
    );

    Ok(AppState::new(
        agent_service,
        access_service,
        config_revision_service,
        built_in_agent_service,
        adapter_registry,
        adapter_registry_state,
        environment_runtime_service,
        issue_service,
        case_service,
        issue_comment_service,
        issue_tree_control_service,
        org_chart_service,
        issue_diagnostics_service,
        low_trust_service,
        company_service,
        project_service,
        routine_service,
        goal_service,
        environment_service,
        pipeline_service,
        skill_registry_service,
        skill_policy_service,
        teams_catalog_service,
        sse_service,
        invite_service,
        openclaw_service,
        user_directory_service,
        custom_image_setup_service,
        secret_provider_config_service,
        secret_remote_import_service,
        environment_diagnostics_service,
        invite_resource_service,
        routine_annotation_service,
        work_product_service,
        attachment_service,
        user_secret_definition_service,
        user_secret_service,
        approval_service,
        watchdog_service,
        heartbeat_service,
        Arc::new(services::DefaultTermService::new()),
        label_service,
        instance_settings_service,
        cost_service.clone(),
        Arc::new(services::DefaultBudgetService::new(
            cost_event_repo.clone() as Arc<dyn repositories::CostEventRepository>,
            budget_policy_repo.clone() as Arc<dyn repositories::BudgetPolicyRepository>,
            budget_incident_repo.clone() as Arc<dyn repositories::BudgetIncidentRepository>,
            Arc::new(company_repo_for_services.clone()),
        )),
        Arc::new(services::DefaultFinanceService::new(
            finance_event_repo.clone() as Arc<dyn repositories::FinanceEventRepository>,
            Arc::new(company_repo_for_services.clone()),
        )),
        Arc::new(services::DefaultPluginService::new(pool.clone())),
        company_portability_service.clone(),
        company_portability_service.clone(),
        company_portability_service,
        Arc::new(services::cloud_upstream_service::DefaultCloudUpstreamService::new(pool.clone())),
        Arc::new(
            services::work_timeline_service::DefaultWorkTimelineService { pool: pool.clone() },
        ),
        event_bus,
        pool,
    ))
}
