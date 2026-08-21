use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;
use regex::Regex;

use crate::auth::{AuthorizationAction, AuthorizationActor, AuthorizationService};

#[derive(Debug, Error)]
pub enum AuthorizationError {
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),

    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid command pattern: {0}")]
    InvalidCommandPattern(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type AuthorizationResult<T> = Result<T, AuthorizationError>;

/// Authorization decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthzDecision {
    pub allowed: bool,
    pub reason: String,
}

impl AuthzDecision {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: reason.into(),
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: reason.into(),
        }
    }
}

/// Command authorization request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAuthzRequest {
    pub workspace_id: Uuid,
    pub command: String,
    pub agent_id: Option<Uuid>,
}

/// Runtime service authorization request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeServiceAuthzRequest {
    pub workspace_id: Uuid,
    pub service_name: String,
    pub action: RuntimeServiceAction,
    pub agent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeServiceAction {
    Start,
    Stop,
    Restart,
    Run,
}

/// Authorization policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationPolicy {
    pub allowed_commands: Vec<String>,
    pub denied_commands: Vec<String>,
    pub allowed_runtime_actions: Vec<RuntimeServiceAction>,
    pub require_runtime_manage_permission: bool,
}

impl Default for AuthorizationPolicy {
    fn default() -> Self {
        Self {
            allowed_commands: vec!["*".to_string()],
            denied_commands: vec![
                "rm -rf /".to_string(),
                "sudo rm -rf".to_string(),
                "chmod 777".to_string(),
                ": (){;:".to_string(), // Fork bomb
            ],
            allowed_runtime_actions: vec![
                RuntimeServiceAction::Start,
                RuntimeServiceAction::Stop,
                RuntimeServiceAction::Restart,
                RuntimeServiceAction::Run,
            ],
            require_runtime_manage_permission: true,
        }
    }
}

/// Workspace command authorization service trait
#[async_trait]
pub trait WorkspaceCommandAuthzService: Send + Sync {
    /// Check if a command is allowed to execute in the workspace
    async fn check_command_permission(
        &self,
        request: CommandAuthzRequest,
    ) -> AuthorizationResult<AuthzDecision>;
}

/// Workspace runtime service authorization service trait
#[async_trait]
pub trait WorkspaceRuntimeServiceAuthzService: Send + Sync {
    /// Check if a runtime service operation is allowed
    async fn check_runtime_service_permission(
        &self,
        request: RuntimeServiceAuthzRequest,
    ) -> AuthorizationResult<AuthzDecision>;
}

/// Default implementation of command authorization service
pub struct DefaultCommandAuthzService {
    policy: AuthorizationPolicy,
}

impl DefaultCommandAuthzService {
    pub fn new(policy: AuthorizationPolicy) -> Self {
        Self { policy }
    }

    pub fn with_default_policy() -> Self {
        Self {
            policy: AuthorizationPolicy::default(),
        }
    }

    /// Check if command matches any pattern in the list
    fn matches_any_pattern(command: &str, patterns: &[String]) -> bool {
        patterns.iter().any(|pattern| {
            if pattern == "*" {
                return true;
            }

            // Convert shell wildcard pattern to regex
            let regex_pattern = pattern
                .replace(".", "\\.")
                .replace("*", ".*")
                .replace("?", ".");

            if let Ok(re) = Regex::new(&format!("^{}$", regex_pattern)) {
                re.is_match(command)
            } else {
                command.contains(pattern)
            }
        })
    }

    /// Check if command is dangerous
    fn is_dangerous_command(command: &str) -> bool {
        let dangerous_patterns = [
            "rm -rf /",
            "sudo rm -rf",
            "chmod 777",
            "chmod -R 777",
            ": (){ :|: & };:", // Fork bomb
            "mkfs",
            "dd if=/dev/zero",
            "mv / ",
            "wget | sh",
            "curl | bash",
            "> /dev/sda",
        ];

        dangerous_patterns.iter().any(|pattern| command.contains(pattern))
    }
}

