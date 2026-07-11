use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::permission::PermissionKey;

/// 授权决策类型 - 授权判定的完整结果
///
/// 核心流程：
/// 1. 收集上下文（Actor + Resource + Action）
/// 2. 执行授权检查（规则引擎 + 权限查询）
/// 3. 返回 AuthorizationDecision（allowed + reason + explanation）

/// 授权操作类型 - 需要检查权限的操作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthorizationAction {
    /// 基于PermissionKey的标准操作
    Permission { key: PermissionKey },

    /// Issue相关操作
    IssueRead { issue_id: Uuid },
    IssueWrite { issue_id: Uuid },
    IssueDelete { issue_id: Uuid },
    IssueAssign { issue_id: Uuid, assignee_id: Uuid },
    IssueMention { issue_id: Uuid, mentioned_agent_id: Uuid },

    /// Agent相关操作
    AgentCreate { company_id: Uuid },
    AgentRead { agent_id: Uuid },
    AgentUpdate { agent_id: Uuid },
    AgentDelete { agent_id: Uuid },
    AgentHire { company_id: Uuid },

    /// Company相关操作
    CompanyRead { company_id: Uuid },
    CompanyUpdate { company_id: Uuid },
    CompanyDelete { company_id: Uuid },

    /// Membership相关操作
    MembershipInvite { company_id: Uuid },
    MembershipApprove { company_id: Uuid, join_request_id: Uuid },
    MembershipRevoke { company_id: Uuid, membership_id: Uuid },

    /// Environment相关操作
    EnvironmentLease { company_id: Uuid },
    EnvironmentRelease { lease_id: Uuid },

    /// Routine相关操作
    RoutineCreate { company_id: Uuid },
    RoutineUpdate { routine_id: Uuid },
    RoutineDelete { routine_id: Uuid },
    RoutineTrigger { routine_id: Uuid },

    /// Goal相关操作
    GoalCreate { company_id: Uuid },
    GoalUpdate { goal_id: Uuid },
    GoalDelete { goal_id: Uuid },

    /// 自定义操作（扩展点）
    Custom { action: String, resource_id: Option<Uuid> },
}

impl AuthorizationAction {
    /// 获取操作涉及的公司ID
    pub fn company_id(&self) -> Option<Uuid> {
        match self {
            Self::Permission { .. } => None,
            Self::IssueRead { .. } | Self::IssueWrite { .. } | Self::IssueDelete { .. } => None,
            Self::IssueAssign { .. } | Self::IssueMention { .. } => None,
            Self::AgentCreate { company_id } | Self::AgentHire { company_id } => Some(*company_id),
            Self::AgentRead { .. } | Self::AgentUpdate { .. } | Self::AgentDelete { .. } => None,
            Self::CompanyRead { company_id } | Self::CompanyUpdate { company_id } | Self::CompanyDelete { company_id } => Some(*company_id),
            Self::MembershipInvite { company_id } | Self::MembershipApprove { company_id, .. } | Self::MembershipRevoke { company_id, .. } => Some(*company_id),
            Self::EnvironmentLease { company_id } => Some(*company_id),
            Self::EnvironmentRelease { .. } => None,
            Self::RoutineCreate { company_id } => Some(*company_id),
            Self::RoutineUpdate { .. } | Self::RoutineDelete { .. } | Self::RoutineTrigger { .. } => None,
            Self::GoalCreate { company_id } => Some(*company_id),
            Self::GoalUpdate { .. } | Self::GoalDelete { .. } => None,
            Self::Custom { .. } => None,
        }
    }

    /// 获取操作涉及的资源ID
    pub fn resource_id(&self) -> Option<Uuid> {
        match self {
            Self::Permission { .. } => None,
       Self::IssueRead { issue_id } | Self::IssueWrite { issue_id } | Self::IssueDelete { issue_id } => Some(*issue_id),
            Self::IssueAssign { issue_id, .. } | Self::IssueMention { issue_id, .. } => Some(*issue_id),
            Self::AgentRead { agent_id } | Self::AgentUpdate { agent_id } | Self::AgentDelete { agent_id } => Some(*agent_id),
            Self::CompanyRead { company_id } | Self::CompanyUpdate { company_id } | Self::CompanyDelete { company_id } => Some(*company_id),
            Self::EnvironmentRelease { lease_id } => Some(*lease_id),
            Self::RoutineUpdate { routine_id } | Self::RoutineDelete { routine_id } | Self::RoutineTrigger { routine_id } => Some(*routine_id),
            Self::GoalUpdate { goal_id } | Self::GoalDelete { goal_id } => Some(*goal_id),
            Self::Custom { resource_id, .. } => *resource_id,
            _ => None,
        }
    }

