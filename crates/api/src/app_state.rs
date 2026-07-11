use std::sync::Arc;
use axum::Router;

// Re-export services
pub use services::{
    AgentService, ConfigRevisionService, IssueService, CaseService,
    IssueCommentService, IssueDocumentService, IssueTreeControlService,
};
pub use access::AccessService;

/// Global application state containing all services
#[derive(Clone)]
pub struct AppState {
    // Phase 1: Agent Management
    pub agent_service: Arc<dyn AgentService>,
    pub access_service: Arc<dyn AccessService>,
    pub config_revision_service: Arc<dyn ConfigRevisionService>,

    // Phase 2: Issue/Case Management
    pub issue_service: Arc<dyn IssueService>,
    pub case_service: Arc<dyn CaseService>,
    pub issue_comment_service: Arc<dyn IssueCommentService>,
    pub issue_document_service: Arc<dyn IssueDocumentService>,
    pub issue_tree_control_service: Arc<dyn IssueTreeControlService>,
}

impl AppState {
    pub fn new(
        agent_service: Arc<dyn AgentService>,
        access_service: Arc<dyn AccessService>,
        config_revision_service: Arc<dyn ConfigRevisionService>,
        issue_service: Arc<dyn IssueService>,
        case_service: Arc<dyn CaseService>,
        issue_comment_service: Arc<dyn IssueCommentService>,
        issue_document_service: Arc<dyn IssueDocumentService>,
        issue_tree_control_service: Arc<dyn IssueTreeControlService>,
    ) -> Self {
        Self {
            agent_service,
            access_service,
            config_revision_service,
            issue_service,
            case_service,
            issue_comment_service,
            issue_document_service,
            issue_tree_control_service,
        }
    }
}

/// Create the main application router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", axum::routing::get(health_check))

        // Phase 1: Agent Management routes
        .merge(crate::routes::agents::agent_routes())
        .merge(crate::routes::adapters::adapter_routes())
        .merge(crate::routes::config_revisions::config_revision_routes())
        .merge(crate::routes::org::org_routes())

        // Phase 2: Issue/Case Management routes
        .merge(crate::routes::issues::issue_routes())
        .merge(crate::routes::cases::case_routes())
        .merge(crate::routes::issue_comments::issue_comment_routes())
        .merge(crate::routes::issue_documents::issue_document_routes())
        .merge(crate::routes::issue_tree_control::issue_tree_control_routes())

        // Apply state
        .with_state(state)

        // Middleware layers
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}