#[async_trait]
impl WorkspaceCommandAuthzService for DefaultCommandAuthzService {
    async fn check_command_permission(
        &self,
        request: CommandAuthzRequest,
    ) -> AuthorizationResult<AuthzDecision> {
        let command = request.command.trim();

        // Check for dangerous commands
        if Self::is_dangerous_command(command) {
            return Ok(AuthzDecision::deny(format!(
                "Command is potentially dangerous: {}",
                command
            )));
        }

        // Check against denied patterns after the semantic danger check so
        // callers receive the stronger reason for commands such as `rm -rf /`.
        if Self::matches_any_pattern(command, &self.policy.denied_commands) {
            return Ok(AuthzDecision::deny(format!(
                "Command matches denied pattern: {}",
                command
            )));
        }

        // Check against allowed patterns
        if Self::matches_any_pattern(command, &self.policy.allowed_commands) {
            return Ok(AuthzDecision::allow(format!(
                "Command matches allowed pattern: {}",
                command
            )));
        }

        // Default deny if not explicitly allowed
        Ok(AuthzDecision::deny(format!(
            "Command not in allowed list: {}",
            command
        )))
    }
}

/// Default implementation of runtime service authorization service
pub struct DefaultRuntimeServiceAuthzService {
    policy: AuthorizationPolicy,
    pool: Option<PgPool>,
}

impl DefaultRuntimeServiceAuthzService {
    pub fn new(policy: AuthorizationPolicy) -> Self {
        Self { policy, pool: None }
    }

    pub fn with_default_policy() -> Self {
        Self {
            policy: AuthorizationPolicy::default(),
            pool: None,
        }
    }

    pub fn with_pool(policy: AuthorizationPolicy, pool: PgPool) -> Self {
        Self {
            policy,
            pool: Some(pool),
        }
    }

    pub fn with_default_policy_and_pool(pool: PgPool) -> Self {
        Self::with_pool(AuthorizationPolicy::default(), pool)
    }

    fn action_allowed(&self, action: &RuntimeServiceAction) -> bool {
        self.policy.allowed_runtime_actions.iter().any(|allowed| {
            matches!(
                (allowed, action),
                (RuntimeServiceAction::Start, RuntimeServiceAction::Start)
                | (RuntimeServiceAction::Stop, RuntimeServiceAction::Stop)
                | (RuntimeServiceAction::Restart, RuntimeServiceAction::Restart)
                | (RuntimeServiceAction::Run, RuntimeServiceAction::Run)
            )
        })
    }
}

#[async_trait]
impl WorkspaceRuntimeServiceAuthzService for DefaultRuntimeServiceAuthzService {
    async fn check_runtime_service_permission(
        &self,
        request: RuntimeServiceAuthzRequest,
    ) -> AuthorizationResult<AuthzDecision> {
        // Check if action is in allowed list
        if !self.action_allowed(&request.action) {
            return Ok(AuthzDecision::deny(format!(
                "Runtime service action not allowed: {:?}",
                request.action
            )));
        }

        // TODO: Integrate with accessService.decide() to check runtime:manage permission
        // For now, allow if require_runtime_manage_permission is false
        if self.policy.require_runtime_manage_permission {
            let Some(agent_id) = request.agent_id else {
                return Ok(AuthzDecision::deny(
                    "Agent ID required for permission check".to_string(),
                ));
            };
            let Some(pool) = &self.pool else {
                return Ok(AuthzDecision::deny(
                    "Runtime access service is not configured".to_string(),
                ));
            };

            let workspace_company_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT company_id FROM execution_workspaces WHERE id = $1",
            )
            .bind(request.workspace_id)
            .fetch_optional(pool)
            .await?
            .ok_or(AuthorizationError::WorkspaceNotFound(request.workspace_id))?;
            let agent_company_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT company_id FROM agents WHERE id = $1 AND status <> 'terminated'",
            )
            .bind(agent_id)
            .fetch_optional(pool)
            .await?
            .ok_or(AuthorizationError::AgentNotFound(agent_id))?;
            if workspace_company_id != agent_company_id {
                return Ok(AuthzDecision::deny(
                    "Agent and workspace belong to different companies".to_string(),
                ));
            }

