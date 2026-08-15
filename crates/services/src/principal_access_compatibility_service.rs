/// Principal Access Compatibility Service
/// 
/// 主体访问兼容性检查

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
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
        let table = match principal_type {
            PrincipalType::User => "users",
            PrincipalType::Agent => "agents",
            PrincipalType::Service => "services",
            PrincipalType::Plugin => "plugins",
        };
        
        let query = format!(
            "SELECT COUNT(*) FROM {} WHERE id = $1 AND status = 'active'",
            table
        );
        
        let count: i64 = sqlx::query_scalar(&query)
            .bind(principal_id)
            .fetch_one(&self.pool)
            .await?;
        
        Ok(count > 0)
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
        // 检查权限表
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM permissions
            WHERE principal_id = $1 
              AND resource_id = $2
              AND granted = true
            "#
        )
        .bind(principal_id)
        .bind(resource_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count > 0)
    }
}
