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

    // ---- Issue/Case 访问控制 ----

    /// 决定 Issue 访问权限（含 watchdog scope 检查）
    async fn decide_issue_access(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
        action: IssueAction,
    ) -> AccessDecision;

    /// 断言 Agent 对 Issue 的变更权限
    async fn assert_agent_issue_mutation_allowed(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
    ) -> Result<(), AccessError>;

    /// 断言 Agent 对 Issue 的评论权限
    async fn assert_agent_issue_comment_allowed(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
    ) -> Result<(), AccessError>;

    /// 过滤 Actor 可见的 Issue 列表
    /// Uses Vec<Box<dyn IssueLike + Send>> for dyn compatibility
    async fn filter_issues_for_actor(
        &self,
        actor: &dyn Actor,
        issues: Vec<Box<dyn IssueLike + Send>>,
    ) -> Vec<Box<dyn IssueLike + Send>>;

    /// 断言 Cases 功能已启用
    async fn assert_cases_enabled(&self, company_id: Uuid) -> Result<(), AccessError>;

    /// 断言项目属于公司
    async fn assert_project_belongs_to_company(
        &self,
        project_id: Uuid,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    /// 断言 Board 操作权限（树形控制、强制释放等）
    async fn assert_board(&self, actor: &dyn Actor) -> Result<(), AccessError>;
}

/// Issue access info used for access decisions
#[derive(Debug, Clone)]
pub struct IssueAccessInfo {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub status: String,
}

/// Issue action types for access control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueAction {
    Read,
    Comment,
    Mutate,
}

/// Trait for types that can be used in issue filtering
pub trait IssueLike {
    fn issue_id(&self) -> Uuid;
    fn issue_company_id(&self) -> Uuid;
    fn issue_project_id(&self) -> Option<Uuid>;
    fn issue_parent_id(&self) -> Option<Uuid>;
    fn issue_assignee_agent_id(&self) -> Option<Uuid>;
    fn issue_assignee_user_id(&self) -> Option<Uuid>;
    fn issue_status(&self) -> &str;
}

// Implement IssueLike for IssueAccessInfo itself
impl IssueLike for IssueAccessInfo {
    fn issue_id(&self) -> Uuid { self.id }
    fn issue_company_id(&self) -> Uuid { self.company_id }
    fn issue_project_id(&self) -> Option<Uuid> { self.project_id }
    fn issue_parent_id(&self) -> Option<Uuid> { self.parent_id }
    fn issue_assignee_agent_id(&self) -> Option<Uuid> { self.assignee_agent_id }
    fn issue_assignee_user_id(&self) -> Option<Uuid> { self.assignee_user_id }
    fn issue_status(&self) -> &str { &self.status }
}

/// Resource - 资源信息
#[derive(Debug, Clone)]
pub struct Resource {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub company_id: Uuid,
    /// Optional Issue-specific context for fine-grained access decisions
    pub issue_context: Option<IssueResourceContext>,
}

/// Issue-specific resource context for fine-grained access decisions
#[derive(Debug, Clone)]
pub struct IssueResourceContext {
    pub project_id: Option<Uuid>,
    pub parent_issue_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub status: Option<String>,
}

/// ResourceType - 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Agent,
    Company,
    Task,
    BuiltInAgent,
    Issue,
    Case,
    IssueDocument,
    IssueComment,
    CaseDocument,
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

    async fn decide_issue_access(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
        action: IssueAction,
    ) -> AccessDecision {
        // 1. 公司级隔离
        if actor.company_id() != Some(issue.company_id) {
            return AccessDecision::deny("Cross-company issue access not allowed");
        }

        // 2. 如果是被分配人，允许访问
        let is_assignee = match actor.agent_id() {
            Some(agent_id) => issue.assignee_agent_id == Some(agent_id),
            None => false,
        };
        if is_assignee {
            return AccessDecision::allow("Assignee access granted");
        }

        // 3. 如果是 Admin 用户，允许访问
        if !actor.is_agent() {
            // 非 Agent 用户（Board/Admin）有完整访问权限
            return AccessDecision::allow("User access granted");
        }

        // 4. Agent 访问：检查权限配置
        match action {
            IssueAction::Read => {
                if actor.has_permission(Action::IssueRead) {
                    AccessDecision::allow("Agent has issue:read permission")
                } else {
                    // 根据 mention grant 或其他机制判断
                    AccessDecision::deny("Agent lacks issue:read permission for this issue")
                }
            }
            IssueAction::Comment => {
                if actor.has_permission(Action::IssueComment) {
                    AccessDecision::allow("Agent has issue:comment permission")
                } else {
                    AccessDecision::deny("Agent lacks issue:comment permission for this issue")
                }
            }
            IssueAction::Mutate => {
                if actor.has_permission(Action::IssueMutate) {
                    AccessDecision::allow("Agent has issue:mutate permission")
                } else {
                    AccessDecision::deny("Agent lacks issue:mutate permission for this issue")
                }
            }
        }
    }

    async fn assert_agent_issue_mutation_allowed(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
    ) -> Result<(), AccessError> {
        if !actor.is_agent() {
            return Ok(());
        }
        let decision = self.decide_issue_access(actor, issue, IssueAction::Mutate).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::Denied(format!(
                "Agent issue mutation denied: {}",
                decision.reason
            )))
        }
    }

    async fn assert_agent_issue_comment_allowed(
        &self,
        actor: &dyn Actor,
        issue: &IssueAccessInfo,
    ) -> Result<(), AccessError> {
        if !actor.is_agent() {
            return Ok(());
        }
        let decision = self.decide_issue_access(actor, issue, IssueAction::Comment).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::Denied(format!(
                "Agent issue comment denied: {}",
                decision.reason
            )))
        }
    }

    async fn filter_issues_for_actor(
        &self,
        actor: &dyn Actor,
        issues: Vec<Box<dyn IssueLike + Send>>,
    ) -> Vec<Box<dyn IssueLike + Send>> {
        let mut visible = Vec::new();
        for issue in issues {
            let info = IssueAccessInfo {
                id: issue.issue_id(),
                company_id: issue.issue_company_id(),
                project_id: issue.issue_project_id(),
                parent_id: issue.issue_parent_id(),
                assignee_agent_id: issue.issue_assignee_agent_id(),
                assignee_user_id: issue.issue_assignee_user_id(),
                status: issue.issue_status().to_string(),
            };
            let decision = self.decide_issue_access(actor, &info, IssueAction::Read).await;
            if decision.allowed {
                visible.push(issue);
            }
        }
        visible
    }

    async fn assert_cases_enabled(&self, company_id: Uuid) -> Result<(), AccessError> {
        let _ = company_id;
        // TODO: 查询公司配置检查 Cases 功能是否启用
        Ok(())
    }

    async fn assert_project_belongs_to_company(
        &self,
        project_id: Uuid,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        let _ = (project_id, company_id);
        // TODO: 查询数据库验证 project.company_id == company_id
        // 这里需要 ProjectRepository，简化实现中假设验证通过
        Ok(())
    }

    async fn assert_board(&self, actor: &dyn Actor) -> Result<(), AccessError> {
        // Board 权限：非 Agent 用户有 Board 权限
        if !actor.is_agent() {
            return Ok(());
        }
        // Agent 需要检查 board 权限
        if actor.has_permission(Action::BoardForceRelease) {
            return Ok(());
        }
        Err(AccessError::Denied("Board access required".to_string()))
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
