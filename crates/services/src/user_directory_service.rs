use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{
    AdminUserDirectoryEntry, AdminUserDirectoryResponse, CompanyUserDirectoryEntry,
    CompanyUserDirectoryResponse, UserDirectoryQuery, UserProfile,
};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

/// Service for user directory operations
#[async_trait]
pub trait UserDirectoryService: Send + Sync {
    /// List company user directory (active members)
    async fn list_company_users(
        &self,
        company_id: Uuid,
        query: UserDirectoryQuery,
    ) -> ServiceResult<CompanyUserDirectoryResponse>;

    /// List admin user directory (instance-wide, requires admin)
    async fn list_admin_users(
        &self,
        query: UserDirectoryQuery,
    ) -> ServiceResult<AdminUserDirectoryResponse>;
}

/// Placeholder implementation of UserDirectoryService
pub struct UserDirectoryServiceImpl {
    pool: Option<PgPool>,
}

impl UserDirectoryServiceImpl {
    pub fn new() -> Self {
        Self { pool: None }
    }
    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    fn pool(&self) -> ServiceResult<&PgPool> {
        self.pool.as_ref().ok_or_else(|| {
            crate::errors::ServiceError::Internal(
                "user directory database pool is not configured".into(),
            )
        })
    }

}

#[async_trait]
impl UserDirectoryService for UserDirectoryServiceImpl {
    async fn list_company_users(
        &self,
        company_id: Uuid,
        query: UserDirectoryQuery,
    ) -> ServiceResult<CompanyUserDirectoryResponse> {
        let pool = self.pool()?;
        let pattern = format!("%{}%", query.query);
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_memberships m JOIN auth_users u ON u.id=m.principal_id::uuid WHERE m.company_id=$1 AND m.principal_type='user'::principal_type AND m.status='active'::company_membership_status AND ($2='' OR u.email ILIKE $3 OR COALESCE(u.name,'') ILIKE $3)").bind(company_id).bind(&query.query).bind(&pattern).fetch_one(pool).await?;
        let rows = sqlx::query("SELECT m.principal_id::uuid as principal_id, m.status::text, u.email, u.name, u.avatar_url FROM company_memberships m JOIN auth_users u ON u.id=m.principal_id::uuid WHERE m.company_id=$1 AND m.principal_type='user'::principal_type AND m.status='active'::company_membership_status AND ($2='' OR u.email ILIKE $3 OR COALESCE(u.name,'') ILIKE $3) ORDER BY COALESCE(u.name,u.email), u.id LIMIT $4 OFFSET $5").bind(company_id).bind(&query.query).bind(&pattern).bind(query.limit as i64).bind(query.offset as i64).fetch_all(pool).await?;
        let users = rows.into_iter().map(|r| CompanyUserDirectoryEntry { principal_id:r.get("principal_id"), status:r.get("status"), user:Some(UserProfile{id:r.get("principal_id"),email:r.get("email"),name:r.get("name"),image:r.get("avatar_url")}) }).collect();

        Ok(CompanyUserDirectoryResponse {
            users,
            total: total as usize,
            limit: query.limit,
            offset: query.offset,
        })
    }

    async fn list_admin_users(
        &self,
        query: UserDirectoryQuery,
    ) -> ServiceResult<AdminUserDirectoryResponse> {
        let pool = self.pool()?;
        let pattern = format!("%{}%", query.query);
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_users WHERE ($1='' OR email ILIKE $2 OR COALESCE(name,'') ILIKE $2)").bind(&query.query).bind(&pattern).fetch_one(pool).await?;
        let rows = sqlx::query("SELECT u.id,u.email,u.name,u.avatar_url, EXISTS(SELECT 1 FROM instance_user_roles r WHERE r.user_id=u.id AND r.role='instance_admin') AS is_instance_admin, (SELECT COUNT(*) FROM company_memberships m WHERE m.principal_id=u.id AND m.principal_type='user'::principal_type AND m.status='active'::company_membership_status)::int AS membership_count FROM auth_users u WHERE ($1='' OR u.email ILIKE $2 OR COALESCE(u.name,'') ILIKE $2) ORDER BY COALESCE(u.name,u.email),u.id LIMIT $3 OFFSET $4").bind(&query.query).bind(&pattern).bind(query.limit as i64).bind(query.offset as i64).fetch_all(pool).await?;
        let users = rows.into_iter().map(|r| AdminUserDirectoryEntry{id:r.get("id"),email:r.get("email"),name:r.get("name"),image:r.get("avatar_url"),is_instance_admin:r.get("is_instance_admin"),active_company_membership_count:r.get("membership_count")}).collect();

        Ok(AdminUserDirectoryResponse {
            users,
            total: total as usize,
            limit: query.limit,
            offset: query.offset,
        })
    }
}

/// Factory function to create UserDirectoryService
pub fn create_user_directory_service() -> Arc<dyn UserDirectoryService> {
    Arc::new(UserDirectoryServiceImpl::new())
}
