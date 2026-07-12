use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::membership::{CompanyMembership, MembershipRole};

/// Actor 类型系统 - 授权主体抽象
///
/// 核心概念：
/// - AuthorizationActor: 执行操作的主体（Board用户、Agent、None匿名）
/// - ActorSource: 主体身份来源（会话token、API密钥、JWT等）
/// - AgentApiKeyScope: Agent API密钥的权限范围限定

/// 授权主体类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthorizationActor {
    /// Board 用户主体
    Board {
        user_id: Uuid,
        company_id: Uuid,
        /// 身份来源（会话/API Key/本地隐式等）
        source: ActorSource,
        /// 该用户在当前解析上下文中的公司成员关系（用于角色级权限检查）
        memberships: Vec<CompanyMembership>,
        /// 是否为实例管理员（跨公司全局权限）
        is_instance_admin: bool,
    },
    /// Agent 主体
    Agent {
        agent_id: Uuid,
        company_id: Uuid,
        /// Agent所属的运行时上下文（可选）
        run_id: Option<Uuid>,
        /// 身份来源（API Key/JWT等）
        source: ActorSource,
    },
    /// 匿名/未认证主体
    None,
}

impl AuthorizationActor {
    /// 创建 Board 用户主体（无成员关系/非实例管理员）
    pub fn board(user_id: Uuid, company_id: Uuid) -> Self {
        Self::Board {
            user_id,
            company_id,
            source: ActorSource::LocalImplicit,
            memberships: Vec::new(),
            is_instance_admin: false,
        }
    }

    /// 创建带成员关系与实例管理员标记的 Board 主体
    pub fn board_with_memberships(
        user_id: Uuid,
        company_id: Uuid,
        memberships: Vec<CompanyMembership>,
        is_instance_admin: bool,
    ) -> Self {
        Self::Board {
            user_id,
            company_id,
            source: ActorSource::LocalImplicit,
            memberships,
            is_instance_admin,
        }
    }

    /// 创建带来源的 Board 用户主体
    pub fn board_with_source(
        user_id: Uuid,
        company_id: Uuid,
        source: ActorSource,
        memberships: Vec<CompanyMembership>,
        is_instance_admin: bool,
    ) -> Self {
        Self::Board {
            user_id,
            company_id,
            source,
            memberships,
            is_instance_admin,
        }
    }

    /// 创建 Agent 主体
    pub fn agent(agent_id: Uuid, company_id: Uuid, run_id: Option<Uuid>) -> Self {
        Self::Agent {
            agent_id,
            company_id,
            run_id,
            source: ActorSource::AgentJwt,
        }
    }

    /// 创建带来源的 Agent 主体
    pub fn agent_with_source(
        agent_id: Uuid,
        company_id: Uuid,
        run_id: Option<Uuid>,
        source: ActorSource,
    ) -> Self {
        Self::Agent {
            agent_id,
            company_id,
            run_id,
            source,
        }
    }

    /// 创建匿名主体
    pub fn none() -> Self {
        Self::None
    }

    /// 获取公司ID（如果存在）
    pub fn company_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { company_id, .. } | Self::Agent { company_id, .. } => Some(*company_id),
            Self::None => None,
        }
    }

    /// 是否为实例管理员（仅 Board 用户可持有）
    pub fn is_instance_admin(&self) -> bool {
        matches!(self, Self::Board { is_instance_admin, .. } if *is_instance_admin)
    }

    /// 查找该 Actor 在指定公司的活跃成员角色
    pub fn role_in(&self, company_id: Uuid) -> Option<MembershipRole> {
        match self {
            Self::Board { memberships, .. } => memberships
                .iter()
                .find(|m| m.company_id == company_id && m.status.is_active())
                .map(|m| m.role),
            _ => None,
        }
    }

    /// 获取主体ID（Board返回user_id，Agent返回agent_id）
    pub fn principal_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { user_id, .. } => Some(*user_id),
            Self::Agent { agent_id, .. } => Some(*agent_id),
            Self::None => None,
        }
    }

    /// 是否为Board用户
    pub fn is_board(&self) -> bool {
        matches!(self, Self::Board { .. })
    }

    /// 是否为Agent
    pub fn is_agent(&self) -> bool {
        matches!(self, Self::Agent { .. })
    }

    /// 是否为匿名主体
    pub fn is_anonymous(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// 主体身份来源 - 标识Actor的认证方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorSource {
    /// 本地隐式认证（单用户模式下的默认身份）
    LocalImplicit,
    /// 会话token认证（Board用户登录后的session）
    Session,
    /// Board API密钥认证
    BoardKey,
    /// Agent API密钥认证
    AgentKey,
    /// Agent JWT认证（短期临时token）
    AgentJwt,
    /// 云租户认证（多租户SaaS模式）
    CloudTenant,
    /// 无认证（匿名访问）
    None,
}

impl ActorSource {
    /// 是否为API密钥类认证
    pub fn is_api_key(&self) -> bool {
        matches!(self, Self::BoardKey | Self::AgentKey)
    }

    /// 是否为会话类认证（需要CSRF保护）
    pub fn is_session_based(&self) -> bool {
        matches!(self, Self::Session | Self::LocalImplicit)
    }

