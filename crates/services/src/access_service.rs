use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AccessError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Actor not authenticated")]
    NotAuthenticated,

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type AccessResult<T> = Result<T, AccessError>;

/// Action enum for ABAC
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Agent actions
    AgentsCreate,
    AgentRead,
    AgentConfigUpdate,
    AgentConfigRead,
    TasksAssign,
    AgentDelete,
    AgentPause,
    AgentResume,

    // Skill actions
    SkillsCreate,
    SkillRead,
    SkillUpdate,
    SkillDelete,

    // Runtime actions
    RuntimeManage,
    RuntimeRead,

    // Built-in agent actions
    BuiltInAgentProvision,
    BuiltInRoutineControl,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::AgentsCreate => write!(f, "agents:create"),
            Action::AgentRead => write!(f, "agent:read"),
            Action::AgentConfigUpdate => write!(f, "agent_config:update"),
            Action::AgentConfigRead => write!(f, "agent_config:read"),
            Action::TasksAssign => write!(f, "tasks:assign"),
            Action::AgentDelete => write!(f, "agent:delete"),
            Action::AgentPause => write!(f, "agent:pause"),
            Action::AgentResume => write!(f, "agent:resume"),
            Action::SkillsCreate => write!(f, "skills:create"),
            Action::SkillRead => write!(f, "skill:read"),
            Action::SkillUpdate => write!(f, "skill:update"),
            Action::SkillDelete => write!(f, "skill:delete"),
            Action::RuntimeManage => write!(f, "runtime:manage"),
            Action::RuntimeRead => write!(f, "runtime:read"),
            Action::BuiltInAgentProvision => write!(f, "built_in_agent:provision"),
            Action::BuiltInRoutineControl => write!(f, "built_in_routine:control"),
        }
    }
}

/// Access decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessDecision {
    pub allowed: bool,
    pub reason: String,
}

impl AccessDecision {
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

/// Actor type for ABAC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorType {
    User { user_id: Uuid },
    Agent { agent_id: Uuid },
    System,
}

/// Actor trait for access control
pub trait Actor: Send + Sync {
    fn company_id(&self) -> Uuid;
    fn is_agent(&self) -> bool;
    fn agent_id(&self) -> Option<Uuid>;
    fn user_id(&self) -> Option<Uuid>;
    fn has_permission(&self, action: &Action) -> bool;
}

/// Resource for ABAC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub company_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Agent,
    Skill,
    ExecutionWorkspace,
    Environment,
    Secret,
    Issue,
    Case,
}

/// Access service trait
#[async_trait]
pub trait AccessService: Send + Sync {
    /// Make access control decision
    async fn decide(
        &self,
        action: Action,
        actor: &dyn Actor,
        resource: &Resource,
    ) -> AccessResult<AccessDecision>;

    /// Assert company access
    async fn assert_company_access(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert agent read permission
    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert can create agents for company
    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert can update agent configuration
    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert can read agent configuration
    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert can provision built-in agents
    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert can control built-in routine
    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        routine_id: Uuid,
    ) -> AccessResult<()>;

    /// Assert built-in agents feature is enabled
    async fn assert_built_in_agents_enabled(
        &self,
        company_id: Uuid,
    ) -> AccessResult<()>;
}

/// Default access service implementation
pub struct DefaultAccessService {
    // TODO: Add dependencies (database, cache)
}

impl DefaultAccessService {
    pub fn new() -> Self {
        Self {}
    }

    fn check_company_membership(&self, actor: &dyn Actor, company_id: Uuid) -> bool {
        actor.company_id() == company_id
    }

    fn check_permission(&self, actor: &dyn Actor, action: &Action) -> bool {
        actor.has_permission(action)
    }
}

impl Default for DefaultAccessService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AccessService for DefaultAccessService {
    async fn decide(
        &self,
        action: Action,
        actor: &dyn Actor,
        resource: &Resource,
    ) -> AccessResult<AccessDecision> {
        // Check company membership
        if !self.check_company_membership(actor, resource.company_id) {
            return Ok(AccessDecision::deny(format!(
                "Actor not member of company {}",
                resource.company_id
            )));
        }

        // Check permission
        if !self.check_permission(actor, &action) {
            return Ok(AccessDecision::deny(format!(
                "Actor lacks permission: {}",
                action
            )));
        }

        Ok(AccessDecision::allow(format!(
            "Access granted for {} on {:?}",
            action, resource.resource_type
        )))
    }

