//! Parrot Agent server entry point.
//!
//! Builds the dependency graph (repositories -> services -> AppState),
//! runs migrations, and serves the Axum router produced by `api::create_router`.

use std::sync::Arc;

use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing_subscriber::EnvFilter;

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
    pg_case_repository::PgCaseRepository,
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
use services::{
    issue_comment_service::IssueCommentServiceImpl,
    // Namespaced impls (avoid root-level name collisions)
    issue_tree_control_service::IssueTreeControlServiceImpl,
    openclaw_service::OpenClawServiceImpl,
    user_secret_definition_service::UserSecretDefinitionServiceImpl,
    // Traits (re-exported from crate root)
    AdapterRegistry,
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
    DefaultInstanceSettingsService,
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
    // Mock impls for not-yet-implemented domains
    MockCaseService,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let state = build_app_state(pool.clone());

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
/// Services that only have a `Mock` implementation in the codebase (case, work-product,
/// attachment, custom-image-setup, environment-diagnostics, invite-resource, routine-annotation,
/// secret-remote-import, skill-registry) are wired with their mocks so the server compiles and
/// runs. They return mock data until real `Default*` implementations land.
///
/// **Recently upgraded from Mock:**
/// - `secret-provider-config` → `DefaultSecretProviderConfigServiceImpl`
fn build_app_state(pool: PgPool) -> AppState {
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
    let _case_repo: Arc<dyn CaseRepository> = Arc::new(PgCaseRepository::new(pool.clone()));
    let _case_issue_link_repo: Arc<dyn CaseIssueLinkRepository> =
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
        DefaultAgentService::new(agent_repo.clone(), Arc::new(agent_api_key_repo.clone()))
            .with_config_revision_repo(config_revision_repo.clone())
            .with_cost_event_repo(cost_event_repo.clone())
            .with_activity_log_repo(activity_log_repo.clone()),
    );
    let access_service: Arc<dyn access::AccessService> = Arc::new(DefaultAccessService::new());
    let config_revision_service: Arc<dyn ConfigRevisionService> = Arc::new(
        ConfigRevisionServiceImpl::new(Arc::new(agent_repo.clone()), config_revision_repo.clone()),
    );
    let built_in_agent_service: Arc<dyn BuiltInAgentService> = Arc::new(
        DefaultBuiltInAgentService::new(Arc::new(agent_repo.clone())),
    );
    let adapter_registry: Arc<AdapterRegistry> = Arc::new(AdapterRegistry::new());
    let environment_runtime_service: Arc<dyn EnvironmentRuntimeService> =
        Arc::new(DefaultEnvironmentRuntimeService::new());
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
        ),
    );
    let low_trust_service: Arc<dyn LowTrustService> =
        Arc::new(DefaultLowTrustService::new(issue_repo.clone()));
    let company_service: Arc<CompanyService> = Arc::new(CompanyService::new(company_repo));
    let project_service: Arc<ProjectService> = Arc::new(ProjectService::new(project_repo));
    let routine_service: Arc<dyn RoutineService> = Arc::new(RoutineServiceImpl::new(routine_repo));
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
    let sse_service: Arc<dyn SseService> = InMemorySseService::new();
    let invite_service: Arc<dyn InviteService> = Arc::new(InviteServiceImpl::new());
    let openclaw_service: Arc<dyn OpenClawService> = Arc::new(OpenClawServiceImpl::new());
    let user_directory_service: Arc<dyn UserDirectoryService> =
        Arc::new(services::user_directory_service::UserDirectoryServiceImpl::new());
    let custom_image_setup_service: Arc<dyn CustomImageSetupService> =
        Arc::new(services::custom_image_setup_service::MockCustomImageSetupService);
    let secret_provider_config_repo: Arc<PgSecretProviderConfigRepository> =
        Arc::new(PgSecretProviderConfigRepository::new(pool.clone()));
    let secret_provider_config_service: Arc<dyn SecretProviderConfigService> = Arc::new(
        services::DefaultSecretProviderConfigServiceImpl::new(secret_provider_config_repo),
    );
    let secret_remote_import_service: Arc<dyn SecretRemoteImportService> =
        Arc::new(services::secret_remote_import_service::MockSecretRemoteImportService);
    let environment_diagnostics_service: Arc<dyn EnvironmentDiagnosticsService> =
        Arc::new(services::environment_diagnostics_service::MockEnvironmentDiagnosticsService);
    let invite_resource_service: Arc<dyn InviteResourceService> =
        Arc::new(services::invite_resource_service::MockInviteResourceService);
    let routine_annotation_service: Arc<dyn RoutineAnnotationService> =
        Arc::new(services::routine_annotation_service::MockRoutineAnnotationService);
    let work_product_service: Arc<dyn WorkProductService> =
        Arc::new(services::work_product_service::MockWorkProductService);
    let attachment_service: Arc<dyn AttachmentService> =
        Arc::new(services::attachment_service::MockAttachmentService);
    let user_secret_definition_service: Arc<dyn UserSecretDefinitionService> =
        Arc::new(UserSecretDefinitionServiceImpl::new());
    let user_secret_service: Arc<dyn UserSecretService> = Arc::new(UserSecretServiceImpl::new(
        user_secret_repo,
        user_secret_definition_repo,
    ));
    let case_service: Arc<dyn CaseService> = Arc::new(MockCaseService);
    let approval_service: Arc<dyn ApprovalService> = Arc::new(DefaultApprovalService::new(
        approval_repo.clone(),
        issue_repo.clone(),
    ));
    let watchdog_service: Arc<dyn WatchdogService> = Arc::new(DefaultWatchdogService::new(
        issue_repo.clone(),
        watchdog_repo,
        heartbeat_repo,
        wakeup_repo,
        interaction_repo,
    ));
    let event_bus: Arc<dyn EventBus> = Arc::new(InMemoryEventBus::new(1024));

    // Label service
    let label_repo: Arc<repositories::label_repository::PgLabelRepository> = Arc::new(
        repositories::label_repository::PgLabelRepository::new(pool.clone()),
    );
    let label_service: Arc<dyn services::LabelService> =
        Arc::new(services::DefaultLabelService::new(label_repo));

    // Instance settings service (in-memory implementation)
    let instance_settings_service: Arc<dyn InstanceSettingsService> =
        Arc::new(DefaultInstanceSettingsService::new());

    // Adapt the complete service to the route-facing legacy trait while preserving the
    // shared repository instances used by approvals and issue sub-resources.
    let issue_service: Arc<dyn IssueService> = Arc::new(services::LegacyIssueService::new(
        issue_repo.clone(),
        approval_repo.clone(),
        issue_tree_control_service.clone(),
        issue_comment_service.clone(),
        work_product_service.clone(),
        attachment_service.clone(),
    ));

    let company_portability_service = Arc::new(services::DefaultCompanyPortabilityService::new(
        pool.clone(),
    ));
    AppState::new(
        agent_service,
        access_service,
        config_revision_service,
        built_in_agent_service,
        adapter_registry,
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
        Arc::new(services::DefaultTermService::new()),
        label_service,
        instance_settings_service,
        Arc::new(
            services::DefaultCostService::new(
                cost_event_repo.clone() as Arc<dyn repositories::CostEventRepository>,
                Arc::new(agent_repo.clone()) as Arc<dyn repositories::AgentRepository>,
                Arc::new(company_repo_for_services.clone()),
            )
            .with_adapter_registry(adapter_registry.clone()),
        ),
        Arc::new(services::DefaultBudgetService::new(
            cost_event_repo.clone() as Arc<dyn repositories::CostEventRepository>,
            budget_policy_repo.clone() as Arc<dyn repositories::BudgetPolicyRepository>,
            budget_incident_repo.clone() as Arc<dyn repositories::BudgetIncidentRepository>,
            Arc::new(agent_repo.clone()) as Arc<dyn repositories::AgentRepository>,
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
        event_bus,
        pool,
    )
}
