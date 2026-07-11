use async_trait::async_trait;
use uuid::Uuid;

use super::models::{AccessDecision, Action, Actor};

/// AccessService trait - 定义访问控制服务接口
#[async_trait]
pub trait AccessService: Send + Sync {
    /// 进行访问决策
    async fn decide(
        &self,
        action: Action,
        actor: &dyn Actor,
        resource: Option<&Resource>,
    ) -> AccessDecision;

    /// 断言公司访问权限
    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError>;

    /// 断言 Agent 读取权限
    async fn assert_agent_read_allowed(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError>;

    /// 断言可以为公司创建 Agent
    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    /// 断言可以更新 Agent
    async fn assert_can_update_agent(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError>;

    /// 断言可以读取配置
    async fn assert_can_read_configurations(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError>;

    /// 断言可以配置内置 Agent
    async fn assert_can_provision_built_in_agents(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError>;

    /// 断言可以控制内置 Routine
    async fn assert_can_control_built_in_routine(&self, actor: &dyn Actor, routine_key: &str) -> Result<(), AccessError>;

    /// 断言内置 Agent 功能已启用
    async fn assert_built_in_agents_enabled(&self, company_id: Uuid) -> Result<(), AccessError>;
}

/// Resource - 资源信息
#[derive(Debug, Clone)]
pub struct Resource {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub company_id: Uuid,
}

/// ResourceType - 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Agent,
    Company,
    Task,
    BuiltInAgent,
}

/// AccessError - 访问控制错误
#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("Access denied: {0}")]
    Denied(String),

    #[error("Insufficient permissions: {0}")]
    InsufficientPermissions(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),
}

/// DefaultAccessService - AccessService 的默认实现
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
        action: Action,
        actor: &dyn Actor,
        resource: Option<&Resource>,
    ) -> AccessDecision {
        // 基本的访问决策逻辑
        match action {
            Action::CompanyRead => {
                if let Some(res) = resource {
                    if actor.company_id() == Some(res.company_id) {
                        return AccessDecision::allow("Same company access");
                    }
                }
                AccessDecision::deny("Cross-company access not allowed")
            }
            Action::AgentsCreate => {
                if actor.has_permission(action) {
                    AccessDecision::allow("Has agents:create permission")
                } else {
                    AccessDecision::deny("Missing agents:create permission")
                }
            }
            Action::AgentRead => {
                if let Some(res) = resource {
                    if actor.company_id() == Some(res.company_id) {
                        return AccessDecision::allow("Same company agent access");
                    }
                }
                AccessDecision::deny("Cannot read agent from different company")
            }
            _ => AccessDecision::deny("Action not implemented"),
        }
    }

    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError> {
        if actor.company_id() == Some(company_id) {
            Ok(())
        } else {
            Err(AccessError::Denied("Cross-company access not allowed".to_string()))
        }
    }

    async fn assert_agent_read_allowed(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError> {
        // 简化实现：需要访问数据库查询 agent 的 company_id
        // 这里假设调用方已经验证了 company_id
        let _ = agent_id;
        if actor.company_id().is_some() {
            Ok(())
        } else {
            Err(AccessError::Denied("No company context".to_string()))
        }
    }

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        // 1. 验证公司访问权限
        self.assert_company_access(actor, company_id).await?;

        // 2. 验证 agents:create 权限
        if !actor.has_permission(Action::AgentsCreate) {
            return Err(AccessError::InsufficientPermissions(
                "Missing agents:create permission".to_string(),
            ));
        }

        // 3. 如果是 Agent，验证同公司
        if actor.is_agent() && actor.company_id() != Some(company_id) {
            return Err(AccessError::Denied("Agent can only create agents in its own company".to_string()));
        }

        Ok(())
    }

    async fn assert_can_update_agent(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError> {
        // 简化实现：需要查询 agent 的详细信息
        let _ = agent_id;
        if actor.company_id().is_some() {
            Ok(())
        } else {
            Err(AccessError::Denied("No company context".to_string()))
        }
    }

    async fn assert_can_read_configurations(&self, actor: &dyn Actor, agent_id: Uuid) -> Result<(), AccessError> {
        // 简化实现
        let _ = agent_id;
        if actor.company_id().is_some() {
            Ok(())
        } else {
            Err(AccessError::Denied("No company context".to_string()))
        }
    }

    async fn assert_can_provision_built_in_agents(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;
        // TODO: 添加额外的权限检查
        Ok(())
    }

    async fn assert_can_control_built_in_routine(&self, actor: &dyn Actor, routine_key: &str) -> Result<(), AccessError> {
        let _ = routine_key;
        if actor.company_id().is_some() {
            Ok(())
        } else {
            Err(AccessError::Denied("No company context".to_string()))
        }
    }

    async fn assert_built_in_agents_enabled(&self, company_id: Uuid) -> Result<(), AccessError> {
        // TODO: 查询公司配置检查实验特性是否启用
        let _ = company_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{UserActor, AgentActor};

    #[tokio::test]
    async fn test_company_access() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_admin: false,
        };

        assert!(service.assert_company_access(&user, company_id).await.is_ok());
        assert!(service.assert_company_access(&user, Uuid::new_v4()).await.is_err());
    }

    #[tokio::test]
    async fn test_create_agents_permission() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let agent_with_perm = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id,
            permissions: serde_json::json!({"can_create_agents": true}),
        };

        let agent_without_perm = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id,
            permissions: serde_json::json!({"can_create_agents": false}),
        };

        assert!(service.assert_can_create_agents_for_company(&agent_with_perm, company_id).await.is_ok());
        assert!(service.assert_can_create_agents_for_company(&agent_without_perm, company_id).await.is_err());
    }
}
