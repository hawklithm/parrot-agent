pub mod access_control;
pub mod health;
pub mod adapters;
pub mod agents;
pub mod attachments;
pub mod auth;
pub mod built_in_agents;
pub mod cases;
pub mod companies;
pub mod config_revisions;
pub mod custom_image_setup;
pub mod environment_diagnostics;
pub mod feedback_traces;
pub mod folders;
pub mod environments;
pub mod goals;
pub mod heartbeats;
pub mod invite_resources;
pub mod invites;
pub mod interactions;
pub mod inbox_dismissals;
pub mod issue_comments;
pub mod issues;
pub mod issue_tree_control;
pub mod openclaw;
pub mod org_chart;
pub mod pipelines;
pub mod projects;
pub mod resource_memberships;
pub mod routine_annotations;
pub mod routines;
pub mod secret_provider_configs;
pub mod secret_remote_import;
pub mod secrets;
pub mod skill_policy;
pub mod sidebar_preferences;
pub mod skills;
pub mod sse;
pub mod websocket;
pub mod teams_catalog;
pub mod tools;
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
pub mod decisions;
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

pub async fn require_cloud_company_access(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use services::auth::AuthorizationActor;
    let Some(actor) = request.extensions().get::<AuthorizationActor>() else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };
    if actor.is_anonymous() {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    if let Some(raw) = request.uri().query().and_then(|query| {
        query.split('&').find_map(|part| {
            part.strip_prefix("companyId=")
                .or_else(|| part.strip_prefix("company_id="))
        })
    }) {
        if let Ok(company_id) = uuid::Uuid::parse_str(raw) {
            if assert_company_access(
                actor,
                company_id,
                request.method() == axum::http::Method::GET,
            )
            .is_err()
            {
                return axum::http::StatusCode::FORBIDDEN.into_response();
            }
        }
    }
    next.run(request).await
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
    if actor.is_anonymous() {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let path = request.uri().path();
    let method = request.method().as_str();
    let mutation_admin = method == "DELETE"
        || path == "/plugins/install"
        || path.ends_with("/enable")
        || path.ends_with("/disable")
        || path.ends_with("/upgrade");
    let agent_allowed =
        path == "/plugins/tools/execute" || path.contains("/bridge/") || path.contains("/actions/");
    if mutation_admin && assert_instance_admin(actor).is_err() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if !mutation_admin && !agent_allowed && assert_board(actor).is_err() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    if agent_allowed && assert_board_or_agent(actor).is_err() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

pub async fn require_company_access_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use services::auth::AuthorizationActor;
    let Some(actor) = request.extensions().get::<AuthorizationActor>() else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };
    if actor.is_anonymous() {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let company_id = request
        .uri()
        .path()
        .split('/')
        .find_map(|part| uuid::Uuid::parse_str(part).ok());
    if let Some(company_id) = company_id {
        if assert_company_access(
            actor,
            company_id,
            request.method() == axum::http::Method::GET,
        )
        .is_err()
        {
            return axum::http::StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(request).await
}

/// 单条路由对 company scope 的访问语义。
///
/// - `Read`：只读，允许 owner/admin/operator/viewer 与同公司 agent。
/// - `Write`：写操作，viewer 角色会被拒（对齐 Paperclip 的 read-only member 语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    Write,
}

impl AccessMode {
    pub const fn read_only(self) -> bool {
        matches!(self, Self::Read)
    }
}

/// `assert_company_access` 的语义化封装，避免调用点出现裸 bool。
pub fn require_company_access(
    actor: &services::auth::AuthorizationActor,
    company_id: uuid::Uuid,
    mode: AccessMode,
) -> Result<(), axum::http::StatusCode> {
    assert_company_access(actor, company_id, mode.read_only())
}

pub fn assert_company_access(
    actor: &services::auth::AuthorizationActor,
    company_id: uuid::Uuid,
    read_only: bool,
) -> Result<(), axum::http::StatusCode> {
    let has_company_access = actor.company_id() == Some(company_id)
        || actor.role_in(company_id).is_some()
        || matches!(
            actor,
            services::auth::AuthorizationActor::Board {
                source: services::auth::ActorSource::LocalImplicit,
                ..
            }
        );
    if actor.is_anonymous() || !has_company_access {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    if !read_only
        && actor
            .role_in(company_id)
            .is_some_and(|role| role.is_read_only())
    {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub fn assert_board(
    actor: &services::auth::AuthorizationActor,
) -> Result<(), axum::http::StatusCode> {
    actor
        .is_board()
        .then_some(())
        .ok_or(axum::http::StatusCode::FORBIDDEN)
}

pub fn assert_instance_admin(
    actor: &services::auth::AuthorizationActor,
) -> Result<(), axum::http::StatusCode> {
    (actor.is_instance_admin() || actor.is_board() && actor.company_id() == Some(uuid::Uuid::nil()))
        .then_some(())
        .ok_or(axum::http::StatusCode::FORBIDDEN)
}

pub fn assert_board_or_agent(
    actor: &services::auth::AuthorizationActor,
) -> Result<(), axum::http::StatusCode> {
    (actor.is_board() || actor.is_agent())
        .then_some(())
        .ok_or(axum::http::StatusCode::FORBIDDEN)
}

/// 写一条 activity log。对齐 Paperclip 的 `logActivity`：审计失败不阻断主流程，
/// 只记录 warn 日志（需要强一致的场景应由调用方在同事务内写入）。
///
/// 注意 `activity_logs.actor_id` / `resource_id` 均为 `UUID NOT NULL`，
/// 匿名 actor 落 nil UUID。
pub async fn log_activity(
    pool: &sqlx::PgPool,
    company_id: uuid::Uuid,
    event_type: &str,
    actor: &services::auth::AuthorizationActor,
    resource_type: &str,
    resource_id: uuid::Uuid,
    metadata: serde_json::Value,
) {
    let actor_id = actor.principal_id().unwrap_or_else(uuid::Uuid::nil);
    if let Err(err) = sqlx::query(
        r#"
        INSERT INTO activity_logs
            (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(company_id)
    .bind(event_type)
    .bind(actor.actor_type())
    .bind(actor_id)
    .bind(resource_type)
    .bind(resource_id)
    .bind(metadata)
    .execute(pool)
    .await
    {
        tracing::warn!(
            error = %err,
            event_type = %event_type,
            company_id = %company_id,
            "failed to write activity log"
        );
    }
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
pub use config_revisions::config_revision_routes;
pub use custom_image_setup::custom_image_setup_routes;
pub use environment_diagnostics::environment_diagnostics_routes;
pub use environments::environment_routes;
pub use heartbeats::list_scheduler_heartbeats;
pub use invite_resources::invite_resource_routes;
pub use invites::invite_subresource_routes;
pub use inbox_dismissals::inbox_dismissal_routes;
pub use issues::issue_routes;
pub use issue_tree_control::issue_tree_control_routes;
pub use openclaw::openclaw_routes;
pub use org_chart::org_chart_routes;
pub use routine_annotations::routine_annotation_routes;
pub use secret_provider_configs::secret_provider_config_routes;
pub use secret_remote_import::secret_remote_import_routes;
pub use secrets::secret_routes;
pub use sidebar_preferences::sidebar_preference_routes;
pub use skill_policy::skill_policy_routes;
pub use teams_catalog::teams_catalog_routes;
pub use skills::skill_routes;
pub use sse::sse_routes;
pub use websocket::websocket_routes;
pub use user_directory::user_directory_routes;
pub use user_secret_definitions::user_secret_definition_routes;
pub use user_secrets::user_secret_routes;
pub use work_products::work_product_routes;
pub mod issue_diagnostics;
pub use issue_diagnostics::issue_diagnostics_routes;
pub mod low_trust;
pub use low_trust::low_trust_routes;
pub use feedback_traces::feedback_trace_routes;
pub use folders::folder_routes;
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
pub use decisions::decision_routes;
pub use resource_memberships::resource_membership_routes;

/// 权限测试用的 actor 构造器（board owner/admin/operator/viewer、agent、匿名）。
/// 供 `attachments` / `work_products` 等模块的权限矩阵测试复用。
#[cfg(test)]
pub(crate) mod access_test_support {
    use services::auth::{
        ActorSource, AuthorizationActor, CompanyMembership, MembershipRole, PrincipalType,
    };
    use uuid::Uuid;

    /// 带真实成员角色的 Board actor（走 Session 来源，避开 LocalImplicit 的本地放行分支）。
    pub(crate) fn board_with_role(company_id: Uuid, role: MembershipRole) -> AuthorizationActor {
        let user_id = Uuid::new_v4();
        let membership =
            CompanyMembership::new(company_id, PrincipalType::User, user_id, role);
        AuthorizationActor::board_with_source(
            user_id,
            company_id,
            ActorSource::Session,
            vec![membership],
            false,
        )
    }

    /// 另一家公司的 Board actor（用于跨租户断言）。
    pub(crate) fn board_of_other_company(
        other_company_id: Uuid,
        role: MembershipRole,
    ) -> AuthorizationActor {
        board_with_role(other_company_id, role)
    }

    pub(crate) fn agent_of(company_id: Uuid) -> AuthorizationActor {
        AuthorizationActor::agent_with_source(
            Uuid::new_v4(),
            company_id,
            None,
            ActorSource::AgentKey,
        )
    }

    pub(crate) fn anonymous() -> AuthorizationActor {
        AuthorizationActor::none()
    }
}

#[cfg(test)]
mod tests {
    use super::{assert_company_access, require_company_access, AccessMode};
    use crate::routes::access_test_support::{
        agent_of, anonymous, board_of_other_company, board_with_role,
    };
    use services::auth::{ActorSource, AuthorizationActor, MembershipRole};
    use uuid::Uuid;

    #[test]
    fn local_trusted_actor_can_access_development_company_routes() {
        let actor = AuthorizationActor::board_with_source(
            Uuid::new_v4(),
            Uuid::nil(),
            ActorSource::LocalImplicit,
            vec![],
            false,
        );

        assert!(assert_company_access(&actor, Uuid::new_v4(), true).is_ok());
    }

    #[test]
    fn access_mode_maps_to_read_only_flag() {
        assert!(AccessMode::Read.read_only());
        assert!(!AccessMode::Write.read_only());
    }

    /// board owner/admin/operator 读写皆可；viewer 只读；跨公司 board、匿名一律 403。
    #[test]
    fn company_access_matrix_covers_board_agent_viewer() {
        let company = Uuid::new_v4();
        let other = Uuid::new_v4();

        for role in [
            MembershipRole::Owner,
            MembershipRole::Admin,
            MembershipRole::Operator,
        ] {
            let actor = board_with_role(company, role);
            assert!(
                require_company_access(&actor, company, AccessMode::Read).is_ok(),
                "{role:?} should read"
            );
            assert!(
                require_company_access(&actor, company, AccessMode::Write).is_ok(),
                "{role:?} should write"
            );
            // 跨公司一律拒绝
            assert!(require_company_access(&actor, other, AccessMode::Read).is_err());
            assert!(require_company_access(&actor, other, AccessMode::Write).is_err());
        }

        let viewer = board_with_role(company, MembershipRole::Viewer);
        assert!(require_company_access(&viewer, company, AccessMode::Read).is_ok());
        assert_eq!(
            require_company_access(&viewer, company, AccessMode::Write),
            Err(axum::http::StatusCode::FORBIDDEN),
            "viewer must not write"
        );

        let foreign_owner = board_of_other_company(other, MembershipRole::Owner);
        assert!(require_company_access(&foreign_owner, company, AccessMode::Read).is_err());

        let agent = agent_of(company);
        assert!(require_company_access(&agent, company, AccessMode::Read).is_ok());
        assert!(require_company_access(&agent, company, AccessMode::Write).is_ok());
        let foreign_agent = agent_of(other);
        assert!(require_company_access(&foreign_agent, company, AccessMode::Read).is_err());
        assert!(require_company_access(&foreign_agent, company, AccessMode::Write).is_err());

        let anon = anonymous();
        assert_eq!(
            require_company_access(&anon, company, AccessMode::Read),
            Err(axum::http::StatusCode::FORBIDDEN)
        );
        assert_eq!(
            require_company_access(&anon, company, AccessMode::Write),
            Err(axum::http::StatusCode::FORBIDDEN)
        );
    }
}
