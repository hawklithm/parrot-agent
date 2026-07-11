use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ServiceError;

/// ABAC action enumeration for agent operations
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    AgentsCreate,
    AgentRead,
    AgentConfigUpdate,
    AgentConfigRead,
    TasksAssign,
    AgentTerminate,
    AgentHire,
}

impl std::fmt::Display for AgentAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentsCreate => write!(f, "agents:create"),
            Self::AgentRead => write!(f, "agent:read"),
            Self::AgentConfigUpdate => write!(f, "agent_config:update"),
            Self::AgentConfigRead => write!(f, "agent_config:read"),
            Self::TasksAssign => write!(f, "tasks:assign"),
            Self::AgentTerminate => write!(f, "agent:terminate"),
            Self::AgentHire => write!(f, "agent:hire"),
        }
    }
}

/// Access decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessDecision {
    pub allowed: bool,
    pub reason: String,
    pub action: String,
}

/// Actor trait for permission checks
pub trait Actor: Send + Sync {
    fn company_id(&self) -> Uuid;
    fn is_agent(&self) -> bool;
    fn agent_id(&self) -> Option<Uuid>;
    fn is_board_user(&self) -> bool;
    fn user_id(&self) -> Option<Uuid>;
}

/// Board user actor
#[derive(Debug, Clone)]
pub struct BoardUserActor {
    pub user_id: Uuid,
    pub company_id: Uuid,
    pub is_admin: bool,
}

impl Actor for BoardUserActor {
    fn company_id(&self) -> Uuid {
        self.company_id
    }

    fn is_agent(&self) -> bool {
        false
    }

    fn agent_id(&self) -> Option<Uuid> {
        None
    }

    fn is_board_user(&self) -> bool {
        true
    }

    fn user_id(&self) -> Option<Uuid> {
        Some(self.user_id)
    }
}

/// Agent actor
#[derive(Debug, Clone)]
pub struct AgentActor {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub can_create_agents: bool,
    pub can_assign_tasks: bool,
}

impl Actor for AgentActor {
    fn company_id(&self) -> Uuid {
        self.company_id
    }

    fn is_agent(&self) -> bool {
        true
    }

    fn agent_id(&self) -> Option<Uuid> {
        Some(self.agent_id)
    }

    fnrd_user(&self) -> bool {
        false
    }

    fn user_id(&self) -> Option<Uuid> {
        None
    }
}

/// Access service for ABAC permission checks
#[async_trait]
pub trait AgentAccessService: Send + Sync {
    /// Make access decision for an action
    async fn decide(
        &self,
        action: AgentAction,
        actor: &dyn Actor,
        resource: Option<&AgentResource>,
    ) -> Result<AccessDecision, ServiceError>;

