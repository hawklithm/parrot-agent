use std::sync::Arc;
use axum::Router;
use sqlx::PgPool;

// Re-export services
pub use services::{
    AgentService, ConfigRevisionService, IssueService, CaseService,
    IssueCommentService, IssueTreeControlService,
    BuiltInAgentService, AdapterRegistry, EnvironmentRuntimeService,
    OrgChartService, LowTrustService, CompanyService, ProjectService,
    RoutineService, GoalService, EnvironmentService, PipelineService,
    SkillRegistryService, SseService, InviteService, OpenClawService,
    UserDirectoryService, CustomImageSetupService, SecretProviderConfigService,
    SecretRemoteImportService, EnvironmentDiagnosticsService,
    InviteResourceService, RoutineAnnotationService, WorkProductService,
    AttachmentService, UserSecretDefinitionService, UserSecretService,
    WatchdogService, ApprovalService, TermService,
};

pub use access::AccessService;

pub use models::event_bus::EventBus;

/// Helper: wrap a service-backed router into an AppState-compatible router
#[allow(dead_code)]
fn wrap_routes<S>(routes: Router<S>, state: S) -> Router
where
    S: Clone + Send + Sync + 'static,
{
    routes.with_state(state)
}

/// Global application state containing all services
#[derive(Clone)]
pub struct AppState {
    // Phase 1: Agent Management
    pub agent_service: Arc<dyn AgentService>,
    pub access_service: Arc<dyn AccessService>,
    pub config_revision_service: Arc<dyn ConfigRevisionService>,
    pub built_in_agent_service: Arc<dyn BuiltInAgentService>,

    // Adapter subsystem
    pub adapter_registry: Arc<AdapterRegistry>,
    pub environment_runtime_service: Arc<dyn EnvironmentRuntimeService>,

    // Phase 2: Issue/Case Management
    pub issue_service: Arc<dyn IssueService>,
    pub case_service: Arc<dyn CaseService>,
    pub issue_comment_service: Arc<dyn IssueCommentService>,
    pub issue_tree_control_service: Arc<dyn IssueTreeControlService>,

    // Org chart
    pub org_chart_service: Arc<dyn OrgChartService>,

    // Issue diagnostics
    pub issue_diagnostics_service: Arc<dyn services::issue_diagnostics_service::IssueDiagnosticsService>,

    // Low trust review
    pub low_trust_service: Arc<dyn LowTrustService>,

    // Phase 3: Company/Org
    pub company_service: Arc<CompanyService>,
    pub project_service: Arc<ProjectService>,

    // Phase 4: Routine/Goal
    pub routine_service: Arc<dyn RoutineService>,
    pub goal_service: Arc<dyn GoalService>,

    // Environment
    pub environment_service: Arc<dyn EnvironmentService>,

    // Pipeline
    pub pipeline_service: Arc<dyn PipelineService>,

    // Skills
    pub skill_registry_service: Arc<dyn SkillRegistryService>,

    // Additional services for unmerged routes
    pub sse_service: Arc<dyn SseService>,
    pub invite_service: Arc<dyn InviteService>,
    pub openclaw_service: Arc<dyn OpenClawService>,
    pub user_directory_service: Arc<dyn UserDirectoryService>,
    pub custom_image_setup_service: Arc<dyn CustomImageSetupService>,
    pub secret_provider_config_service: Arc<dyn SecretProviderConfigService>,
    pub secret_remote_import_service: Arc<dyn SecretRemoteImportService>,
    pub environment_diagnostics_service: Arc<dyn EnvironmentDiagnosticsService>,
    pub invite_resource_service: Arc<dyn InviteResourceService>,
    pub routine_annotation_service: Arc<dyn RoutineAnnotationService>,
    pub work_product_service: Arc<dyn WorkProductService>,
    pub attachment_service: Arc<dyn AttachmentService>,
    pub user_secret_definition_service: Arc<dyn UserSecretDefinitionService>,
    pub user_secret_service: Arc<dyn UserSecretService>,

    // P2: Approval subsystem
    pub approval_service: Arc<dyn ApprovalService>,

    // Task watchdog subsystem
    pub watchdog_service: Arc<dyn WatchdogService>,

    // Terms service
    pub term_service: Arc<dyn TermService>,

    // Event bus
    pub event_bus: Arc<dyn EventBus>,

    // Shared DB pool
    pub pool: PgPool,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_service: Arc<dyn AgentService>,
        access_service: Arc<dyn AccessService>,
        config_revision_service: Arc<dyn ConfigRevisionService>,
        built_in_agent_service: Arc<dyn BuiltInAgentService>,
        adapter_registry: Arc<AdapterRegistry>,
        environment_runtime_service: Arc<dyn EnvironmentRuntimeService>,
        issue_service: Arc<dyn IssueService>,
        case_service: Arc<dyn CaseService>,
        issue_comment_service: Arc<dyn IssueCommentService>,
        issue_tree_control_service: Arc<dyn IssueTreeControlService>,
        org_chart_service: Arc<dyn OrgChartService>,
        issue_diagnostics_service: Arc<dyn services::issue_diagnostics_service::IssueDiagnosticsService>,
        low_trust_service: Arc<dyn LowTrustService>,
        company_service: Arc<CompanyService>,
        project_service: Arc<ProjectService>,
        routine_service: Arc<dyn RoutineService>,
        goal_service: Arc<dyn GoalService>,
        environment_service: Arc<dyn EnvironmentService>,
        pipeline_service: Arc<dyn PipelineService>,
        skill_registry_service: Arc<dyn SkillRegistryService>,
        sse_service: Arc<dyn SseService>,
        invite_service: Arc<dyn InviteService>,
        openclaw_service: Arc<dyn OpenClawService>,
        user_directory_service: Arc<dyn UserDirectoryService>,
        custom_image_setup_service: Arc<dyn CustomImageSetupService>,
        secret_provider_config_service: Arc<dyn SecretProviderConfigService>,
        secret_remote_import_service: Arc<dyn SecretRemoteImportService>,
        environment_diagnostics_service: Arc<dyn EnvironmentDiagnosticsService>,
        invite_resource_service: Arc<dyn InviteResourceService>,
        routine_annotation_service: Arc<dyn RoutineAnnotationService>,
        work_product_service: Arc<dyn WorkProductService>,
        attachment_service: Arc<dyn AttachmentService>,
        user_secret_definition_service: Arc<dyn UserSecretDefinitionService>,
        user_secret_service: Arc<dyn UserSecretService>,
        approval_service: Arc<dyn ApprovalService>,
        watchdog_service: Arc<dyn WatchdogService>,
        term_service: Arc<dyn TermService>,
        event_bus: Arc<dyn EventBus>,
        pool: PgPool,
    ) -> Self {
        Self {
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
            term_service,
            event_bus,
            pool,
        }
    }
}

