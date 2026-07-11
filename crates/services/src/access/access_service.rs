use async_trait::async_trait;
use uuid::Uuid;

use super::abac::{AccessDecision, Action, Actor};

#[async_trait]
pub trait AccessService: Send + Sync {
    async fn decide(
        &self,
        action: &Action,
        actor: &dyn Actor,
        resource: Option<&ResourceContext>,
    ) -> AccessDecision;

    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError>;

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_built_in_agents_enabled(
        &self,
        company_id: Uuid,
    ) -> Result<(), AccessError>;
}

#[derive(Debug, Clone)]
pub struct ResourceContext {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub company_id: Uuid,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Agent,
    AgentConfig,
    Task,
    Company,
    BuiltInAgent,
    Routine,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("Access denied: {0}")]
    Denied(String),

    #[error("Company access denied for company {company_id}")]
    CompanyAccessDenied { company_id: Uuid },

    #[error("Agent access denied for agent {agent_id}")]
    AgentAccessDenied { agent_id: Uuid },

    #[error("Permission required: {permission}")]
    PermissionRequired { permission: String },

    #[error("Built-in agents feature is not enabled for this company")]
    BuiltInAgentsNotEnabled,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub struct DefaultAccessService;

impl DefaultAccessService {
    pub fn new() -> Self {
        Self
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
        action: &Action,
        actor: &dyn Actor,
        resource: Option<&ResourceContext>,
    ) -> AccessDecision {
        if actor.is_board_admin() || actor.is_instance_admin() {
            return AccessDecision::allow_with_reason("Admin access");
        }

        match action {
            Action::AgentsCreate => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            if actor.permissions().can_create_agents {
                                return AccessDecision::allow();
                            }
                            return AccessDecision::deny("Missing can_create_agents permission");
                        }
                        return AccessDecision::deny("Company mismatch");
                    }
                }
                AccessDecision::deny("No company context")
            }
            Action::AgentRead => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                        return AccessDecision::deny("Company mismatch");
                    }
                }
                AccessDecision::deny("No company context")
            }
            Action::AgentConfigUpdate | Action::AgentConfigRead => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                    }
                }
                AccessDecision::deny("Access denied")
            }
            Action::TasksAssign => {
                if actor.permissions().can_create_agents {
                    AccessDecision::allow()
                } else {
                    AccessDecision::deny("Missing can_create_agents permission")
                }
            }
            Action::AgentsDelete => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                    }
                }
                AccessDecision::deny("Access denied")
            }
            Action::AgentsProvisionBuiltIn | Action::RoutinesControlBuiltIn => {
                if actor.is_board_admin() {
                    AccessDecision::allow()
                } else {
                    AccessDecision::deny("Board admin required")
                }
            }
        }
    }

    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError> {
        if actor.is_instance_admin() {
            return Ok(());
        }

        match actor.company_id() {
            Some(actor_company) if actor_company == company_id => Ok(()),
            _ => Err(AccessError::CompanyAccessDenied { company_id }),
        }
    }

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Agent,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentRead, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::AgentAccessDenied { agent_id })
        }
    }

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Company,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentsCreate, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agents:create".to_string(),
            })
        }
    }

    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::AgentConfig,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentConfigUpdate, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agent_config:update".to_string(),
            })
        }
    }

    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::AgentConfig,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentConfigRead, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agent_config:read".to_string(),
            })
        }
    }

    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::BuiltInAgent,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentsProvisionBuiltIn, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agents:provision_built_in".to_string(),
            })
        }
    }

    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Routine,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::RoutinesControlBuiltIn, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "routines:control_built_in".to_string(),
            })
        }
    }

    async fn assert_built_in_agents_enabled(
        &self,
        _company_id: Uuid,
    ) -> Result<(), AccessError> {
        // TODO: 实际实现需要检查公司配置中的实验特性开关
        // 目前默认返回启用
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::abac::{AgentActor, AgentPermissions, TrustPreset, AuthorizationPolicy, UserActor};

    #[tokio::test]
    async fn test_admin_access() {
        let service = DefaultAccessService::new();
        let admin = UserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_board_admin: true,
            is_instance_admin: false,
        };

        let decision = service.decide(&Action::AgentsCreate, &admin, None).await;
        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_agent_create_permission() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let permissions = AgentPermissions {
            can_create_agents: true,
            can_create_skills: false,
            trust_preset: TrustPreset::Medium,
            authorization_policy: AuthorizationPolicy::RequireApproval,
        };

        let agent = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id,
            permissions,
        };

        let resource = ResourceContext {
            resource_type: ResourceType::Company,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = service.decide(&Action::AgentsCreate, &agent, Some(&resource)).await;
        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_company_access_denied() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();
        let different_company = Uuid::new_v4();

        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_board_admin: false,
            is_instance_admin: false,
        };

        let result = service.assert_company_access(&user, different_company).await;
        assert!(result.is_err());
    }
}
