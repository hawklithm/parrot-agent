use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{
    AdminUserDirectoryEntry, AdminUserDirectoryQuery, AdminUserDirectoryResponse,
    CompanyUserDirectoryEntry, CompanyUserDirectoryResponse, UserProfile,
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
    ) -> ServiceResult<CompanyUserDirectoryResponse>;

    /// List admin user directory (instance-wide, requires admin)
    async fn list_admin_users(
        &self,
        query: AdminUserDirectoryQuery,
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
    ) -> ServiceResult<CompanyUserDirectoryResponse> {
        let pool = self.pool()?;
        // Paperclip `loadCompanyUserDirectory` 不做搜索/分页，仅按成员更新时间倒序
        // 取全部活跃用户成员，再逐 id 挂上用户资料。
        let rows = sqlx::query(
            "SELECT m.principal_id::uuid AS principal_id, m.status::text AS status, \
             u.id AS user_id, u.email, u.name, u.avatar_url \
             FROM company_memberships m \
             LEFT JOIN auth_users u ON u.id = m.principal_id::uuid \
             WHERE m.company_id = $1 \
               AND m.principal_type = 'user'::principal_type \
               AND m.status = 'active'::company_membership_status \
             ORDER BY m.updated_at DESC",
        )
        .bind(company_id)
        .fetch_all(pool)
        .await?;
        let users = rows
            .into_iter()
            .map(|r| {
                let user_id: Option<Uuid> = r.get("user_id");
                CompanyUserDirectoryEntry {
                    principal_id: r.get("principal_id"),
                    status: r.get("status"),
                    user: user_id.map(|id| UserProfile {
                        id,
                        email: r.get("email"),
                        name: r.get("name"),
                        image: r.get("avatar_url"),
                    }),
                }
            })
            .collect();

        Ok(CompanyUserDirectoryResponse { users })
    }

    async fn list_admin_users(
        &self,
        query: AdminUserDirectoryQuery,
    ) -> ServiceResult<AdminUserDirectoryResponse> {
        let pool = self.pool()?;
        // Paperclip 先把全部用户读进内存、按 needle 过滤，再截断前 50 条；此处
        // 用 ILIKE 在库内完成同一语义（`%` 是调用方可注入的通配符，与 Paperclip
        // 的 `includes(needle)` 子串语义存在差异，属已知轻微放宽）。
        let pattern = format!("%{}%", query.query);
        let rows = sqlx::query(
            "SELECT u.id, u.email, u.name, u.avatar_url, \
             EXISTS(SELECT 1 FROM instance_user_roles r \
                    WHERE r.user_id = u.id AND r.role = 'instance_admin') AS is_instance_admin, \
             (SELECT COUNT(*) FROM company_memberships m \
              WHERE m.principal_id = u.id \
                AND m.principal_type = 'user'::principal_type \
                AND m.status = 'active'::company_membership_status)::int AS membership_count \
             FROM auth_users u \
             WHERE $1 = '' OR u.email ILIKE $2 OR COALESCE(u.name, '') ILIKE $2 \
             ORDER BY u.updated_at DESC \
             LIMIT $3",
        )
        .bind(&query.query)
        .bind(&pattern)
        .bind(models::ADMIN_USER_DIRECTORY_LIMIT as i64)
        .fetch_all(pool)
        .await?;
        let users = rows
            .into_iter()
            .map(|r| AdminUserDirectoryEntry {
                user: UserProfile {
                    id: r.get("id"),
                    email: r.get("email"),
                    name: r.get("name"),
                    image: r.get("avatar_url"),
                },
                is_instance_admin: r.get("is_instance_admin"),
                active_company_membership_count: r.get("membership_count"),
            })
            .collect();

        Ok(AdminUserDirectoryResponse { users })
    }
}

/// Factory function to create UserDirectoryService
pub fn create_user_directory_service() -> Arc<dyn UserDirectoryService> {
    Arc::new(UserDirectoryServiceImpl::new())
}
