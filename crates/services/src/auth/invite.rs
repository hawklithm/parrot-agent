use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 邀请类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteType {
    /// 公司加入邀请（邀请用户或Agent加入公司）
    CompanyJoin,
    /// Bootstrap CEO邀请（初始化公司的第一个用户）
    BootstrapCeo,
}

impl InviteType {
    /// 是否为引导类型邀请
    pub fn is_bootstrap(&self) -> bool {
        matches!(self, Self::BootstrapCeo)
    }

    /// 是否为普通加入邀请
    pub fn is_join(&self) -> bool {
        matches!(self, Self::CompanyJoin)
    }
}

/// 允许的加入类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllowedJoinTypes {
    /// 仅允许人类用户加入
    Human,
    /// 仅允许Agent加入
    Agent,
    /// 允许人类和Agent加入
    Both,
}

impl AllowedJoinTypes {
    /// 检查是否允许人类加入
    pub fn allows_human(&self) -> bool {
        matches!(self, Self::Human | Self::Both)
    }

    /// 检查是否允许Agent加入
    pub fn allows_agent(&self) -> bool {
        matches!(self, Self::Agent | Self::Both)
    }

    /// 检查是否允许指定主体类型加入
    pub fn allows_principal_type(&self, principal_type: super::membership::PrincipalType) -> bool {
        match principal_type {
            super::membership::PrincipalType::User => self.allows_human(),
            super::membership::PrincipalType::Agent => self.allows_agent(),
        }
    }
}

/// 加入请求状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinRequestStatus {
    /// 待审批
    PendingApproval,
    /// 已批准
    Approved,
    /// 已拒绝
    Rejected,
}

impl JoinRequestStatus {
    /// 是否为待处理状态
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingApproval)
    }

    /// 是否已完成处理
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }

    /// 是否已批准
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved)
    }

    /// 是否已拒绝
    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected)
    }
}

/// 邀请记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invite {
    pub id: Uuid,
    pub company_id: Uuid,
    pub invite_type: InviteType,
    pub invited_by_user_id: Option<Uuid>,
    pub email: Option<String>,
    pub token: String,
    pub allowed_join_types: AllowedJoinTypes,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Invite {
    /// 创建新的邀请
    pub fn new(
        company_id: Uuid,
        invite_type: InviteType,
        invited_by_user_id: Option<Uuid>,
        email: Option<String>,
        allowed_join_types: AllowedJoinTypes,
        ttl_hours: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            invite_type,
            invited_by_user_id,
            email,
            token: Self::generate_token(),
            allowed_join_types,
            expires_at: now + chrono::Duration::hours(ttl_hours),
            used_at: None,
            created_at: now,
        }
    }

    /// 生成邀请token（安全随机）
    fn generate_token() -> String {
        use rand::Rng;
        let random_bytes: [u8; 32] = rand::thread_rng().gen();
        hex::encode(random_bytes)
    }

    /// 检查邀请是否已过期
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// 检查邀请是否已使用
    pub fn is_used(&self) -> bool {
        self.used_at.is_some()
    }

    /// 检查邀请是否仍然有效
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.is_used()
    }

    /// 标记邀请为已使用
    pub fn mark_as_used(&mut self) {
        self.used_at = Some(Utc::now());
    }

    /// 检查是否允许指定主体类型使用此邀请
    pub fn allows_principal_type(&self, principal_type: super::membership::PrincipalType) -> bool {
        self.allowed_join_types.allows_principal_type(principal_type)
    }
}

