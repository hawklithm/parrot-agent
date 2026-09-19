/// Principal Access Compatibility Service
/// 
/// 主体访问兼容性检查

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PrincipalAccessError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("incompatible: {0}")]
    Incompatible(String),
}

pub type PrincipalAccessResult<T> = Result<T, PrincipalAccessError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PrincipalType {
    User,
    Agent,
    Service,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCompatibility {
    pub principal_id: Uuid,
    pub principal_type: PrincipalType,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub compatible: bool,
    pub reasons: Vec<String>,
}

pub struct PrincipalAccessCompatibilityService {
    pool: PgPool,
}

impl PrincipalAccessCompatibilityService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Backfill the canonical principal rows expected by Paperclip's
    /// authorization layer. Older Parrot databases could contain agents and
    /// human memberships without the corresponding compatibility records,
    /// which made otherwise valid company-scoped requests fail at the first
    /// authorization check after a restart.
    pub async fn backfill_principal_access_compatibility(
        &self,
    ) -> PrincipalAccessResult<(u64, u64)> {
        let mut agent_memberships_inserted = 0_u64;
        let agent_rows = sqlx::query(
            "SELECT id, company_id FROM agents \
             WHERE status NOT IN ('pending_approval', 'terminated')",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in agent_rows {
            let result = sqlx::query(
                "INSERT INTO company_memberships \
                 (company_id, principal_type, principal_id, status, membership_role) \
                 VALUES ($1, 'agent'::principal_type, $2, 'active'::company_membership_status, 'viewer'::membership_role) \
                 ON CONFLICT (company_id, principal_type, principal_id) DO NOTHING",
            )
            .bind(row.get::<Uuid, _>("company_id"))
            .bind(row.get::<Uuid, _>("id"))
            .execute(&self.pool)
            .await?;
            agent_memberships_inserted += result.rows_affected();
        }

        // The request path already owns the role-to-grant mapping. Reuse it so
        // startup backfill cannot drift from grants created on login/company
        // creation, and keep the operation idempotent.
        let human_rows = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            "SELECT company_id, principal_id, membership_role::text \
             FROM company_memberships \
             WHERE principal_type = 'user'::principal_type \
               AND status = 'active'::company_membership_status",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut human_grants_inserted = 0_u64;
        for (company_id, user_id, role) in human_rows {
            let role = match role.as_str() {
                "owner" => crate::auth::MembershipRole::Owner,
                "admin" => crate::auth::MembershipRole::Admin,
                "operator" => crate::auth::MembershipRole::Operator,
                _ => crate::auth::MembershipRole::Viewer,
            };
            // This helper is deliberately best-effort per grant to preserve
            // compatibility with databases created before every permission
            // key was introduced.
            crate::auth::middleware::ensure_human_role_default_grants(
                &self.pool,
                company_id,
                user_id,
                role,
            )
            .await;
            human_grants_inserted += 1;
        }

        Ok((agent_memberships_inserted, human_grants_inserted))
    }
    
    pub async fn check_compatibility(
        &self,
        principal_id: Uuid,
        principal_type: PrincipalType,
        resource_id: Uuid,
        resource_type: &str,
    ) -> PrincipalAccessResult<AccessCompatibility> {
        let mut compatible = true;
        let mut reasons = Vec::new();
        
        // 检查主体状态
        let principal_active = self.is_principal_active(principal_id, &principal_type).await?;
        if !principal_active {
            compatible = false;
            reasons.push("Principal is not active".to_string());
        }
        
        // 检查资源状态
        let resource_available = self.is_resource_available(resource_id, resource_type).await?;
        if !resource_available {
            compatible = false;
            reasons.push("Resource is not available".to_string());
        }
        
        // 检查权限兼容性
        let has_permission = self.has_compatible_permissions(principal_id, &principal_type, resource_id).await?;
        if !has_permission {
            compatible = false;
            reasons.push("Insufficient permissions".to_string());
        }
        
        Ok(AccessCompatibility {
            principal_id,
            principal_type,
            resource_id,
            resource_type: resource_type.to_string(),
            compatible,
            reasons,
        })
    }
    
    async fn is_principal_active(
        &self,
        principal_id: Uuid,
        principal_type: &PrincipalType,
    ) -> PrincipalAccessResult<bool> {
        let active = match principal_type {
            PrincipalType::User => sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM auth_users WHERE id = $1 AND COALESCE(is_active, true))",
            )
            .bind(principal_id)
            .fetch_one(&self.pool)
            .await?,
            PrincipalType::Agent => sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id = $1 AND status NOT IN ('pending_approval', 'terminated'))",
            )
            .bind(principal_id)
            .fetch_one(&self.pool)
            .await?,
            // These principal types are retained for compatibility with old
            // library callers; Parrot's canonical schema does not persist
            // service/plugin principals as first-class users.
            PrincipalType::Service | PrincipalType::Plugin => false,
        };
        Ok(active)
    }
    
    async fn is_resource_available(
        &self,
        _resource_id: Uuid,
        _resource_type: &str,
    ) -> PrincipalAccessResult<bool> {
        // 简化实现：假设大多数资源可用
        Ok(true)
    }
    
    async fn has_compatible_permissions(
        &self,
        principal_id: Uuid,
        _principal_type: &PrincipalType,
        resource_id: Uuid,
    ) -> PrincipalAccessResult<bool> {
        let principal_type = match _principal_type {
            PrincipalType::User => "user",
            PrincipalType::Agent => "agent",
            PrincipalType::Service | PrincipalType::Plugin => return Ok(false),
        };
        let resource_id = resource_id.to_string();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM principal_permission_grants \
             WHERE principal_type = $1::principal_type AND principal_id = $2 \
               AND (scope = '{}'::jsonb OR scope @> jsonb_build_object('id', $3::text))",
        )
        .bind(principal_type)
        .bind(principal_id)
        .bind(resource_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }
}
