use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 邀请表 - invites
///
/// 存储公司邀请记录，支持用户和Agent加入公司
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InviteRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub invite_type: String,
    pub invited_by_user_id: Option<Uuid>,
    pub email: Option<String>,
    pub token: String,
    pub allowed_join_types: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl InviteRow {
    /// 创建新的邀请
    pub fn new(
        company_id: Uuid,
        invite_type: String,
        invited_by_user_id: Option<Uuid>,
        email: Option<String>,
        token: String,
        allowed_join_types: String,
        ttl_hours: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            invite_type,
            invited_by_user_id,
            email,
            token,
            allowed_join_types,
            expires_at: now + chrono::Duration::hours(ttl_hours),
            used_at: None,
            created_at: now,
        }
    }

    /// 检查邀请是否已过期
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// 检查邀请是否已使用
    pub fn is_used(&self) -> bool {
        self.used_at.is_some()
    }

    /// 检查邀请是否有效
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.is_used()
    }

    /// 标记邀请为已使用
    pub fn mark_as_used(&mut self) {
        self.used_at = Some(Utc::now());
    }

    /// 是否为Bootstrap CEO邀请
    pub fn is_bootstrap(&self) -> bool {
        self.invite_type == "bootstrap_ceo"
    }

    /// 是否允许人类加入
    pub fn allows_human(&self) -> bool {
        self.allowed_join_types == "human" || self.allowed_join_types == "both"
    }

    /// 是否允许Agent加入
    pub fn allows_agent(&self) -> bool {
        self.allowed_join_types == "agent" || self.allowed_join_types == "both"
    }
}

/// 加入请求表 - join_requests
///
/// 存储用户或Agent的公司加入请求，支持审批流程
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JoinRequestRow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: String,
    pub principal_id: Uuid,
    pub status: String,
    pub requested_role: String,
    pub message: Option<String>,
    pub reviewed_by_user_id: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JoinRequestRow {
    /// 创建新的加入请求
    pub fn new(
        company_id: Uuid,
        principal_type: String,
        principal_id: Uuid,
        requested_role: String,
        message: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            principal_type,
            principal_id,
            status: "pending_approval".to_string(),
            requested_role,
            message,
            reviewed_by_user_id: None,
            reviewed_at: None,
            rejection_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 检查请求状态
    pub fn is_pending(&self) -> bool {
        self.status == "pending_approval"
    }

    pub fn is_approved(&self) -> bool {
        self.status == "approved"
    }

    pub fn is_rejected(&self) -> bool {
        self.status == "rejected"
    }

    pub fn is_final(&self) -> bool {
        self.is_approved() || self.is_rejected()
    }

    /// 检查请求是否可以被审批
    pub fn can_be_reviewed(&self) -> bool {
        self.is_pending()
    }

    /// 批准加入请求
    pub fn approve(&mut self, reviewed_by_user_id: Uuid) {
        self.status = "approved".to_string();
        self.reviewed_by_user_id = Some(reviewed_by_user_id);
        self.reviewed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 拒绝加入请求
    pub fn reject(&mut self, reviewed_by_user_id: Uuid, reason: Option<String>) {
        self.status = "rejected".to_string();
        self.reviewed_by_user_id = Some(reviewed_by_user_id);
        self.reviewed_at = Some(Utc::now());
        self.rejection_reason = reason;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_creation() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let invite = InviteRow::new(
            company_id,
            "company_join".to_string(),
            Some(user_id),
            Some("test@example.com".to_string()),
            "invite_token_123".to_string(),
            "both".to_string(),
            24,
        );

        assert_eq!(invite.company_id, company_id);
        assert!(invite.is_valid());
        assert!(!invite.is_expired());
        assert!(!invite.is_used());
        assert!(invite.allows_human());
        assert!(invite.allows_agent());
    }

    #[test]
    fn test_invite_expiration() {
        let company_id = Uuid::new_v4();
        let mut invite = InviteRow::new(
            company_id,
            "company_join".to_string(),
            None,
            None,
            "token".to_string(),
            "both".to_string(),
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
        let mut invite = InviteRow::new(
            company_id,
            "company_join".to_string(),
            None,
            None,
            "token".to_string(),
            "both".to_string(),
            24,
        );

        invite.mark_as_used();
        assert!(invite.is_used());
        assert!(!invite.is_valid());
    }

    #[test]
    fn test_invite_allowed_join_types() {
        let company_id = Uuid::new_v4();

        let human_only = InviteRow::new(
            company_id,
            "company_join".to_string(),
            None,
            None,
            "token".to_string(),
            "human".to_string(),
            24,
        );
        assert!(human_only.allows_human());
        assert!(!human_only.allows_agent());

        let agent_only = InviteRow
            company_id,
            "company_join".to_string(),
            None,
            None,
            "token".to_string(),
            "agent".to_string(),
            24,
        );
        assert!(!agent_only.allows_human());
        assert!(agent_only.allows_agent());
    }

    #[test]
    fn test_join_request_creation() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let request = JoinRequestRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "operator".to_string(),
            Some("I want to join".to_string()),
        );

        assert_eq!(request.company_id, company_id);
        assert_eq!(request.principal_id, user_id);
        assert!(request.is_pending());
        assert!(request.can_be_reviewed());
    }

    #[test]
    fn test_join_request_approve() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();

        let mut request = JoinRequestRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "operator".to_string(),
            None,
        );

        request.approve(reviewer_id);
        assert!(request.is_approved());
        assert!(request.is_final());
        assert_eq!(request.reviewed_by_user_id, Some(reviewer_id));
        assert!(!request.can_be_reviewed());
    }

    #[test]
    fn test_join_request_reject() {
        let company_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();

        let mut request = JoinRequestRow::new(
            company_id,
            "user".to_string(),
            user_id,
            "operator".to_string(),
            None,
        );

        request.reject(reviewer_id, Some("Insufficient qualifications".to_string()));
        assert!(request.is_rejected());
        assert!(request.is_final());
        assert_eq!(request.rejection_reason, Some("Insufficient qualifications".to_string()));
        assert!(!request.can_be_reviewed());
    }
}
