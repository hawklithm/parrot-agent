use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 公司成员角色类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    /// 所有者 - 最高权限，可以管理公司和所有资源
    Owner,
    /// 管理员 - 高级管理权限，可以管理成员和大部分资源
    Admin,
    /// 操作员 - 日常操作权限，可以创建和管理Agent、Issue等
    Operator,
    /// 查看者 - 只读权限，只能查看资源
    Viewer,
}

impl MembershipRole {
    /// 是否可以管理成员
    pub fn can_manage_members(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// 是否可以创建资源
    pub fn can_create_resources(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Operator)
    }

    /// 是否可以删除资源
    pub fn can_delete_resources(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// 是否可以修改资源
    pub fn can_update_resources(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Operator)
    }

    /// 是否只有只读权限
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Viewer)
    }

    /// 角色优先级（数字越大权限越高）
    pub fn priority(&self) -> u8 {
        match self {
            Self::Owner => 4,
            Self::Admin => 3,
            Self::Operator => 2,
            Self::Viewer => 1,
        }
    }

    /// 检查是否有足够权限（self >= required）
    pub fn has_privilege(&self, required: &Self) -> bool {
        self.priority() >= required.priority()
    }
}

impl fmt::Display for MembershipRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner => write!(f, "Owner"),
            Self::Admin => write!(f, "Admin"),
            Self::Operator => write!(f, "Operator"),
            Self::Viewer => write!(f, "Viewer"),
        }
    }
}

/// 主体类型（权限系统中的参与者）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    /// 人类用户
    User,
    /// AI Agent
    Agent,
}

impl fmt::Display for PrincipalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "User"),
            Self::Agent => write!(f, "Agent"),
        }
    }
}

/// 成员关系状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    /// 活跃成员
    Active,
    /// 已归档（软删除，保留历史记录）
    Archived,
}

impl MembershipStatus {
    /// 是否为活跃状态
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 是否为归档状态
    pub fn is_archived(&self) -> bool {
        matches!(self, Self::Archived)
    }
}

/// 公司成员关系
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyMembership {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: PrincipalType,
    pub principal_id: Uuid,
    pub role: MembershipRole,
    pub status: MembershipStatus,
    pub joined_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CompanyMembership {
    /// 创建新的成员关系
    pub fn new(
        company_id: Uuid,
        principal_type: PrincipalType,
        principal_id: Uuid,
        role: MembershipRole,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            role,
            status: MembershipStatus::Active,
            joined_at: now,
            archived_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 归档成员关系（软删除）
    pub fn archive(&mut self) {
        self.status = MembershipStatus::Archived;
        self.archived_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 恢复成员关系
    pub fn restore(&mut self) {
        self.status = MembershipStatus::Active;
        self.archived_at = None;
        self.updated_at = Utc::now();
    }

    /// 更新角色
    pub fn update_role(&mut self, new_role: MembershipRole) {
        self.role = new_role;
        self.updated_at = Utc::now();
    }

    /// 是否为用户成员
    pub fn is_user(&self) -> bool {
        self.principal_type == PrincipalType::User
    }

    /// 是否为Agent成员
    pub fn is_agent(&self) -> bool {
        self.principal_type == PrincipalType::Agent
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_role_priority() {
        assert_eq!(MembershipRole::Owner.priority(), 4);
        assert_eq!(MembershipRole::Admin.priority(), 3);
        assert_eq!(MembershipRole::Operator.priority(), 2);
        assert_eq!(MembershipRole::Viewer.priority(), 1);
    }

    #[test]
    fn test_membership_role_has_privilege() {
        assert!(MembershipRole::Owner.has_privilege(&MembershipRole::Admin));
        assert!(MembershipRole::Admin.has_privilege(&MembershipRole::Operator));
        assert!(MembershipRole::Operator.has_privilege(&MembershipRole::Viewer));
        assert!(!MembershipRole::Viewer.has_privilege(&MembershipRole::Operator));
    }

    #[test]
    fn test_membership_role_permissions() {
        assert!(MembershipRole::Owner.can_manage_members());
        assert!(MembershipRole::Admin.can_manage_members());
        assert!(!MembershipRole::Operator.can_manage_members());
        assert!(!MembershipRole::Viewer.can_manage_members());

        assert!(MembershipRole::Operator.can_create_resources());
        assert!(!MembershipRole::Viewer.can_create_resources());

        assert!(MembershipRole::Viewer.is_read_only());
        assert!(!MembershipRole::Operator.is_read_only());
    }

    #[test]
    fn test_membership_status() {
        assert!(MembershipStatus::Active.is_active());
        assert!(!MembershipStatus::Active.is_archived());
        assert!(MembershipStatus::Archived.is_archived());
        assert!(!MembershipStatus::Archived.is_active());
    }

    #[test]
    fn test_company_membership_new() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Admin,
        );

        assert_eq!(membership.company_id, company_id);
        assert_eq!(membership.principal_id, user_id);
        assert_eq!(membership.principal_type, PrincipalType::User);
        assert_eq!(membership.role, MembershipRole::Admin);
        assert_eq!(membership.status, MembershipStatus::Active);
        assert!(membership.is_user());
        assert!(!membership.is_agent());
    }

    #[test]
    fn test_company_membership_archive() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        );

        membership.archive();
        assert_eq!(membership.status, MembershipStatus::Archived);
        assert!(membership.archived_at.is_some());
    }

    #[test]
    fn test_company_membership_restore() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Operator,
        );

        membership.archive();
        membership.restore();
        assert_eq!(membership.status, MembershipStatus::Active);
        assert!(membership.archived_at.is_none());
    }

    #[test]
    fn test_company_membership_update_role() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut membership = CompanyMembership::new(
            company_id,
            PrincipalType::User,
            user_id,
            MembershipRole::Viewer,
        );

        membership.update_role(MembershipRole::Admin);
        assert_eq!(membership.role, MembershipRole::Admin);
    }
}
