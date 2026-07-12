use std::sync::Arc;
use axum::Router;
use sqlx::PgPool;

// Re-export services
pub use services::{
    AgentService, ConfigRevisionService, IssueService, CaseService,
    IssueCommentService, IssueDocumentService, IssueTreeControlService,
    BuiltInAgentService, AdapterRegistry, EnvironmentRuntimeService,
    OrgChartService, LowTrustService,
};

pub use access::AccessService;

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
    pub issue_document_service: Arc<dyn IssueDocumentService>,
    pub issue_tree_control_service: Arc<dyn IssueTreeControlService>,

    // Org chart
    pub org_chart_service: Arc<dyn OrgChartService>,

    // Issue diagnostics
    pub issue_diagnostics_service: Arc<dyn services::issue_diagnostics_service::IssueDiagnosticsService>,

    // Low trust review
    pub low_trust_service: Arc<dyn LowTrustService>,

    // Shared DB pool
    pub pool: PgPool,
}

impl AppState {
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
        issue_document_service: Arc<dyn IssueDocumentService>,
        issue_tree_control_service: Arc<dyn IssueTreeControlService>,
        org_chart_service: Arc<dyn OrgChartService>,
        issue_diagnostics_service: Arc<dyn services::issue_diagnostics_service::IssueDiagnosticsService>,
        low_trust_service: Arc<dyn LowTrustService>,
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
            issue_document_service,
            issue_tree_control_service,
            org_chart_service,
            issue_diagnostics_service,
            low_trust_service,
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
    Router::new()
        // Health check
        .route("/health", axum::routing::get(health_check))

        // Phase 1: Agent Management routes
        .merge(crate::routes::agents::agent_routes())
        .merge(crate::routes::auth::auth_routes(state.clone()))
        .merge(crate::routes::access_control::access_control_routes(state.clone()))
        .merge(crate::routes::adapters::adapter_routes())
        .merge(crate::routes::config_revisions::config_revision_routes())
        .merge(crate::routes::built_in_agents::built_in_agent_routes())
        .merge(crate::routes::org::org_routes())

        // Phase 2: Issue/Case Management routes
        .merge(crate::routes::issues::issue_routes())
        .merge(crate::routes::cases::case_routes())
        .merge(crate::routes::issue_comments::issue_comment_routes())
        .merge(crate::routes::issue_documents::issue_document_routes())
        .merge(crate::routes::issue_tree_control::issue_tree_control_routes())
        .merge(crate::routes::issue_diagnostics::issue_diagnostics_routes())
        .merge(crate::routes::low_trust::low_trust_routes())

        // Apply state
        .with_state(state)

        // Middleware layers
        .layer(axum::middleware::from_fn(
            crate::middleware::security_headers::security_headers_middleware,
        ))
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
