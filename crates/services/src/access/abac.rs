use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[serde(rename = "agents:create")]
    AgentsCreate,
    #[serde(rename = "agent:read")]
    AgentRead,
    #[serde(rename = "agent_config:update")]
    AgentConfigUpdate,
    #[serde(rename = "agent_config:read")]
    AgentConfigRead,
    #[serde(rename = "tasks:assign")]
    TasksAssign,
    #[serde(rename = "agents:delete")]
    AgentsDelete,
    #[serde(rename = "agents:provision_built_in")]
    AgentsProvisionBuiltIn,
    #[serde(rename = "routines:control_built_in")]
    RoutinesControlBuiltIn,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::AgentsCreate => "agents:create",
            Action::AgentRead => "agent:read",
            Action::AgentConfigUpdate => "agent_config:update",
            Action::AgentConfigRead => "agent_config:read",
            Action::TasksAssign => "tasks:assign",
            Action::AgentsDelete => "agents:delete",
            Action::AgentsProvisionBuiltIn => "agents:provision_built_in",
            Action::RoutinesControlBuiltIn => "routines:control_built_in",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl AccessDecision {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }

    pub fn allow_with_reason(reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: Some(reason.into()),
        }
    }
}

pub trait Actor: Send + Sync {
    fn company_id(&self) -> Option<Uuid>;
    fn is_agent(&self) -> bool;
    fn agent_id(&self) -> Option<Uuid>;
    fn permissions(&self) -> &AgentPermissions;
    fn is_board_admin(&self) -> bool {
        false
    }
    fn is_instance_admin(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissions {
    pub can_create_agents: bool,
    pub can_create_skills: bool,
    pub trust_preset: TrustPreset,
    pub authorization_policy: AuthorizationPolicy,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            can_create_agents: false,
            can_create_skills: false,
            trust_preset: TrustPreset::Restricted,
            authorization_policy: AuthorizationPolicy::RequireApproval,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPreset {
    Full,
    High,
    Medium,
    Low,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationPolicy {
    RequireApproval,
    AutoApprove,
    Delegate,
}

#[derive(Debug, Clone)]
pub struct UserActor {
    pub user_id: Uuid,
    pub company_id: Uuid,
    pub is_board_admin: bool,
    pub is_instance_admin: bool,
}

impl Actor for UserActor {
    fn company_id(&self) -> Option<Uuid> {
        Some(self.company_id)
    }

    fn is_agent(&self) -> bool {
        false
    }

    fn agent_id(&self) -> Option<Uuid> {
        None
    }

    fn permissions(&self) -> &AgentPermissions {
        &DEFAULT_USER_PERMISSIONS
    }

    fn is_board_admin(&self) -> bool {
        self.is_board_admin
    }

    fn is_instance_admin(&self) -> bool {
        self.is_instance_admin
    }
}

static DEFAULT_USER_PERMISSIONS: AgentPermissions = AgentPermissions {
    can_create_agents: true,
    can_create_skills: true,
    trust_preset: TrustPreset::Full,
    authorization_policy: AuthorizationPolicy::AutoApprove,
};

#[derive(Debug, Clone)]
pub struct AgentActor {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub permissions: AgentPermissions,
}

impl Actor for AgentActor {
    fn company_id(&self) -> Option<Uuid> {
        Some(self.company_id)
    }

    fn is_agent(&self) -> bool {
        true
    }

    fn agent_id(&self) -> Option<Uuid> {
        Some(self.agent_id)
    }

    fn permissions(&self) -> &AgentPermissions {
        &self.permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_serialization() {
        let action = Action::AgentsCreate;
        assert_eq!(action.as_str(), "agents:create");

        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, r#""agents:create""#);

        let deserialized: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, action);
    }

    #[test]
    fn test_access_decision() {
        let allow = AccessDecision::allow();
        assert!(allow.allowed);
        assert!(allow.reason.is_none());

        let deny = AccessDecision::deny("No permission");
        assert!(!deny.allowed);
        assert_eq!(deny.reason.unwrap(), "No permission");

        let allow_with_reason = AccessDecision::allow_with_reason("Admin override");
        assert!(allow_with_reason.allowed);
        assert_eq!(allow_with_reason.reason.unwrap(), "Admin override");
    }

    #[test]
    fn test_user_actor() {
        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_board_admin: true,
            is_instance_admin: false,
        };

        assert!(!user.is_agent());
        assert!(user.is_board_admin());
        assert!(!user.is_instance_admin());
        assert!(user.company_id().is_some());
        assert!(user.agent_id().is_none());
        assert!(user.permissions().can_create_agents);
    }

    #[test]
    fn test_agent_actor() {
        let permissions = AgentPermissions {
            can_create_agents: true,
            can_create_skills: false,
            trust_preset: TrustPreset::Medium,
            authorization_policy: AuthorizationPolicy::RequireApproval,
        };

        let agent = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            permissions: permissions.clone(),
        };

        assert!(agent.is_agent());
        assert!(!agent.is_board_admin());
        assert!(agent.agent_id().is_some());
        assert_eq!(agent.permissions().trust_preset, TrustPreset::Medium);
    }
}