    async fn assert_company_access(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()> {
        if !self.check_company_membership(actor, company_id) {
            return Err(AccessError::PermissionDenied(format!(
                "Actor not member of company {}",
                company_id
            )));
        }
        Ok(())
    }

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        _agent_id: Uuid,
    ) -> AccessResult<()> {
        if !self.check_permission(actor, &Action::AgentRead) {
            return Err(AccessError::PermissionDenied(
                "Agent read permission required".to_string(),
            ));
        }
        Ok(())
    }

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()> {
        self.assert_company_access(actor, company_id).await?;

        if !self.check_permission(actor, &Action::AgentsCreate) {
            return Err(AccessError::PermissionDenied(
                "agents:create permission required".to_string(),
            ));
        }

        Ok(())
    }

    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        _agent_id: Uuid,
    ) -> AccessResult<()> {
        if !self.check_permission(actor, &Action::AgentConfigUpdate) {
            return Err(AccessError::PermissionDenied(
                "agent_config:update permission required".to_string(),
            ));
        }

        // TODO: Add change grant and consent checks
        Ok(())
    }

    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        _agent_id: Uuid,
    ) -> AccessResult<()> {
        if !self.check_permission(actor, &Action::AgentConfigRead) {
            return Err(AccessError::PermissionDenied(
                "agent_config:read permission required".to_string(),
            ));
        }
        Ok(())
    }

    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> AccessResult<()> {
        self.assert_company_access(actor, company_id).await?;

        if !self.check_permission(actor, &Action::BuiltInAgentProvision) {
            return Err(AccessError::PermissionDenied(
                "built_in_agent:provision permission required".to_string(),
            ));
        }

        Ok(())
    }

    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        _routine_id: Uuid,
    ) -> AccessResult<()> {
        if !self.check_permission(actor, &Action::BuiltInRoutineControl) {
            return Err(AccessError::PermissionDenied(
                "built_in_routine:control permission required".to_string(),
            ));
        }

        Ok(())
    }

    async fn assert_built_in_agents_enabled(
        &self,
        _company_id: Uuid,
    ) -> AccessResult<()> {
        // TODO: Check feature flag in company settings
        // For now, assume enabled
        Ok(())
    }
}

/// Simple actor implementation for testing
pub struct SimpleActor {
    pub company_id: Uuid,
    pub actor_type: ActorType,
    pub permissions: Vec<Action>,
}

impl Actor for SimpleActor {
    fn company_id(&self) -> Uuid {
        self.company_id
    }

    fn is_agent(&self) -> bool {
        matches!(self.actor_type, ActorType::Agent { .. })
    }

    fn agent_id(&self) -> Option<Uuid> {
        match self.actor_type {
            ActorType::Agent { agent_id } => Some(agent_id),
            _ => None,
        }
    }

    fn user_id(&self) -> Option<Uuid> {
        match self.actor_type {
            ActorType::User { user_id } => Some(user_id),
            _ => None,
        }
    }

    fn has_permission(&self, action: &Action) -> bool {
        self.permissions.contains(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_access_decision() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let actor = SimpleActor {
            company_id,
            actor_type: ActorType::User { user_id: Uuid::new_v4() },
            permissions: vec![Action::AgentRead],
        };

        let resource = Resource {
            resource_type: ResourceType::Agent,
            resource_id: Uuid::new_v4(),
            company_id,
        };

        let decision = service.decide(Action::AgentRead, &actor, &resource).await.unwrap();
        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_company_access_denied() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();
        let other_company = Uuid::new_v4();

        let actor = SimpleActor {
            company_id,
            actor_type: ActorType::User { user_id: Uuid::new_v4() },
            permissions: vec![],
        };

        let result = service.assert_company_access(&actor, other_company).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_permission_check() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let actor = SimpleActor {
            company_id,
            actor_type: ActorType::User { user_id: Uuid::new_v4() },
            permissions: vec![Action::AgentsCreate],
        };

        let result = service.assert_can_create_agents_for_company(&actor, company_id).await;
        assert!(result.is_ok());
    }
}
