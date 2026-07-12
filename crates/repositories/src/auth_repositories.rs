use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::auth::{AuthSession, AuthUser, Company, InstanceUserRole};
use crate::models::authorization::{CompanyMembershipRow, PrincipalPermissionGrantRow};
use crate::models::invite::{InviteRow, JoinRequestRow};
use super::board_api_key_repository::{RepositoryError, RepositoryResult};

/// AuthUser Repository
#[async_trait]
pub trait AuthUserRepository: Send + Sync {
    async fn find_by_email(&self, email: &str) -> RepositoryResult<Option<AuthUser>>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<AuthUser>>;
    async fn create(&self, user: AuthUser) -> RepositoryResult<AuthUser>;
    async fn update(&self, user: AuthUser) -> RepositoryResult<AuthUser>;
    async fn find_by_oauth_provider(&self, provider: &str, provider_id: &str) -> RepositoryResult<Option<AuthUser>>;
}

/// AuthSession Repository
#[async_trait]
pub trait AuthSessionRepository: Send + Sync {
    async fn find_by_token(&self, token: &str) -> RepositoryResult<Option<AuthSession>>;
    async fn create(&self, session: AuthSession) -> RepositoryResult<AuthSession>;
    async fn delete(&self, id: Uuid) -> RepositoryResult<()>;
    async fn delete_by_user(&self, user_id: Uuid) -> RepositoryResult<()>;
    async fn extend(&self, id: Uuid, ttl_seconds: i64) -> RepositoryResult<()>;
}

/// Company Repository
#[async_trait]
pub trait CompanyRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Company>>;
    async fn find_by_slug(&self, slug: &str) -> RepositoryResult<Option<Company>>;
    async fn create(&self, company: Company) -> RepositoryResult<Company>;
    async fn update(&self, company: Company) -> RepositoryResult<Company>;
    async fn list_all(&self) -> RepositoryResult<Vec<Company>>;
}

/// CompanyMembership Repository
#[async_trait]
pub trait CompanyMembershipRepository: Send + Sync {
    async fn find_by_principal(&self, company_id: Uuid, principal_type: &str, principal_id: Uuid) -> RepositoryResult<Option<CompanyMembershipRow>>;
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<CompanyMembershipRow>>;
    async fn list_by_principal(&self, principal_type: &str, principal_id: Uuid) -> RepositoryResult<Vec<CompanyMembershipRow>>;
    async fn create(&self, membership: CompanyMembershipRow) -> RepositoryResult<CompanyMembershipRow>;
    async fn update_role(&self, id: Uuid, new_role: String) -> RepositoryResult<()>;
    async fn archive(&self, id: Uuid) -> RepositoryResult<()>;
    async fn restore(&self, id: Uuid) -> RepositoryResult<()>;
}

/// Invite Repository
#[async_trait]
pub trait InviteRepository: Send + Sync {
    async fn find_by_token(&self, token: &str) -> RepositoryResult<Option<InviteRow>>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<InviteRow>>;
    async fn create(&self, invite: InviteRow) -> RepositoryResult<InviteRow>;
    async fn mark_used(&self, id: Uuid) -> RepositoryResult<()>;
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<InviteRow>>;
}

/// JoinRequest Repository
#[async_trait]
pub trait JoinRequestRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<JoinRequestRow>>;
    async fn create(&self, request: JoinRequestRow) -> RepositoryResult<JoinRequestRow>;
    async fn approve(&self, id: Uuid, reviewed_by_user_id: Uuid) -> RepositoryResult<()>;
    async fn reject(&self, id: Uuid, reviewed_by_user_id: Uuid, reason: Option<String>) -> RepositoryResult<()>;
    async fn list_pending_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<JoinRequestRow>>;
}

/// InstanceUserRole Repository
#[async_trait]
pub trait InstanceUserRoleRepository: Send + Sync {
    async fn find_by_user(&self, user_id: Uuid) -> RepositoryResult<Vec<InstanceUserRole>>;
    async fn create(&self, role: InstanceUserRole) -> RepositoryResult<InstanceUserRole>;
    async fn delete(&self, id: Uuid) -> RepositoryResult<()>;
    async fn is_instance_admin(&self, user_id: Uuid) -> RepositoryResult<bool>;
}

// ==================== PostgreSQL实现 ====================

pub struct PgAuthUserRepository {
    pool: PgPool,
}

