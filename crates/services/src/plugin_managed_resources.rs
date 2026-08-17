/// Plugin 管理的资源
/// 
/// 管理 Plugin 创建的 Agent、Routine、Skill 等资源

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ManagedResourceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("resource not found: {0}")]
    ResourceNotFound(Uuid),
    
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

pub type ManagedResourceResult<T> = Result<T, ManagedResourceError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    Agent,
    Routine,
    Skill,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedResource {
    pub id: Uuid,
    pub company_id: Uuid,
    pub plugin_id: Uuid,
    pub plugin_key: String,
    pub resource_kind: String,
    pub resource_key: String,
    pub resource_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Plugin 管理的资源服务
pub struct PluginManagedResourcesService {
    pool: PgPool,
}

impl PluginManagedResourcesService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 注册 Plugin 管理的资源
    pub async fn register_resource(
        &self,
        company_id: Uuid,
        plugin_id: Uuid,
        plugin_key: String,
        resource_kind: String,
        resource_key: String,
        resource_id: Uuid,
    ) -> ManagedResourceResult<Uuid> {
        let id = Uuid::new_v4();
        
        sqlx::query!(
            "INSERT INTO plugin_managed_resources (id, company_id, plugin_id, plugin_key, resource_kind, resource_key, resource_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
            id,
            company_id,
            plugin_id,
            plugin_key,
            resource_kind,
            resource_key,
            resource_id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 列出 Plugin 
    pub async fn list_resources(&self, plugin_id: Uuid) -> ManagedResourceResult<Vec<ManagedResource>> {
        let rows = sqlx::query!(
            "SELECT id, company_id, plugin_id, plugin_key, resource_kind, resource_key, resource_id, created_at
             FROM plugin_managed_resources
             WHERE plugin_id = $1
             ORDER BY created_at DESC",
            plugin_id
        )
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|row| {
            ManagedResource {
                id: row.id,
                company_id: row.company_id,
                plugin_id: row.plugin_id,
                plugin_key: row.plugin_key,
                resource_kind: row.resource_kind,
                resource_key: row.resource_key,
                resource_id: row.resource_id,
                created_at: row.created_at,
            }
        }).collect())
    }
    
    /// 取消注册资源
    pub async fn unregister_resource(&self, id: Uuid) -> ManagedResourceResult<()> {
        sqlx::query!(
            "DELETE FROM plugin_managed_resources WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// 清理 Plugin 的所有资源
    pub async fn cleanup_plugin_resources(&self, plugin_id: Uuid) -> ManagedResourceResult<usize> {
        let result = sqlx::query!(
            "DELETE FROM plugin_managed_resources WHERE plugin_id = $1",
            plugin_id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    #[ignore]
    async fn test_register_and_list() {
        // 需要数据库连接
    }
}