/// 加入请求记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: super::membership::PrincipalType,
    pub principal_id: Uuid,
    pub status: JoinRequestStatus,
    pub requested_role: super::membership::MembershipRole,
    pub message: Option<String>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JoinRequest {
    /// 创建新的加入请求
    pub fn new(
        company_id: Uuid,
        principal_type: super::membership::PrincipalType,
        principal_id: Uuid,
        requested_role: super::membership::MembershipRole,
        message: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            status: JoinRequestStatus::PendingApproval,
            requested_role,
            message,
            reviewed_by_user_id: None,
            reviewed_at: None,
            rejection_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 批准加入请求
    pub fn approve(&mut self, reviewed_by_user_id: Uuid) {
        self.status = JoinRequestStatus::Approved;
        self.reviewed_by_user_id = Some(reviewed_by_user_id);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 拒绝加入请求
    pub fn reject(&mut self, reviewed_by_user_id: Uuid, reason: Option<String>) {
        self.status = JoinRequestStatus::Rejected;
        self.reviewed_by_user_id = Some(reviewed_by_user_id);
        self.reviewed_at = Some(Utc::now());
        self.rejection_reason = reason;
        self.updated_at = Utc::now();
    }

    /// 检查请求是否可以被审批
    pub fn can_be_reviewed(&self) -> bool {
        self.status.is_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_type() {
        assert!(InviteType::BootstrapCeo.is_bootstrap());
        assert!(!InviteType::BootstrapCeo.is_join());
        assert!(InviteType::CompanyJoin.is_join());
        assert!(!InviteType::CompanyJoin.is_bootstrap());
    }

    #[test]
    fn test_allowed_join_types() {
        assert!(AllowedJoinTypes::Human.allows_human());
        assert!(!AllowedJoinTypes::Human.allows_agent());

        assert!(!AllowedJoinTypes::Agent.allows_human());
        assert!(AllowedJoinTypes::Agent.allows_agent());

        assert!(AllowedJoinTypes::Both.allows_human());
        assert!(AllowedJoinTypes::Both.allows_agent());
    }

    #[test]
    fn test_join_request_status() {
        assert!(JoinRequestStatus::PendingApproval.is_pending());
        assert!(!JoinRequestStatus::PendingApproval.is_final());

        assert!(!JoinRequestStatus::Approved.is_pending());
        assert!(JoinRequestStatus::Approved.is_final());
        assert!(JoinRequestStatus::Approved.is_approved());

        assert!(!JoinRequestStatus::Rejected.is_pending());
        assert!(JoinRequestStatus::Rejected.is_final());
        assert!(JoinRequestStatus::Rejected.is_rejected());
    }

    #[test]
    fn test_invite_creation() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let invite = Invite::new(
            company_id,
            InviteType::CompanyJoin,
            Some(user_id),
            Some("test@example.com".to_string()),
            AllowedJoinTypes::Both,
            24,
        );

        assert_eq!(invite.company_id, company_id);
        assert_eq!(invite.invite_type, InviteType::CompanyJoin);
        assert!(!invite.is_expired());
        assert!(!invite.is_used());
        assert!(invite.is_valid());
    }

    #[test]
    fn test_invite_expiration() {
        let company_id = Uuid::new_v4();
        let mut invite = Invite::new(
            company_id,
            InviteType::CompanyJoin,
            None,
            None,
            AllowedJoinTypes::Both,
            24,
        );

        // 模拟过期
        invite.expires_at = Utc::now() - chrono::Duration::hours(1);
        assert!(invite.is_expired());
        assert!(!invite.is_valid());
    }

    #[test]
    fn test_invite_mark_as_used() {
        let company_id = Uuid::new_v4();
        let mut invite = Invite::new(
            company_id,
            InviteType::CompanyJoin,
            None,
            None,
            AllowedJoinTypes::Both,
            24,
        );

        invite.mark_as_used();
        assert!(invite.is_used());
        assert!(!invite.is_valid());
    }

    #[test]
    fn test_join_request_creation() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let request = JoinRequest::new(
            company_id,
            super::super::membership::PrincipalType::User,
            user_id,
            super::super::membership::MembershipRole::Operator,
            Some("Please let me join".to_string()),
        );

        assert_eq!(request.company_id, company_id);
        assert_eq!(request.principal_id, user_id);
        assert_eq!(request.status, JoinRequestStatus::PendingApproval);
        assert!(request.can_be_reviewed());
    }

    #[test]
    fn test_join_request_approve() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let mut request = JoinRequest::new(
            company_id,
            super::super::membership::PrincipalType::User,
            user_id,
            super::super::membership::MembershipRole::Operator,
            None,
        );

        request.approve(reviewer_id);
        assert_eq!(request.status, JoinRequestStatus::Approved);
        assert_eq!(request.reviewed_by_user_id, Some(reviewer_id));
        assert!(request.reviewed_at.is_some());
        assert!(!request.can_be_reviewed());
    }

    #[test]
    fn test_join_request_reject() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let mut request = JoinRequest::new(
            company_id,
            super::super::membership::PrincipalType::User,
            user_id,
            super::super::membership::MembershipRole::Operator,
            None,
        );

        request.reject(reviewer_id, Some("Not qualified".to_string()));
        assert_eq!(request.status, JoinRequestStatus::Rejected);
        assert_eq!(request.reviewed_by_user_id, Some(reviewer_id));
        assert!(request.reviewed_at.is_some());
        assert_eq!(request.rejection_reason, Some("Not qualified".to_string()));
        assert!(!request.can_be_reviewed());
    }
}
