/// Managed Config Service
/// 
/// 托管配置管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ManagedConfigError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type ManagedConfigResult<T> = Result<T, ManagedConfigError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedConfig {
    pub id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub managed_by: Option<Uuid>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct ManagedConfigService {
    pool: PgPool,
}

impl ManagedConfigService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn set_config(
        &self,
        key: String,
        value: serde_json::Value,
        managed_by: Option<Uuid>,
    ) -> ManagedConfigResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO managed_configs 
            (id, key, value, managed_by, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (key)
            DO UPDATE SET value = $3, managed_by = $4, updated_at = $5
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&key)
        .bind(&value)
        .bind(managed_by)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_config(&self, key: &str) -> ManagedConfigResult<Option<ManagedConfig>> {
        let row = sqlx::query(
            r#"
            SELECT id, key, value, managed_by, updated_at
            FROM managed_configs
            WHERE key = $1
            "#
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| ManagedConfig {
            id: r.get("id"),
            key: r.get("key"),
            value: r.get("value"),
            managed_by: r.get("managed_by"),
            updated_at: r.get("updated_at"),
        }))
    }
    
    pub async fn list_configs(&self) -> ManagedConfigResult<Vec<ManagedConfig>> {
        let rows = sqlx::query(
            r#"
            SELECT id, key, value, managed_by, updated_at
            FROM managed_configs
            ORDER BY key
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let configs = rows.into_iter().map(|row| {
            ManagedConfig {
                id: row.get("id"),
                key: row.get("key"),
                value: row.get("value"),
                managed_by: row.get("managed_by"),
                updated_at: row.get("updated_at"),
            }
        }).collect();
        
        Ok(configs)
    }
}
