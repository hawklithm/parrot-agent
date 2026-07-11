use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::membership::PrincipalType;

/// 权限键 - 标识特定权限的唯一标识符
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PermissionKey(String);

impl PermissionKey {
    /// 创建新的权限键
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// 获取权限键字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }

    // 公司级权限常量
    pub const USERS_INVITE: &'static str = "users:invite";
    pub const JOINS_APPROVE: &'static str = "joins:approve";
    pub const COMPANY_SETTINGS_UPDATE: &'static str = "company:settings:update";
    pub const COMPANY_DELETE: &'static str = "company:delete";
    pub const MEMBERS_MANAGE: &'static str = "members:manage";
    pub const ROLES_ASSIGN: &'static str = "roles:assign";

    // 项目级权限常量
    pub const ISSUES_READ: &'static str = "issues:read";
    pub const ISSUES_WRITE: &'static str = "issues:write";
    pub const ISSUES_DELETE: &'static str = "issues:delete";
    pub const ISSUES_ASSIGN: &'static str = "issues:assign";

    // Agent特定权限常量
    pub const AGENTS_CREATE: &'static str = "agents:create";
    pub const AGENTS_UPDATE: &'static str = "agents:update";
    pub const AGENTS_DELETE: &'static str = "agents:delete";
    pub const AGENTS_HIRE: &'static str = "agents:hire";
    pub const TASKS_ASSIGN: &'static str = "tasks:assign";

    // Environment权限常量
    pub const ENVIRONMENTS_LEASE: &'static str = "environments:lease";
    pub const ENVIRONMENTS_RELEASE: &'static str = "environments:release";
    pub const ENVIRONMENTS_MANAGE: &'static str = "environments:manage";

    // Routine权限常量
    pub const ROUTINES_CREATE: &'static str = "routines:create";
    pub const ROUTINES_UPDATE: &'static str = "routines:update";
    pub const ROUTINES_DELETE: &'static str = "routines:delete";
    pub const ROUTINES_TRIGGER: &'static str = "routines:trigger";

    // Goal权限常量
    pub const GOALS_CREATE: &'static str = "goals:create";
    pub const GOALS_UPDATE: &'static str = "goals:update";
    pub const GOALS_DELETE: &'static str = "goals:delete";

    /// 从常量创建权限键
    pub fn from_const(key: &'static str) -> Self {
        Self(key.to_string())
    }

    /// 检查是否为公司级权限
    pub fn is_company_level(&self) -> bool {
        self.0.starts_with("company:") || self.0.starts_with("members:") || self.0.starts_with("roles:")
    }

    /// 检查是否为资源级权限
    pub fn is_resource_level(&self) -> bool {
        !self.is_company_level()
    }
}

impl fmt::Display for PermissionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for PermissionKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for PermissionKey {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// 权限授予范围 - JSON包装器，用于灵活定义权限适用范围
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantScope(JsonValue);

impl GrantScope {
    /// 创建新的授予范围
    pub fn new(scope: JsonValue) -> Self {
        Self(scope)
    }

    /// 创建全局范围（适用于整个公司）
    pub fn global() -> Self {
        Self(serde_json::json!({"type": "global"}))
    }

    /// 创建项目范围
    pub fn project(project_id: Uuid) -> Self {
        Self(serde_json::json!({
            "type": "project",
            "project_id": project_id.to_string()
        }))
    }

    /// 创建Issue范围
    pub fn issue(issue_id: Uuid) -> Self {
        Self(serde_json::json!({
            "type": "issue",
            "issue_id": issue_id.to_string()
        }))
    }

    /// 创建Agent范围
    pub fn agent(agent_id: Uuid) -> Self {
        Self(serde_json::json!({
            "type": "agent",
            "agent_id": agent_id.to_string()
        }))
    }

    /// 创建自定义范围
    pub fn custom(scope: JsonValue) -> Self {
        Self(scope)
    }

    /// 获取范围类型
    pub fn scope_type(&self) -> Option<&str> {
        self.0.get("type").and_then(|v| v.as_str())
    }

    /// 获取原始JSON值
    pub fn as_json(&self) -> &JsonValue {
        &self.0
    }