    /// 获取操作类型标识符（用于日志和审计）
    pub fn action_type(&self) -> &str {
        match self {
            Self::Permission { .. } => "permission",
            Self::IssueRead { .. } => "issue:read",
            Self::IssueWrite { .. } => "issue:write",
            Self::IssueDelete { .. } => "issue:delete",
            Self::IssueAssign { .. } => "issue:assign",
            Self::IssueMention { .. } => "issue:mention",
            Self::AgentCreate { .. } => "agent:create",
            Self::AgentRead { .. } => "agent:read",
            Self::AgentUpdate { .. } => "agent:update",
            Self::AgentDelete { .. } => "agent:delete",
       AgentHire { .. } => "agent:hire",
            Self::CompanyRead { .. } => "company:read",
            Self::CompanyUpdate { .. } => "company:update",
            Self::CompanyDelete { .. } => "company:delete",
            Self::MembershipInvite { .. } => "membership:invite",
            Self::MembershipApprove { .. } => "membership:approve",
            Self::MembershipRevoke { .. } => "membership:revoke",
            Self::EnvironmentLease { .. } => "environment:lease",
            Self::EnvironmentRelease { .. } => "environment:release",
            Self::RoutineCreate { .. } => "routine:create",
            Self::RoutineUpdate { .. } => "routine:update",
            Self::RoutineDelete { .. } => "routine:delete",
            Self::RoutineTrigger { .. } => "routine:trigger",
            Self::GoalCreate { .. } => "goal:create",
            Self::GoalUpdate { .. } => "goal:update",
            Self::GoalDelete { .. } => "goal:delete",
            Self::Custom { action, .. } => action,
        }
    }
}

/// 授权决策原因 - 允许/拒绝的具体理由
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionReason {
    // 允许类原因
    /// 实例管理员权限
    AllowInstanceAdmin,
    /// 公司所有者权限
    AllowCompanyOwner,
    /// 公司管理员权限
    AllowCompanyAdmin,
    /// 公司操作员权限
    AllowCompanyOperator,
    /// 显式权限授予
    AllowExplicitGrant { grant_id: Uuid },
    /// 资源所有者权限（如Agent操作自己的资源）
    AllowResourceOwner,
    /// Issue mention授予的临时权限
    AllowIssueMentionGrant { issue_id: Uuid },
    /// 本地隐式授权（单用户模式）
    AllowLocalImplicit,
    /// 公开资源（无需认证）
    AllowPublicResource,

    // 拒绝类原因
    /// 未认证
    DenyUnauthenticated,
    /// 缺少所需权限
    DenyMissingPermission { required: PermissionKey },
    /// 不属于该公司
    DenyNotCompanyMember { company_id: Uuid },
    /// 角色权限不足
    DenyInsufficientRole { required: String, actual: String },
    /// 跨公司访问被拒绝
    DenyCrossCompanyAccess { resource_company: Uuid, actor_company: Uuid },
    /// 资源不存在
    DenyResourceNotFound { resource_type: String, resource_id: Uuid },
    /// API密钥范围限制
    DenyApiKeyScopeRestriction { reason: String },
    /// 低信任边界（Agent访问敏感资源）
    DenyLowTrustBoundary { reason: String },
    /// 预算超限
    DenyBudgetExceeded { agent_id: Uuid, spent: i32, limit: i32 },
    /// 资源配额耗尽
    DenyQuotaExhausted { quota_type: String, limit: u32 },
    /// 自定义拒绝理由
    DenyCustom { reason: String },
}

impl DecisionReason {
    /// 是否为允许类原因
    pub fn is_allow(&self) -> bool {
        matches!(
            self,
            Self::AllowInstanceAdmin
                | Self::AllowCompanyOwner
                | Self::AllowCompanyAdmin
                | Self::AllowCompanyOperator
                | Self::AllowExplicitGrant { .. }
                | Self::AllowResourceOwner
                | Self::AllowIssueMentionGrant { .. }
                | Self::AllowLocalImplicit
                | Self::AllowPublicResource
        )
    }

    /// 是否为拒绝类原因
    pub fn is_deny(&self) -> bool {
        !self.is_allow()
    }

    /// 获取HTTP状态码建议
    pub fn suggested_status_code(&self) -> u16 {
        match self {
            Self::DenyUnauthenticated => 401,
            Self::DenyMissingPermission { .. }
            | Self::DenyNotCompanyMember { .. }
            | Self::DenyInsufficientRole { .. }
            | Self::DenyCrossCompanyAccess { .. }
            | Self::DenyApiKeyScopeRestriction { .. }
            | Self::DenyLowTrustBoundary { .. }
            | Self::DenyBudgetExceeded { .. }
            | Self::DenyQuotaExhausted { .. } => 403,
            Self::DenyResourceNotFound { .. } => 404,
            Self::DenyCustom { .. } => 403,
            _ => 200,
        }
    }
}

