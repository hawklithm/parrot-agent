/// Plugin Log Retention Service
/// 
/// Plugin日志保留策略

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PluginLogRetentionError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type PluginLogRetentionResult<T> = Result<T, PluginLogRetentionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub plugin_id: Uuid,
    pub retention_days: i32,
    pub compress_after_days: i32,
    pub max_size_mb: i64,
}

pub struct PluginLogRetentionService {
    pool: PgPool,
}

impl PluginLogRetentionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn set_policy(
        &self,
        plugin_id: Uuid,
        retention_days: i32,
        compress_after_days: i32,
        max_size_mb: i64,
    ) -> PluginLogRetentionResult<()> {
        sqlx::query(
            r#"
            INSERT INTO plugin_log_retention_policies 
            (plugin_id, retention_days, compress_after_days, max_size_mb)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (plugin_id)
            DO UPDATE SET 
                retention_days = $2,
                compress_after_days = $3,
                max_size_mb = $4
            "#
        )
        .bind(plugin_id)
        .bind(retention_days)
        .bind(compress_after_days)
        .bind(max_size_mb)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_policy(&self, plugin_id: Uuid) -> PluginLogRetentionResult<Option<RetentionPolicy>> {
        let row = sqlx::query(
            r#"
            SELECT plugin_id, retention_days, compress_after_days, max_size_mb
            FROM plugin_log_retention_policies
            WHERE plugin_id = $1
            "#
        )
        .bind(plugin_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| RetentionPolicy {
            plugin_id: r.get("plugin_id"),
            retention_days: r.get("retention_days"),
            compress_after_days: r.get("compress_after_days"),
            max_size_mb: r.get("max_size_mb"),
        }))
    }
    
    pub async fn cleanup_old_logs(&self) -> PluginLogRetentionResult<i64> {
        // 删除过期日志
        let result = sqlx::query(
            r#"
            DELETE FROM plugin_logs
            WHERE created_at < NOW() - INTERVAL '1 day' * (
                SELECT retention_days 
                FROM plugin_log_retention_policies 
                WHERE plugin_id = plugin_logs.plugin_id
            )
            "#
        )
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as i64)
    }
}