    /// 是否为临时令牌认证
    pub fn is_ephemeral(&self) -> bool {
        matches!(self, Self::AgentJwt)
    }
}

/// Agent API密钥权限范围
///
/// 限制Agent API密钥的使用范围，防止密钥泄露后的权限滥用
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentApiKeyScope {
    /// 密钥所属Agent ID
    pub agent_id: Uuid,
    /// 密钥所属公司ID
    pub company_id: Uuid,
    /// 允许的Issue范围（None表示允许所有Issue）
    pub allowed_issue_ids: Option<Vec<Uuid>>,
    /// 允许的操作列表（None表示允许所有操作）
    pub allowed_actions: Option<Vec<String>>,
    /// 是否允许读取敏感数据（如其他Agent的配置）
    pub allow_sensitive_read: bool,
    /// 密钥创建时间
    pub created_at: DateTime<Utc>,
    /// 密钥过期时间（None表示永不过期）
    pub expires_at: Option<DateTime<Utc>>,
}

impl AgentApiKeyScope {
    /// 创建新的AgentApiKeyScope
    pub fn new(agent_id: Uuid, company_id: Uuid) -> Self {
        Self {
            agent_id,
            company_id,
            allowed_issue_ids: None,
            allowed_actions: None,
            allow_sensitive_read: false,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    /// 设置允许的Issue范围
    pub fn with_issue_ids(mut self, issue_ids: Vec<Uuid>) -> Self {
        self.allowed_issue_ids = Some(issue_ids);
        self
    }

    /// 设置允许的操作列表
    pub fn with_actions(mut self, actions: Vec<String>) -> Self {
        self.allowed_actions = Some(actions);
        self
    }

    /// 允许读取敏感数据
    pub fn with_sensitive_read(mut self, allow: bool) -> Self {
        self.allow_sensitive_read = allow;
        self
    }

    /// 设置过期时间
    pub fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 检查密钥是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// 检查是否允许访问指定Issue
    pub fn can_access_issue(&self, issue_id: Uuid) -> bool {
        match &self.allowed_issue_ids {
            None => true, // 允许所有Issue
            Some(ids) => ids.contains(&issue_id),
        }
    }

    /// 检查是否允许执行指定操作
    pub fn can_perform_action(&self, action: &str) -> bool {
        match &self.allowed_actions {
            None => true, // 允许所有操作
            Some(actions) => actions.contains(&action.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_actor_board() {
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let actor = AuthorizationActor::board(user_id, company_id);

        assert!(actor.is_board());
        assert!(!actor.is_agent());
        assert!(!actor.is_anonymous());
        assert_eq!(actor.company_id(), Some(company_id));
        assert_eq!(actor.principal_id(), Some(user_id));
    }

    #[test]
    fn test_authorization_actor_agent() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let run_id = Some(Uuid::new_v4());
        let actor = AuthorizationActor::agent(agent_id, company_id, run_id);

        assert!(!actor.is_board());
        assert!(actor.is_agent());
        assert!(!actor.is_anonymous());
        assert_eq!(actor.company_id(), Some(company_id));
        assert_eq!(actor.principal_id(), Some(agent_id));
    }

    #[test]
    fn test_authorization_actor_none() {
        let actor = AuthorizationActor::none();

        assert!(!actor.is_board());
        assert!(!actor.is_agent());
        assert!(actor.is_anonymous());
        assert_eq!(actor.company_id(), None);
        assert_eq!(actor.principal_id(), None);
    }

    #[test]
    fn test_actor_source_api_key() {
        assert!(ActorSource::BoardKey.is_api_key());
        assert!(ActorSource::AgentKey.is_api_key());
        assert!(!ActorSource::Session.is_api_key());
    }

    #[test]
    fn test_actor_source_session_based() {
        assert!(ActorSource::Session.is_session_based());
        assert!(ActorSource::LocalImplicit.is_session_based());
        assert!(!ActorSource::AgentJwt.is_session_based());
    }

    #[test]
    fn test_agent_api_key_scope_issue_access() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let issue1 = Uuid::new_v4();
        let issue2 = Uuid::new_v4();

        let scope = AgentApiKeyScope::new(agent_id, company_id)
            .with_issue_ids(vec![issue1]);

        assert!(scope.can_access_issue(issue1));
        assert!(!scope.can_access_issue(issue2));
    }

    #[test]
    fn test_agent_api_key_scope_action_access() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let scope = AgentApiKeyScope::new(agent_id, company_id)
            .with_actions(vec!["read".to_string(), "write".to_string()]);

        assert!(scope.can_perform_action("read"));
        assert!(scope.can_perform_action("write"));
        assert!(!scope.can_perform_action("delete"));
    }

    #[test]
    fn test_agent_api_key_scope_expiration() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let expired_time = Utc::now() - chrono::Duration::hours(1);
        let scope = AgentApiKeyScope::new(agent_id, company_id)
            .with_expiration(expired_time);

        assert!(scope.is_expired());

        let future_time = Utc::now() + chrono::Duration::hours(1);
        let scope2 = AgentApiKeyScope::new(agent_id, company_id)
            .with_expiration(future_time);

        assert!(!scope2.is_expired());
    }
}