/// Create the main application router with all routes.
///
/// 统一使用 `crate::app_state::AppState` 作为状态类型。各子路由工厂
/// 返回 `Router<AppState>`，或返回已绑定状态的无状态 `Router`，方可被
/// `merge` 合并。
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        // Phase 1: Agent Management routes
        .merge(crate::routes::agents::agent_routes())
        .merge(crate::routes::auth::auth_routes(state.clone()))
        .merge(crate::routes::access_control::access_control_routes(state.clone()))
        .merge(crate::routes::adapters::adapter_routes())
        .merge(crate::routes::config_revisions::config_revision_routes())
        .merge(crate::routes::built_in_agents::built_in_agent_routes())
        // Org chart routes (includes /companies/:companyId/org, /org-chart.svg, /org.png)
        .merge(crate::routes::org_chart::org_chart_routes())

        // Phase 2: Issue/Case Management routes
        .merge(crate::routes::issues::issue_routes())
        .merge(crate::routes::cases::case_routes())
        .merge(crate::routes::issue_comments::issue_comment_routes())
        .merge(crate::routes::issue_tree_control::issue_tree_control_routes())
        .merge(crate::routes::issue_diagnostics::issue_diagnostics_routes())
        .merge(crate::routes::low_trust::low_trust_routes())

        // Phase 3: Company/Org routes
        .merge(crate::routes::companies::company_routes())
        .merge(crate::routes::projects::project_routes())
        // Company secrets + secret providers (SE5, SE14-SE20)
        .merge(crate::routes::secrets::secret_routes())
        // Pipeline routes
        .merge(crate::routes::pipelines::pipeline_routes())
        // Routine/Goal routes
        .merge(crate::routes::routines::routine_routes())
        .merge(crate::routes::goals::goal_routes())

        // Phase 4: Additional service routes (now all AppState compatible)
        .merge(crate::routes::attachments::attachment_routes())
        .merge(crate::routes::work_products::work_product_routes())
        .merge(crate::routes::custom_image_setup::custom_image_setup_routes())
        .merge(crate::routes::environment_diagnostics::environment_diagnostics_routes())
        .merge(crate::routes::invite_resources::invite_resource_routes())
        .merge(crate::routes::openclaw::openclaw_routes())
        .merge(crate::routes::routine_annotations::routine_annotation_routes())
        .merge(crate::routes::secret_provider_configs::secret_provider_config_routes())
        .merge(crate::routes::secret_remote_import::secret_remote_import_routes())
        .merge(crate::routes::skills::skill_routes())
        .merge(crate::routes::sse::sse_routes())
        .merge(crate::routes::user_directory::user_directory_routes())
        .merge(crate::routes::user_secret_definitions::user_secret_definition_routes())
        // Routes with Arc<dyn X> state type (need wrapping)
        .merge(crate::routes::user_secrets::user_secret_routes().with_state(state.user_secret_service.clone()))
        .merge(crate::routes::invites::invite_subresource_routes().with_state(state.invite_service.clone()))

        // Task watchdog routes (Arc<dyn WatchdogService> state)
        .merge(crate::routes::watchdogs::watchdog_routes().with_state(state.watchdog_service.clone()))

        // P2: New domain routes
        .merge(crate::routes::approvals::approval_routes())
        .merge(crate::routes::costs::cost_routes())
        .merge(crate::routes::plugins::plugin_routes())
        .merge(crate::routes::activity::activity_routes())
        .merge(crate::routes::assets::asset_routes())
        .merge(crate::routes::board_chat::board_chat_routes())
        .merge(crate::routes::cloud_upstreams::cloud_upstream_routes())
        .merge(crate::routes::instance_settings::instance_settings_routes())
        .merge(crate::routes::labels::label_routes())
        .merge(crate::routes::llms::llm_routes())
        // P2: Execution workspace + heartbeat-run routes (X1-X18)
        .merge(crate::routes::execution_workspaces::execution_workspace_routes())
        .merge(crate::routes::heartbeat_runs::heartbeat_run_routes())
        .with_state(state);

    Router::new()
        // The Paperclip HTTP contract exposes all service routes below `/api`.
        .nest("/api", api_routes)

        // Middleware layers
        .layer(axum::middleware::from_fn(
            crate::middleware::security_headers::security_headers_middleware,
        ))
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Health check endpoint
#[allow(dead_code)]
async fn health_check() -> &'static str {
    "OK"
}
