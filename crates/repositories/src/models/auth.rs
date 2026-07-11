use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 认证用户表 - auth_users
///
/// 存储所有用户的基础认证信息，支持多种认证方式（Email/密码、OAuth、云租户等）
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub password_hash: Option<String>,
    pub email_verified: bool,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub avatar_url: Option<String>,
    pub oauth_provider: Option<String>,
    pub oauth_provider_id: Option<String>,
    pub cloud_tenant_id: Option<String>,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AuthUser {
    /// 创建新用户（Email/密码方式）
    pub fn new_with_password(email: String, password_hash: String, name: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            email,
            name,
            password_hash: Some(password_hash),
            email_verified: false,
            email_verified_at: None,
            avatar_url: None,
            oauth_provider: None,
            oauth_provider_id: None,
            cloud_tenant_id: None,
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建新用户（OAuth方式）
    pub fn new_with_oauth(
        email: String,
        name: Option<String>,
        provider: String,
        provider_id: String,
        avatar_url: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            email,
            name,
            password_hash: None,
            email_verified: true, // OAuth用户默认已验证
            email_verified_at: Some(now),
            avatar_url,
            oauth_provider: Some(provider),
            oauth_provider_id: Some(provider_id),
            cloud_tenant_id: None,
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建新用户（云租户方式）
    pub fn new_with_cloud_tenant(
        email: String,
        name: Option<String>,
        cloud_tenant_id: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            email,
            name,
            password_hash: None,
            email_verified: true, // 云租户用户默认已验证
            email_verified_at: Some(now),
           avatar_url: None,
            oauth_provider: None,
            oauth_provider_id: None,
            cloud_tenant_id: Some(cloud_tenant_id),
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 标记邮箱为已验证
    pub fn verify_email(&mut self) {
        self.email_verified = true;
        self.email_verified_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 更新最后登录时间
    pub fn record_login(&mut self) {
        self.last_login_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 停用用户
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    /// 激活用户
    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
}

/// 认证会话表 - auth_sessions
///
/// 存储用户登录会话信息，支持会话管理和单点登录
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub session_token: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl AuthSession {
    /// 创建新会话
    pub fn new(
        user_id: Uuid,
        session_token: String,
        ttl_seconds: i64,
        user_agent: Option<String>,
        ip_address: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            session_token,
            user_agent,
            ip_address,
            expires_at: now + chrono::Duration::seconds(ttl_seconds),
            last_activity_at: now,
            created_at: now,
        }
    }

    /// 检查会话是否已过期
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// 更新最后活动时间
    pub fn touch(&mut self) {
        self.last_activity_at = Utc::now();
    }

    /// 延长会话有效期
    pub fn extend(&mut self, ttl_seconds: i64) {
        self.expires_at = Utc::now() + chrono::Duration::seconds(ttl_seconds);
        self.last_activity_at = Utc::now();
    }
}

/// 公司表 - companies
///
/// 存储公司（租户）的基础信息，支持多租户隔离
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Company {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub website: Option<String>,
    pub industry: Option<String>,
    pub size: Option<String>,
    pub cloud_stack_id: Option<String>,
    pub settings: sqlx::types::JsonValue,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Company {
    /// 创建新公司
    pub fn new(name: String, slug: String, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            slug,
            description,
            logo_url: None,
            website: None,
            industry: None,
            size: None,
            cloud_stack_id: None,
            settings: sqlx::types::JsonValue::Object(serde_json::Map::new()),
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建新公司（云租户方式）
    pub fn new_with_cloud_stack(
        name: String,
        slug: String,
        cloud_stack_id: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            slug,
            description: None,
            logo_url: None,
            website: None,
            industry: None,
            size: None,
            cloud_stack_id: Some(cloud_stack_id),
            settings: sqlx::types::JsonValue::Object(serde_json::Map::new()),
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// 停用公司
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }

    /// 激活公司
    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }

    /// 更新设置
    pub fn update_settings(&mut self, settings: sqlx::types::JsonValue) {
        self.settings = settings;
    self.updated_at = Utc::now();
    }
}

/// 实例用户角色表 - instance_user_roles
///
/// 存储用户的实例级角色（跨公司的全局权限），如实例管理员
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InstanceUserRole {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub granted_by_user_id: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl InstanceUserRole {
    /// 创建新的实例角色
    pub fn new(user_id: Uuid, role: String, granted_by_user_id: Option<Uuid>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            role,
            granted_by_user_id,
            granted_at: now,
            created_at: now,
        }
    }

    /// 检查是否为实例管理员角色
    pub fn is_instance_admin(&self) -> bool {
        self.role == "instance_admin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_new_with_password() {
        let user = AuthUser::new_with_password(
            "test@example.com".to_string(),
            "hashed_password".to_string(),
            Some("Test User".to_string()),
        );

        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.password_hash, Some("hashed_password".to_string()));
        assert!(!user.email_verified);
        assert!(user.is_active);
    }

    #[test]
    fn test_auth_user_new_with_oauth() {
        let user = AuthUser::new_with_oauth(
            "test@example.com".to_string(),
            Some("Test User".to_string()),
            "google".to_string(),
            "google_123".to_string(),
            Some("https://avatar.url".to_string()),
        );

        assert_eq!(user.email, "test@example.com");
        assert_eq!(user.oauth_provider, Some("google".to_string()));
        assert!(user.email_verified);
        assert!(user.is_active);
    }

    #[test]
    fn test_auth_user_verify_email() {
        let mut user = AuthUser::new_with_password(
            "test@example.com".to_string(),
            "hashed_password".to_string(),
            None,
        );

        user.verify_email();
        assert!(user.email_verified);
        assert!(user.email_verified_at.is_some());
    }

    #[test]
    fn test_auth_session_expiration() {
        let user_id = Uuid::new_v4();
        let session = AuthSession::new(
            user_id,
            "session_token".to_string(),
            3600,
            None,
            None,
        );

        assert!(!session.is_expired());
    }

    #[test]
    fn test_company_new() {
        let company = Company::new(
            "Test Company".to_string(),
            "test-company".to_string(),
            Some("A test company".to_string()),
        );

        assert_eq!(company.name, "Test Company");
        assert_eq!(company.slug, "test-company");
        assert!(company.is_active);
    }

    #[test]
    fn test_instance_user_role_is_admin() {
        let user_id = Uuid::new_v4();
        let role = InstanceUserRole::new(user_id, "instance_admin".to_string(), None);

        assert!(role.is_instance_admin());
    }
}