/// 授权决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationDecision {
    /// 是否允许操作
    pub allowed: bool,
    /// 被检查的操作
    pub action: AuthorizationAction,
    /// 决策原因
    pub reason: DecisionReason,
    /// 人类可读的解释
    pub explanation: String,
    /// 错误代码（用于客户端国际化）
    pub code: Option<String>,
    /// 关联的权限授予记录ID（如果通过显式授权）
    pub grant_id: Option<Uuid>,
}

impl AuthorizationDecision {
    /// 创建允许决策
    pub fn allow(action: AuthorizationAction, reason: DecisionReason, explanation: String) -> Self {
        let grant_id = match &reason {
            DecisionReason::AllowExplicitGrant { grant_id } => Some(*grant_id),
            _ => None,
        };

        Self {
            allowed: true,
            action,
            reason,
            explanation,
            code: None,
            grant_id,
        }
    }

    /// 创建拒绝决策
    pub fn deny(action: AuthorizationAction, reason: DecisionReason, explanation: String) -> Self {
        Self {
            allowed: false,
            action,
            reason,
            explanation,
            code: Some(Self::reason_to_code(&reason)),
            grant_id: None,
        }
    }

    /// 设置错误代码
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// 将决策原因转换为错误代码
    fn reason_to_code(reason: &DecisionReason) -> String {
        match reason {
            DecisionReason::DenyUnauthenticated => "auth.unauthenticated",
            DecisionReason::DenyMissingPermission { .. } => "auth.missing_permission",
            DecisionReason::DenyNotCompanyMember { .. } => "auth.not_company_member",
            DecisionReason::DenyInsufficientRole { .. } => "auth.insufficient_role",
            DecisionReason::DenyCrossCompanyAccess { .. } => "auth.cross_company_access",
            DecisionReason::DenyResourceNotFound { .. } => "auth.resource_not_found",
            DecisionReason::DenyApiKeyScopeRestriction { .. } => "auth.api_key_scope",
            DecisionReason::DenyLowTrustBoundary { .. } => "auth.low_trust_boundary",
            DecisionReason::DenyBudgetExceeded { .. } => "auth.budget_exceeded",
            DecisionReason::DenyQuotaExhausted { .. } => "auth.quota_exhausted",
            DecisionReason::DenyCustom { .. } => "auth.custom_deny",
            _ => "auth.unknown",
        }
        .to_string()
    }

    /// 获取建议的HTTP状态码
    pub fn status_code(&self) -> u16 {
        if self.allowed {
            200
        } else {
            self.reason.suggested_status_code()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_action_company_id() {
        let company_id = Uuid::new_v4();
        let action = AuthorizationAction::AgentCreate { company_id };
        assert_eq!(action.company_id(), Some(company_id));
    }

    #[test]
    fn test_authorization_action_resource_id() {
        let issue_id = Uuid::new_v4();
        let action = AuthorizationAction::IssueRead { issue_id };
        assert_eq!(action.resource_id(), Some(issue_id));
    }

    #[test]
    fn test_decision_reason_is_allow() {
        assert!(DecisionReason::AllowCompanyOwner.is_allow());
        assert!(!DecisionReason::DenyUnauthenticated.is_allow());
    }

    #[test]
    fn test_decision_reason_status_code() {
        assert_eq!(DecisionReason::DenyUnauthenticated.suggested_status_code(), 401);
        assert_eq!(DecisionReason::DenyMissingPermission { required: PermissionKey::new("test") }.suggested_status_code(), 403);
        assert_eq!(DecisionReason::AllowCompanyOwner.suggested_status_code(), 200);
    }

    #[test]
    fn test_authorization_decision_allow() {
        let action = AuthorizationAction::IssueRead { issue_id: Uuid::new_v4() };
        let decision = AuthorizationDecision::allow(
            action,
            DecisionReason::AllowCompanyOwner,
            "User is company owner".to_string(),
        );

        assert!(decision.allowed);
        assert_eq!(decision.status_code(), 200);
        assert!(decision.code.is_none());
    }

    #[test]
    fn test_authorization_decision_deny() {
        let action = AuthorizationAction::IssueRead { issue_id: Uuid::new_v4() };
        let decision = AuthorizationDecision::deny(
            action,
            DecisionReason::DenyUnauthenticated,
            "User not authenticated".to_string(),
        );

        assert!(!decision.allowed);
        assert_eq!(decision.status_code(), 401);
        assert_eq!(decision.code, Some("auth.unauthenticated".to_string()));
    }
}