    /// Assert company access
    async fn assert_company_access(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert agent read permission
    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert can create agents for company
    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert can update agent configuration
    async fn assert_can_update_agent_config(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert can assign tasks
    async fn assert_can_assign_tasks(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError>;

    /// Assert can terminate agent
    async fn assert_can_terminate_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError>;
}

/// Agent resource for permission checks
#[derive(Debug, Clone)]
pub struct AgentResource {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub responsible_user_id: Option<Uuid>,
}

/// Default implementation of AgentAccessService
pub struct DefaultAgentAccessService {
    // In production: inject AgentRepository to fetch agent details
    _marker: std::marker::PhantomData<()>,
}

impl DefaultAgentAccessService {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Check if actor has company access
    fn has_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> bool {
        actor.company_id() == company_id
    }

    /// Check if actor is admin (board user with admin role)
    fn is_admin(&self, actor: &dyn Actor) -> bool {
        // In production: check user's role from database
        actor.is_board_user()
    }
}

impl Default for DefaultAgentAccessService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAccessService for DefaultAgentAccessService {
    async fn decide(
        &self,
        action: AgentAction,
        actor: &dyn Actor,
        resource: Option<&AgentResource>,
    ) -> Result<AccessDecision, ServiceError> {
        // Check company access first
        if let Some(res) = resource {
            if !self.has_company_access(actor, res.company_id) {
                return Ok(AccessDecision {
                    allowed: false,
                    reason: "Actor does not have access to this company".to_string(),
                    action: action.to_string(),
                });
            }
        }

        let allowed = match action {
            AgentAction::AgentsCreate => {
                // Board users can create agents
                if actor.is_board_user() {
                    true
                } else if actor.is_agent() {
                    // Agents with can_create_agents permission can create
                    if let Some(agent_id) = actor.agent_id() {
                        // In production: fetch agent permissions from database
                        // For now: check via AgentActor struct
                        false // Placeholder
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            AgentAction::AgentRead => {
                // Same company access allows reading
                resource.map(|r| self.has_company_access(actor, r.company_id))
                    .unwrap_or(false)
            }
            AgentAction::AgentConfigUpdate | AgentAction::AgentTerminate | AgentAction::AgentHire => {
                // Admin or responsible user can update/terminate/hire
                self.is_admin(actor)
            }
            AgentAction::AgentConfigRead => {
                // Same company access allows reading config
                resource.map(|r| self.has_company_access(actor, r.company_id))
                    .unwrap_or(false)
            }
            AgentAction::TasksAssign => {
                // Board users and agents with permission can assign tasks
                actor.is_board_user() || actor.is_agent()
            }
        };

        Ok(AccessDecision {
            allowed,
            reason: if allowed {
                "Permission grtring()
            } else {
                format!("Insufficient permissions for {}", action)
            },
            action: action.to_string(),
        })
    }

    async fn assert_company_access(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError> {
        if !self.has_company_access(actor, company_id) {
            return Err(ServiceError::Forbidden(
                "No access to this company".to_string(),
            ));
        }
        Ok(())
    }

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError> {
        // In production: fetch agent's company_id from database
        // For now: assume agent_id check passes if actor has agent_id
        if actor.agent_id() == Some(agent_id) {
            return Ok(());
        }

        // Or if actor is board user in same company (needs db lookup)
        if actor.is_board_user() {
            return Ok(());
        }

        Err(ServiceError::Forbidden(
            "Cannot read this agent".to_string(),
        ))
    }

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError> {
        self.assert_company_access(actor, company_id).await?;

        let decision = self
            .decide(AgentAction::AgentsCreate, actor, None)
            .await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.reason));
        }

        Ok(())
    }

    async fn assert_can_update_agent_config(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError> {
        // In production: fetch agent resource from database
        let resource = AgentResource {
            agent_id,
            company_id: actor.company_id(), // Placeholder
            responsible_user_id: None,
        };

        let decision = self
            .decide(AgentAction::AgentConfigUpdate, actor, Some(&resource))
            .await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.reason));
        }

        Ok(())
    }

    async fn assert_can_assign_tasks(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), ServiceError> {
        self.assert_company_access(actor, company_id).await?;

        let decision = self.decide(AgentAction::TasksAssign, actor, None).await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.reason));
        }

        Ok(())
    }

    async fn assert_can_terminate_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
    ) -> Result<(), ServiceError> {
        let resource = AgentResource {
            agent_id,
            company_id: actor.company_id(),
            responsible_user_id: None,
        };

        let decision = self
            .decide(AgentAction::AgentTerminate, actor, Some(&resource))
            .await?;

        if !decision.allowed {
            return Err(ServiceError::Forbidden(decision.reason));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_board_user_can_create_agents() {
        let service = DefaultAgentAccessService::new();

        let actor = BoardUserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_admin: true,
        };

        let decision = service
            .decide(AgentAction::AgentsCreate, &actor, None)
            .await
            .unwrap();

        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_company_access_check() {
        let service = DefaultAgentAccessService::new();

        let company_id = Uuid::new_v4();
        let actor = BoardUserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_admin: true,
        };

        let result = service.assert_company_access(&actor, company_id).await;
        assert!(result.is_ok());

        let other_company = Uuid::new_v4();
        let result = service.assert_company_access(&actor, other_company).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_read_permission() {
        let service = DefaultAgentAccessService::new();

        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let resource = AgentResource {
            agent_id,
            company_id,
            responsible_user_id: None,
        };

        let actor = BoardUserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_admin: false,
        };

        let decision = service
            .decide(AgentAction::AgentRead, &actor, Some(&resource))
            .await
            .unwrap();

        assert!(decision.allowed);
    }
}
