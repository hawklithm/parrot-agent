pub mod access_control;
pub mod adapters;
pub mod agents;
pub mod attachments;
pub mod auth;
pub mod built_in_agents;
pub mod cases;
pub mod comments;
pub mod companies;
pub mod config_revisions;
pub mod custom_image_setup;
pub mod environment_diagnostics;
pub mod environments;
pub mod goals;
pub mod heartbeats;
pub mod invite_resources;
pub mod invites;
pub mod issue_comments;
pub mod issue_tree_control;
pub mod issues;
pub mod openclaw;
pub mod org_chart;
pub mod pipelines;
pub mod projects;
pub mod routine_annotations;
pub mod routines;
pub mod secret_provider_configs;
pub mod secret_remote_import;
pub mod secrets;
pub mod skills;
pub mod sse;
pub mod tree_control;
pub mod user_directory;
pub mod user_secret_definitions;
pub mod user_secrets;
pub mod work_products;
// P2: New domains
pub mod activity;
pub mod approvals;
pub mod assets;
pub mod board_chat;
pub mod cloud_upstreams;
pub mod costs;
pub mod execution_workspaces;
pub mod heartbeat_runs;
pub mod instance_settings;
pub mod labels;
pub mod llms;
pub mod plugins;

/// Reject requests which did not receive an actor from the global auth middleware.
/// Route-specific company/role checks remain in the handlers as the actor also
/// carries membership information.
pub async fn require_authenticated(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use services::auth::AuthorizationActor;
    match request.extensions().get::<AuthorizationActor>() {
        Some(actor) if !actor.is_anonymous() => next.run(request).await,
        _ => axum::http::StatusCode::UNAUTHORIZED.into_response(),
    }
}

pub async fn require_plugin_access(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use services::auth::AuthorizationActor;
    let Some(actor) = request.extensions().get::<AuthorizationActor>() else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };
    if actor.is_anonymous() { return axum::http::StatusCode::UNAUTHORIZED.into_response(); }
    let path = request.uri().path();
    let method = request.method().as_str();
    let mutation_admin = method == "DELETE"
        || path == "/plugins/install"
        || path.ends_with("/enable")
        || path.ends_with("/disable")
        || path.ends_with("/upgrade");
    let agent_allowed = path == "/plugins/tools/execute"
        || path.contains("/bridge/")
        || path.contains("/actions/");
    if mutation_admin && !(actor.is_instance_admin() || actor.is_board() && actor.company_id() == Some(uuid::Uuid::nil())) {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if !mutation_admin && !agent_allowed && !actor.is_board() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if agent_allowed && !actor.is_board() && !actor.is_agent() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

pub async fn require_company_access(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use services::auth::AuthorizationActor;
    let Some(actor) = request.extensions().get::<AuthorizationActor>() else { return axum::http::StatusCode::UNAUTHORIZED.into_response(); };
    if actor.is_anonymous() { return axum::http::StatusCode::UNAUTHORIZED.into_response(); }
    let company_id = request.uri().path().split('/').find_map(|part| uuid::Uuid::parse_str(part).ok());
    if let Some(company_id) = company_id {
        if actor.company_id() != Some(company_id) { return axum::http::StatusCode::FORBIDDEN.into_response(); }
        if request.method() != axum::http::Method::GET && actor.role_in(company_id).is_some_and(|role| role.is_read_only()) {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(request).await
}

pub use access_control::{access_control_routes, CompanyId, MemberId, Token};
pub use adapters::adapter_routes;
pub use agents::agent_routes;
pub use attachments::attachment_routes;
pub use auth::auth_routes;
pub use built_in_agents::{
    built_in_agent_routes, list_built_in_agents, provision_built_in_agent, reconcile_built_in_agent,
};
pub use cases::case_routes;
pub use comments::comment_routes;
pub use config_revisions::config_revision_routes;
pub use custom_image_setup::custom_image_setup_routes;
pub use environment_diagnostics::environment_diagnostics_routes;
pub use environments::environment_routes;
pub use heartbeats::list_scheduler_heartbeats;
pub use invite_resources::invite_resource_routes;
pub use invites::invite_subresource_routes;
pub use issues::issue_routes;
pub use openclaw::openclaw_routes;
pub use org_chart::org_chart_routes;
pub use routine_annotations::routine_annotation_routes;
pub use secret_provider_configs::secret_provider_config_routes;
pub use secret_remote_import::secret_remote_import_routes;
pub use secrets::secret_routes;
pub use skills::skill_routes;
pub use sse::sse_routes;
pub use tree_control::tree_control_routes;
pub use user_directory::user_directory_routes;
pub use user_secret_definitions::user_secret_definition_routes;
pub use user_secrets::user_secret_routes;
pub use work_products::work_product_routes;
pub mod issue_diagnostics;
pub use issue_diagnostics::issue_diagnostics_routes;
pub mod low_trust;
pub use low_trust::low_trust_routes;
pub mod watchdogs;
pub use companies::company_routes;
pub use goals::goal_routes;
pub use pipelines::pipeline_routes;
pub use projects::project_routes;
pub use routines::routine_routes;
pub use watchdogs::watchdog_routes;
// P2: New domain routes
pub use approvals::approval_routes;
pub use costs::cost_routes;
pub use plugins::plugin_routes;