            let actor = AuthorizationActor::agent(agent_id, agent_company_id, None);
            let decision = AuthorizationService::decide(
                pool,
                &actor,
                &AuthorizationAction::Custom {
                    action: "runtime:manage".to_string(),
                    resource_id: Some(request.workspace_id),
                },
                Some(workspace_company_id),
            )
            .await;
            if !decision.allowed {
                return Ok(AuthzDecision::deny(format!(
                    "Runtime manage permission denied: {}",
                    decision.explanation
                )));
            }
        }

        Ok(AuthzDecision::allow(format!(
            "Runtime service action allowed: {:?} on {}",
            request.action, request.service_name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_command_authz_allowed() {
        let service = DefaultCommandAuthzService::with_default_policy();

        let request = CommandAuthzRequest {
            workspace_id: Uuid::new_v4(),
            command: "ls -la".to_string(),
            agent_id: None,
        };

        let result = service.check_command_permission(request).await.unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_command_authz_dangerous() {
        let service = DefaultCommandAuthzService::with_default_policy();

        let request = CommandAuthzRequest {
            workspace_id: Uuid::new_v4(),
            command: "rm -rf /".to_string(),
            agent_id: None,
        };

        let result = service.check_command_permission(request).await.unwrap();
        assert!(!result.allowed);
        assert!(result.reason.contains("dangerous"));
    }

    #[tokio::test]
    async fn test_command_authz_denied_pattern() {
        let service = DefaultCommandAuthzService::with_default_policy();

        let request = CommandAuthzRequest {
            workspace_id: Uuid::new_v4(),
            command: "chmod 777 /etc/passwd".to_string(),
            agent_id: None,
        };

        let result = service.check_command_permission(request).await.unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_runtime_service_authz_allowed() {
        let service = DefaultRuntimeServiceAuthzService::new(AuthorizationPolicy {
            require_runtime_manage_permission: false,
            ..AuthorizationPolicy::default()
        });

        let request = RuntimeServiceAuthzRequest {
            workspace_id: Uuid::new_v4(),
            service_name: "postgres".to_string(),
            action: RuntimeServiceAction::Start,
            agent_id: Some(Uuid::new_v4()),
        };

        let result = service.check_runtime_service_permission(request).await.unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_runtime_service_authz_fails_closed_without_access_service() {
        let service = DefaultRuntimeServiceAuthzService::with_default_policy();
        let request = RuntimeServiceAuthzRequest {
            workspace_id: Uuid::new_v4(),
            service_name: "postgres".to_string(),
            action: RuntimeServiceAction::Start,
            agent_id: Some(Uuid::new_v4()),
        };

        let result = service
            .check_runtime_service_permission(request)
            .await
            .expect("authorization decision");
        assert!(!result.allowed);
        assert!(result.reason.contains("not configured"));
    }

    #[tokio::test]
    async fn test_runtime_service_authz_uses_database_grants_and_scope() {
        let Some(database_url) = std::env::var_os("DATABASE_URL") else {
            eprintln!("skipping runtime authz integration test: DATABASE_URL is not set");
            return;
        };
        let pool = PgPool::connect(
            database_url
                .to_str()
                .expect("DATABASE_URL must be valid UTF-8"),
        )
        .await
        .expect("connect database");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run migrations");

        let company_id = Uuid::new_v4();
        let other_company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let ungranted_agent_id = Uuid::new_v4();
        let other_agent_id = Uuid::new_v4();
        let project_id = Uuid::new_v4();
        let other_project_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let other_workspace_id = Uuid::new_v4();
        let grantor_id = Uuid::new_v4();
        let prefix = format!("RA{}", &company_id.simple().to_string()[..6]);
        let other_prefix = format!("RA{}", &other_company_id.simple().to_string()[..6]);

        for (id, name, issue_prefix) in [
            (company_id, "Runtime Authz Company", prefix),
            (other_company_id, "Other Runtime Authz Company", other_prefix),
        ] {
            sqlx::query("INSERT INTO companies (id, name, issue_prefix) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(name)
                .bind(issue_prefix)
                .execute(&pool)
                .await
                .expect("insert company");
        }
        sqlx::query("INSERT INTO auth_users (id, email) VALUES ($1, $2)")
            .bind(grantor_id)
            .bind(format!("runtime-authz-{}@example.test", grantor_id))
            .execute(&pool)
            .await
            .expect("insert grantor");
        for (id, company, name) in [
            (agent_id, company_id, "Granted runtime agent"),
            (ungranted_agent_id, company_id, "Ungrant runtime agent"),
            (other_agent_id, other_company_id, "Other company runtime agent"),
        ] {
            sqlx::query("INSERT INTO agents (id, company_id, name) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(company)
                .bind(name)
                .execute(&pool)
                .await
                .expect("insert agent");
        }
        for (id, company, name) in [
            (project_id, company_id, "Runtime authz project"),
            (other_project_id, other_company_id, "Other runtime authz project"),
        ] {
            sqlx::query("INSERT INTO projects (id, company_id, name) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(company)
                .bind(name)
                .execute(&pool)
                .await
                .expect("insert project");
        }
        for (id, company, project, name) in [
            (workspace_id, company_id, project_id, "Granted workspace"),
            (
                other_workspace_id,
                other_company_id,
                other_project_id,
                "Other workspace",
            ),
        ] {
            sqlx::query(
                "INSERT INTO execution_workspaces
                    (id, company_id, project_id, mode, strategy_type, name)
                 VALUES ($1, $2, $3, 'local', 'shared', $4)",
            )
            .bind(id)
            .bind(company)
            .bind(project)
            .bind(name)
            .execute(&pool)
            .await
            .expect("insert execution workspace");
        }
        sqlx::query(
            "INSERT INTO principal_permission_grants
                (company_id, principal_type, principal_id, permission_key, scope, granted_by_user_id)
             VALUES ($1, 'agent', $2, 'runtime:manage', '{}', $3)",
        )
        .bind(company_id)
        .bind(agent_id)
        .bind(grantor_id)
        .execute(&pool)
        .await
        .expect("insert runtime grant");
        let raw_grant: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT principal_id, permission_key FROM principal_permission_grants
             WHERE company_id = $1 AND principal_id = $2",
        )
        .bind(company_id)
        .bind(agent_id)
        .fetch_optional(&pool)
        .await
        .expect("read runtime grant");
        assert_eq!(raw_grant, Some((agent_id, "runtime:manage".to_string())));
        assert!(
            crate::auth::check_explicit_grants(
                &pool,
                company_id,
                "agent",
                agent_id,
                "runtime:manage",
            )
            .await
        );

        let service = DefaultRuntimeServiceAuthzService::with_default_policy_and_pool(pool.clone());
        let request = |workspace_id, agent_id| RuntimeServiceAuthzRequest {
            workspace_id,
            service_name: "postgres".to_string(),
            action: RuntimeServiceAction::Start,
            agent_id: Some(agent_id),
        };
        let granted = service
            .check_runtime_service_permission(request(workspace_id, agent_id))
            .await
            .expect("granted runtime authorization");
        assert!(granted.allowed, "{granted:?}");
        assert!(!service
            .check_runtime_service_permission(request(workspace_id, ungranted_agent_id))
            .await
            .expect("ungranted runtime authorization")
            .allowed);
        assert!(!service
            .check_runtime_service_permission(request(other_workspace_id, agent_id))
            .await
            .expect("cross-company runtime authorization")
            .allowed);
        assert!(matches!(
            service
                .check_runtime_service_permission(request(Uuid::new_v4(), other_agent_id))
                .await,
            Err(AuthorizationError::WorkspaceNotFound(_))
        ));

        sqlx::query("DELETE FROM execution_workspaces WHERE id IN ($1, $2)")
            .bind(workspace_id)
            .bind(other_workspace_id)
            .execute(&pool)
            .await
            .expect("cleanup execution workspaces");
        sqlx::query("DELETE FROM companies WHERE id IN ($1, $2)")
            .bind(company_id)
            .bind(other_company_id)
            .execute(&pool)
            .await
            .expect("cleanup runtime authz companies");
        sqlx::query("DELETE FROM auth_users WHERE id = $1")
            .bind(grantor_id)
            .execute(&pool)
            .await
            .expect("cleanup grantor");
    }

    #[tokio::test]
    async fn test_pattern_matching() {
        assert!(DefaultCommandAuthzService::matches_any_pattern(
            "git status",
            &["git *".to_string()]
        ));

        assert!(DefaultCommandAuthzService::matches_any_pattern(
            "npm install",
            &["npm ?nstall".to_string()]
        ));

        assert!(!DefaultCommandAuthzService::matches_any_pattern(
            "sudo rm",
            &["git *".to_string(), "npm *".to_string()]
        ));
    }
}
