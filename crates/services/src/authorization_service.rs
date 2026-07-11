use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use uuid::Uuid;
use regex::Regex;

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

        // Check against denied patterns first
        if Self::matches_any_pattern(command, &self.policy.denied_commands) {
            return Ok(AuthzDecision::deny(format!(
                "Command matches denied pattern: {}",
                command
            )));
        }

        // Check for dangerous commands
        if Self::is_dangerous_command(command) {
            return Ok(AuthzDecision::deny(format!(
                "Command is potentially dangerous: {}",
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
}

impl DefaultRuntimeServiceAuthzService {
    pub fn new(policy: AuthorizationPolicy) -> Self {
        Self { policy }
    }

    pub fn with_default_policy() -> Self {
        Self {
            policy: AuthorizationPolicy::default(),
        }
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
            if request.agent_id.is_none() {
                return Ok(AuthzDecision::deny(
                    "Agent ID required for permission check".to_string(),
                ));
            }

            // TODO: Call accessService.decide({
            //   actor: { type: 'agent', agentId: request.agent_id },
            //   resource: { type: 'execution_workspace', id: request.workspace_id },
            //   action: 'runtime:manage',
            // })
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
        let service = DefaultRuntimeServiceAuthzService::with_default_policy();

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
