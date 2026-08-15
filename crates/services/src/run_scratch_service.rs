/// Run Scratch Service
/// 
/// Run 临时存储管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RunScratchError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("key not found: {0}")]
    KeyNotFound(String),
}

pub type RunScratchResult<T> = Result<T, RunScratchError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchData {
    pub id: Uuid,
    pub run_id: Uuid,
    pub key: String,
    pub value: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct RunScratchService {
    pool: PgPool,
}

impl RunScratchService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn set(
        &self,
        run_id: Uuid,
        key: String,
        value: serde_json::Value,
        ttl_seconds: Option<i64>,
    ) -> RunScratchResult<Uuid> {
        let expires_at = ttl_seconds.map(|ttl| {
            chrono::Utc::now() + chrono::Duration::seconds(ttl)
        });
        
        // Upsert
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO run_scratch (id, run_id, key, value, created_at, updated_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (run_id, key) 
            DO UPDATE SET value = $4, updated_at = $6, expires_at = $7
            RETURNING id
            "#
        )
        .bind(id)
        .bind(run_id)
        .bind(&key)
        .bind(&value)
        .bind(chrono::Utc::now())
        .bind(chrono::Utc::now())
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get(
        &self,
        run_id: Uuid,
        key: &str,
    ) -> RunScratchResult<Option<serde_json::Value>> {
        // 清理过期数据
        self.cleanup_expired().await?;
        
        let row = sqlx::query(
            r#"
            SELECT value
            FROM run_scratch
            WHERE run_id = $1 AND key = $2
              AND (expires_at IS NULL OR expires_at > $3)
            "#
        )
        .bind(run_id)
        .bind(key)
        .bind(chrono::Utc::now())
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| r.get("value")))
    }
    
    pub async fn get_all(&self, run_id: Uuid) -> RunScratchResult<Vec<ScratchData>> {
        self.cleanup_expired().await?;
        
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, key, value, created_at, updated_at, expires_at
            FROM run_scratch
            WHERE run_id = $1
              AND (expires_at IS NULL OR expires_at > $2)
            ORDER BY key
            "#
        )
        .bind(run_id)
        .bind(chrono::Utc::now())
        .fetch_all(&self.pool)
        .await?;
        
        let data = rows.into_iter().map(|row| {
            ScratchData {
                id: row.get("id"),
                run_id: row.get("run_id"),
                key: row.get("key"),
                value: row.get("value"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                expires_at: row.get("expires_at"),
            }
        }).collect();
        
        Ok(data)
    }
    
    pub async fn delete(&self, run_id: Uuid, key: &str) -> RunScratchResult<()> {
        sqlx::query(
            r#"
            DELETE FROM run_scratch
            WHERE run_id = $1 AND key = $2
            "#
        )
        .bind(run_id)
        .bind(key)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn delete_all(&self, run_id: Uuid) -> RunScratchResult<()> {
        sqlx::query(
            r#"
            DELETE FROM run_scratch
            WHERE run_id = $1
            "#
        )
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn cleanup_expired(&self) -> RunScratchResult<()> {
        sqlx::query(
            r#"
            DELETE FROM run_scratch
            WHERE expires_at IS NOT NULL AND expires_at <= $1
            "#
        )
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