impl PgAuthUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthUserRepository for PgAuthUserRepository {
    async fn find_by_email(&self, email: &str) -> RepositoryResult<Option<AuthUser>> {
        let row = sqlx::query_as::<_, AuthUser>(
            "SELECT * FROM auth_users WHERE email = $1 AND is_active = true"
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<AuthUser>> {
        let row = sqlx::query_as::<_, AuthUser>(
            "SELECT * FROM auth_users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn create(&self, user: AuthUser) -> RepositoryResult<AuthUser> {
        sqlx::query(
            r#"INSERT INTO auth_users (id, email, name, password_hash, email_verified, email_verified_at,
               avatar_url, oauth_provider, oauth_provider_id, cloud_tenant_id, is_active,
               last_login_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#
        )
        .bind(user.id).bind(&user.email).bind(&user.name).bind(&user.password_hash)
        .bind(user.email_verified).bind(user.email_verified_at).bind(&user.avatar_url)
        .bind(&user.oauth_provider).bind(&user.oauth_provider_id).bind(&user.cloud_tenant_id)
        .bind(user.is_active).bind(user.last_login_at).bind(user.created_at).bind(user.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(user)
    }

    async fn update(&self, user: AuthUser) -> RepositoryResult<AuthUser> {
        sqlx::query(
            r#"UPDATE auth_users SET email = $2, name = $3, password_hash = $4, email_verified = $5,
               email_verified_at = $6, avatar_url = $7, is_active = $8, last_login_at = $9, updated_at = $10
               WHERE id = $1"#
        )
        .bind(user.id).bind(&user.email).bind(&user.name).bind(&user.password_hash)
        .bind(user.email_verified).bind(user.email_verified_at).bind(&user.avatar_url)
        .bind(user.is_active).bind(user.last_login_at).bind(user.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(user)
    }

    async fn find_by_oauth_provider(&self, provider: &str, provider_id: &str) -> RepositoryResult<Option<AuthUser>> {
        let row = sqlx::query_as::<_, AuthUser>(
            "SELECT * FROM auth_users WHERE oauth_provider = $1 AND oauth_provider_id = $2 AND is_active = true"
        )
        .bind(provider).bind(provider_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

pub struct PgAuthSessionRepository {
    pool: PgPool,
}

impl PgAuthSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthSessionRepository for PgAuthSessionRepository {
    async fn find_by_token(&self, token: &str) -> RepositoryResult<Option<AuthSession>> {
        let row = sqlx::query_as::<_, AuthSession>(
            "SELECT * FROM auth_sessions WHERE session_token = $1 AND expires_at > NOW()"
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn create(&self, session: AuthSession) -> RepositoryResult<AuthSession> {
        sqlx::query(
            r#"INSERT INTO auth_sessions (id, user_id, session_token, user_agent, ip_address,
               expires_at, last_activity_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#
        )
        .bind(session.id).bind(session.user_id).bind(&session.session_token)
        .bind(&session.user_agent).bind(&session.ip_address).bind(session.expires_at)
        .bind(session.last_activity_at).bind(session.created_at)
        .execute(&self.pool)
        .await?;
        Ok(session)
    }

    async fn delete(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM auth_sessions WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_by_user(&self, user_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn extend(&self, id: Uuid, ttl_seconds: i64) -> RepositoryResult<()> {
        sqlx::query(
            "UPDATE auth_sessions SET expires_at = NOW() + INTERVAL '1 second' * $2, last_activity_at = NOW() WHERE id = $1"
        )
        .bind(id).bind(ttl_seconds)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub struct PgCompanyRepository {
    pool: PgPool,
}

impl PgCompanyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CompanyRepository for PgCompanyRepository {
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Company>> {
        let row = sqlx::query_as::<_, Company>(
            "SELECT * FROM companies WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn find_by_slug(&self, slug: &str) -> RepositoryResult<Option<Company>> {
        let row = sqlx::query_as::<_, Company>(
            "SELECT * FROM companies WHERE slug = $1"
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn create(&self, company: Company) -> RepositoryResult<Company> {
        sqlx::query(
            r#"INSERT INTO companies (id, name, slug, descrio_url, website, industry, size,
               cloud_stack_id, settings, is_active, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#
        )
        .bind(company.id).bind(&company.name).bind(&company.slug).bind(&company.description)
        .bind(&company.logo_url).bind(&company.website).bind(&company.industry).bind(&company.size)
        .bind(&company.cloud_stack_id).bind(&company.settings).bind(company.is_active)
        .bind(company.created_at).bind(company.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(company)
    }

    async fn update(&self, company: Company) -> RepositoryResult<Company> {
        sqlx::query(
            r#"UPDATE companies SET name = $2, description = $3, logo_url = $4, website = $5,
               industry = $6, size = $7, settings = $8, is_active = $9, updated_at = $10 WHERE id = $1"#
        )
        .bind(company.id).bind(&company.name).bind(&company.description).bind(&company.logo_url)
        .bind(&company.website).bind(&company.industry).bind(&company.size).bind(&company.settings)
        .bind(company.is_active).bind(company.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(company)
    }

    async fn list_all(&self) -> RepositoryResult<Vec<Company>> {
        let rows = sqlx::query_as::<_, Company>(
            "SELECT * FROM companies WHERE is_active = true ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

pub struct PgCompanyMembershipRepository {
    pool: PgPool,
}

impl PgCompanyMembershipRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CompanyMembershipRepository for PgCompanyMembershipRepository {
    async fn find_by_principal(&self, company_id: Uuid, principal_type: &str, principal_id: Uuid) -> RepositoryResult<Option<CompanyMembershipRow>> {
        let row = sqlx::query_as::<_, CompanyMembershipRow>(
            "SELECT * FROM company_memberships WHERE company_id = $1 AND principal_type = $2 AND principal_id = $3"
        )
        .bind(company_id).bind(principal_type).bind(principal_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<CompanyMembershipRow>> {
        let rows = sqlx::query_as::<_, CompanyMembershipRow>(
            "SELECT * FROM company_memberships WHERE company_id = $1 AND status = 'active'"
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn list_by_principal(&self, principal_type: &str, principal_id: Uuid) -> RepositoryResult<Vec<CompanyMembershipRow>> {
        let rows = sqlx::query_as::<_, CompanyMembershipRow>(
            "SELECT * FROM company_memberships WHERE principal_type = $1 AND principal_id = $2 AND status = 'active'"
        )
        .bind(principal_type).bind(principal_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn create(&self, membership: CompanyMembershipRow) -> RepositoryResult<CompanyMembershipRow> {
        sqlx::query(
            r#"INSERT INTO company_memberships (id, company_id, principal_type, principal_id, role, status,
               joined_at, archived_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(membership.id).bind(membership.company_id).bind(&membership.principal_type)
        .bind(membership.principal_id).bind(&membership.role).bind(&membership.status)
        .bind(membership.joined_at).bind(membership.archived_at)
        .bind(membership.created_at).bind(membership.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(membership)
    }

    async fn update_role(&self, id: Uuid, new_role: String) -> RepositoryResult<()> {
        sqlx::query("UPDATE company_memberships SET role = $2, updated_at = NOW() WHERE id = $1")
            .bind(id).bind(new_role)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn archive(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query("UPDATE company_memberships SET status = 'archived', archived_at = NOW(), updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn restore(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query("UPDATE company_memberships SET status = 'active', archived_at = NULL, updated_at = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// Principal Permission Grant Repository
///
/// 管理 `principal_permission_grants` 表中的显式权限授予记录。
#[async_trait]
pub trait PrincipalPermissionGrantRepository: Send + Sync {
    /// 查询某个主体在公司内对指定权限键的**有效**授予（未过期）。
    async fn find_valid_grant(
        &self,
        company_id: Uuid,
        principal_type: &str,
        principal_id: Uuid,
        permission_key: &str,
    ) -> RepositoryResult<Option<PrincipalPermissionGrantRow>>;

    /// 列出某个主体在公司内的所有有效授予。
    async fn list_valid_grants(
        &self,
        company_id: Uuid,
        principal_type: &str,
        principal_id: Uuid,
    ) -> RepositoryResult<Vec<PrincipalPermissionGrantRow>>;

    /// 创建一条权限授予记录。
    async fn create(&self, grant: PrincipalPermissionGrantRow) -> RepositoryResult<PrincipalPermissionGrantRow>;
}

#[derive(Debug, Clone)]
pub struct PgPrincipalPermissionGrantRepository {
    pool: PgPool,
}

impl PgPrincipalPermissionGrantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PrincipalPermissionGrantRepository for PgPrincipalPermissionGrantRepository {
    async fn find_valid_grant(
        &self,
        company_id: Uuid,
        principal_type: &str,
        principal_id: Uuid,
        permission_key: &str,
    ) -> RepositoryResult<Option<PrincipalPermissionGrantRow>> {
        let grant = sqlx::query_as::<_, PrincipalPermissionGrantRow>(
            r#"SELECT id, company_id, principal_type, principal_id, permission_key,
                      scope, granted_by_user_id, expires_at, created_at, updated_at
               FROM principal_permission_grants
               WHERE company_id = $1 AND principal_type = $2 AND principal_id = $3
                 AND permission_key = $4
                 AND (expires_at IS NULL OR expires_at > NOW())"#,
        )
        .bind(company_id)
        .bind(principal_type)
        .bind(principal_id)
        .bind(permission_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(grant)
    }

    async fn list_valid_grants(
        &self,
        company_id: Uuid,
        principal_type: &str,
        principal_id: Uuid,
    ) -> RepositoryResult<Vec<PrincipalPermissionGrantRow>> {
        let grants = sqlx::query_as::<_, PrincipalPermissionGrantRow>(
            r#"SELECT id, company_id, principal_type, principal_id, permission_key,
                      scope, granted_by_user_id, expires_at, created_at, updated_at
               FROM principal_permission_grants
               WHERE company_id = $1 AND principal_type = $2 AND principal_id = $3
                 AND (expires_at IS NULL OR expires_at > NOW())"#,
        )
        .bind(company_id)
        .bind(principal_type)
        .bind(principal_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(grants)
    }

    async fn create(&self, grant: PrincipalPermissionGrantRow) -> RepositoryResult<PrincipalPermissionGrantRow> {
        sqlx::query(
            r#"INSERT INTO principal_permission_grants
               (id, company_id, principal_type, principal_id, permission_key, scope,
                granted_by_user_id, expires_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(grant.id)
        .bind(grant.company_id)
        .bind(&grant.principal_type)
        .bind(grant.principal_id)
        .bind(&grant.permission_key)
        .bind(&grant.scope)
        .bind(grant.granted_by_user_id)
        .bind(grant.expires_at)
        .bind(grant.created_at)
        .bind(grant.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(grant)
    }
}