    /// 检查范围是否匹配指定资源
    pub fn matches_resource(&self, resource_type: &str, resource_id: Uuid) -> bool {
        match self.scope_type() {
            Some("global") => true,
            Some(scope_type) if scope_type == resource_type => {
                let id_key = format!("{}_id", resource_type);
                self.0.get(&id_key)
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .map(|id| id == resource_id)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }
}

/// 权限授予记录 - 显式授予某个主体特定权限
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: PrincipalType,
    pub principal_id: Uuid,
    pub permission_key: PermissionKey,
    pub scope: GrantScope,
    pub granted_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl PermissionGrant {
    /// 创建新的权限授予
    pub fn new(
        company_id: Uuid,
        principal_type: PrincipalType,
        principal_id: Uuid,
        permission_key: PermissionKey,
        scope: GrantScope,
        granted_by_user_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            permission_key,
            scope,
            granted_by_user_id,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    /// 设置过期时间
    pub fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 检查授予是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// 检查授予是否适用于指定资源
    pub fn applies_to_resource(&self, resource_type: &str, resource_id: Uuid) -> bool {
        !self.is_expired() && self.scope.matches_resource(resource_type, resource_id)
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_key_constants() {
        let key = PermissionKey::from_const(PermissionKey::ISSUES_READ);
        assert_eq!(key.as_str(), "issues:read");
    }

    #[test]
    fn test_permission_key_levels() {
        let company_key = PermissionKey::new("company:settings:update");
        assert!(company_key.is_company_level());
        assert!(!company_key.is_resource_level());

        let resource_key = PermissionKey::new("issues:read");
        assert!(!resource_key.is_company_level());
        assert!(resource_key.is_resource_level());
    }

    #[test]
    fn test_grant_scope_global() {
        let scope = GrantScope::global();
        assert_eq!(scope.scope_type(), Some("global"));
        assert!(scope.matches_resource("issue", Uuid::new_v4()));
        assert!(scope.matches_resource("agent", Uuid::new_v4()));
    }

    #[test]
    fn test_grant_scope_issue() {
        let issue_id = Uuid::new_v4();
        let scope = GrantScope::issue(issue_id);
        assert_eq!(scope.scope_type(), Some("issue"));
        assert!(scope.matches_resource("issue", issue_id));
        assert!(!scope.matches_resource("issue", Uuid::new_v4()));
    }

    #[test]
    fn test_grant_scope_agent() {
        let agent_id = Uuid::new_v4();
        let scope = GrantScope::agent(agent_id);
        assert_eq!(scope.scope_type(), Some("agent"));
        assert!(scope.matches_resource("agent", agent_id));
        assert!(!scope.matches_resource("agent", Uuid::new_v4()));
    }

    #[test]
    fn test_permission_grant_expiration() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let granted_by = Uuid::new_v4();

        let expired_time = Utc::now() - chrono::Duration::hours(1);
        let grant = PermissionGrant::new(
            company_id,
            PrincipalType::User,
            user_id,
            PermissionKey::from_const(PermissionKey::ISSUES_READ),
            GrantScope::global(),
            granted_by,
        )
        .with_expiration(expired_time);

        assert!(grant.is_expired());

        let future_time = Utc::now() + chrono::Duration::hours(1);
        let grant2 = PermissionGrant::new(
            company_id,
            PrincipalType::User,
            user_id,
            PermissionKey::from_const(PermissionKey::ISSUES_READ),
            GrantScope::global(),
            granted_by,
        )
        .with_expiration(future_time);

        assert!(!grant2.is_expired());
    }

    #[test]
    fn test_permission_grant_applies_to_resource() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let granted_by = Uuid::new_v4();
        let issue_id = Uuid::new_v4();

        let grant = PermissionGrant::new(
            company_id,
            PrincipalType::User,
            user_id,
            PermissionKey::from_const(PermissionKey::ISSUES_READ),
            GrantScope::issue(issue_id),
            granted_by,
        );

        assert!(grant.applies_to_resource("issue", issue_id));
        assert!(!grant.applies_to_resource("issue", Uuid::new_v4()));
    }
}
