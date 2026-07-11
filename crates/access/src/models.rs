use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Action 枚举 - 定义所有可能的权限操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // Agent 相关
    AgentsCreate,
    AgentRead,
    AgentConfigUpdate,
    AgentConfigRead,
    AgentsDelete,

    // Task 相关
    TasksAssign,
    TasksRead,

    // Built-in Agent 相关
    BuiltInAgentsProvision,
    BuiltInRoutineControl,

    // Company 相关
    CompanyRead,
    CompanyUpdate,
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::AgentsCreate => write!(f, "agents:create"),
            Action::AgentRead => write!(f, "agent:read"),
            Action::AgentConfigUpdate => write!(f, "agent_config:update"),
            Action::AgentConfigRead => write!(f, "agent_config:read"),
            Action::AgentsDelete => write!(f, "agents:delete"),
            Action::TasksAssign => write!(f, "tasks:assign"),
            Action::TasksRead => write!(f, "tasks:read"),
            Action::BuiltInAgentsProvision => write!(f, "built_in_agents:provision"),
            Action::BuiltInRoutineControl => write!(f, "built_in_routine:control"),
            Action::CompanyRead => write!(f, "company:read"),
            Action::CompanyUpdate => write!(f, "company:update"),
        }
    }
}

/// AccessDecision - 访问决策结果
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

/// Actor trait - 定义访问主体的通用接口
pub trait Actor: Send + Sync {
    /// 获取所属公司ID
    fn company_id(&self) -> Option<Uuid>;

    /// 是否是 Agent
    fn is_agent(&self) -> bool;

    /// 获取 Agent ID（如果是 Agent）
    fn agent_id(&self) -> Option<Uuid>;

    /// 获取权限配置
    fn permissions(&self) -> Option<&serde_json::Value>;

    /// 检查是否有特定权限
    fn has_permission(&self, action: Action) -> bool {
        if let Some(perms) = self.permissions() {
            match action {
                Action::AgentsCreate => perms
                    .get("can_create_agents")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                Action::TasksAssign => perms
                    .get("can_assign_tasks")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                _ => false,
            }
        } else {
            false
        }
    }
}

/// UserActor - 用户访问主体
#[derive(Debug, Clone)]
pub struct UserActor {
    pub user_id: Uuid,
    pub company_id: Uuid,
    pub is_admin: bool,
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

    fn permissions(&self) -> Option<&serde_json::Value> {
        None
    }
}

/// AgentActor - Agent 访问主体
#[derive(Debug, Clone)]
pub struct AgentActor {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub permissions: serde_json::Value,
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

    fn permissions(&self) -> Option<&serde_json::Value> {
        Some(&self.permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_decision() {
        let allow = AccessDecision::allow("User is admin");
        assert!(allow.allowed);

        let deny = AccessDecision::deny("Insufficient permissions");
        assert!(!deny.allowed);
    }

    #[test]
    fn test_user_actor() {
        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_admin: true,
        };

        assert!(!user.is_agent());
        assert_eq!(user.agent_id(), None);
        assert!(user.company_id().is_some());
    }

    #[test]
    fn test_agent_actor() {
        let agent = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            permissions: serde_json::json!({"can_create_agents": true}),
        };

        assert!(agent.is_agent());
        assert!(agent.agent_id().is_some());
        assert!(agent.has_permission(Action::AgentsCreate));
        assert!(!agent.has_permission(Action::AgentConfigUpdate));
    }
}
