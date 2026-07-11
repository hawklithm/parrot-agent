use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 公司成员关系表 - company_memberships
///
/// 存储用户或Agent与公司的成员关系及角色
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CompanyMembershipRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: String,
    pub principal_id: Uuid,
    pub role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CompanyMembershipRow {
    /// 创建新的成员关系
    pub fn new(
        company_id: Uuid,
        principal_type: String,
        principal_id: Uuid,
        role: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            role,
            status: "active".to_string(),
            joined_at: now,
            archived_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 归档成员关系
    pub fn archive(&mut self) {
        self.status = "archived".to_string();
        self.archived_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 恢复成员关系
    pub fn restore(&mut self) {
        self.status = "active".to_string();
        self.archived_at = None;
        self.updated_at = Utc::now();
    }

    /// 更新角色
    pub fn update_role(&mut self, new_role: String) {
        self.role = new_role;
        self.updated_at = Utc::now();
    }

    /// 是否为活跃成员
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
}

/// 主体权限授予表 - principal_permission_grants
///
/// 存储显式的权限授予记录，支持细粒度权限控制
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PrincipalPermissionGrantRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: String,
    pub principal_id: Uuid,
    pub permission_key: String,
    pub scope: sqlx::types::JsonValue,
    pub granted_by_user_id: Uuid,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PrincipalPermissionGrantRow {
    /// 创建新的权限授予
    pub fn new(
        company_id: Uuid,
        principal_type: String,
        principal_id: Uuid,
        permission_key: String,
        scope: sqlx::types::JsonValue,
        granted_by_user_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            permission_key,
            scope,
            granted_by_user_id,
            expires_at: None,
            created_at: now,
            updated_at: now,
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

    /// 检查授予是否有效
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_company_membership_new() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let membership = CompanyMembershipRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "admin".to_string(),
        );

        assert_eq!(membership.company_id, company_id);
        assert_eq!(membership.principal_id, user_id);
        assert_eq!(membership.principal_type, "user");
        assert_eq!(membership.role, "admin");
        assert!(membership.is_active());
    }

    #[test]
    fn test_company_membership_archive() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let mut membership = CompanyMembershipRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "operator".to_string(),
        );

        membership.archive();
        assert!(!membership.is_active());
        assert_eq!(membership.status, "archived");
        assert!(membership.archived_at.is_some());
    }

    #[test]
    fn test_company_membership_restore() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let mut membership = CompanyMembershipRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "operator".to_string(),
        );

        membership.archive();
        membership.restore();
        assert!(membership.is_active());
        assert_eq!(membership.status, "active");
        assert!(membership.archived_at.is_none());
    }

    #[test]
    fn test_permission_grant_validity() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let granted_by = Uuid::new_v4();

        let grant = PrincipalPermissionGrantRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "issues:read".to_string(),
            serde_json::json!({"type": "global"}),
            granted_by,
        );

        assert!(grant.is_valid());
        assert!(!grant.is_expired());
    }

    #[test]
    fn test_permission_grant_expiration() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let granted_by = Uuid::new_v4();

        let expired_time = Utc::now() - chrono::Duration::hours(1);
        let grant = PrincipalPermissionGrantRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "issues:read".to_string(),
            serde_json::json!({"type": "global"}),
            granted_by,
        )
        .with_expiration(expired_time);

        assert!(!grant.is_valid());
        assert!(grant.is_expired());
    }
}
