/// Quota Window Service
/// 
/// 配额窗口管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum QuotaWindowError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("quota exceeded")]
    QuotaExceeded,
}

pub type QuotaWindowResult<T> = Result<T, QuotaWindowError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub resource_type: String,
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
    pub limit: i64,
    pub consumed: i64,
}

pub struct QuotaWindowService {
    pool: PgPool,
}

impl QuotaWindowService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn get_current_window(
        &self,
        user_id: Uuid,
        resource_type: &str,
    ) -> QuotaWindowResult<QuotaWindow> {
        let now = chrono::Utc::now();
        
        // 查找当前窗口
        let row = sqlx::query(
            r#"
            SELECT id, user_id, resource_type, window_start, window_end, quota_limit, consumed
            FROM quota_windows
            WHERE user_id = $1 
              AND resource_type = $2
              AND window_start <= $3 
              AND window_end > $3
            "#
        )
        .bind(user_id)
        .bind(resource_type)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(r) = row {
            Ok(QuotaWindow {
                id: r.get("id"),
                user_id: r.get("user_id"),
                resource_type: r.get("resource_type"),
                window_start: r.get("window_start"),
                window_end: r.get("window_end"),
                limit: r.get("quota_limit"),
                consumed: r.get("consumed"),
            })
        } else {
            // 创建新窗口
            let id = Uuid::new_v4();
            let window_start = now;
            let window_end = now + chrono::Duration::hours(1);
            
            sqlx::query(
                r#"
                INSERT INTO quota_windows 
                (id, user_id, resource_type, window_start, window_end, quota_limit, consumed)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#
            )
            .bind(id)
            .bind(user_id)
            .bind(resource_type)
            .bind(window_start)
            .bind(window_end)
            .bind(1000i64)
            .bind(0i64)
            .execute(&self.pool)
            .await?;
            
            Ok(QuotaWindow {
                id,
                user_id,
                resource_type: resource_type.to_string(),
                window_start,
                window_end,
                limit: 1000,
                consumed: 0,
            })
        }
    }
    
    pub async fn consume(
        &self,
        user_id: Uuid,
        resource_type: &str,
        amount: i64,
    ) -> QuotaWindowResult<()> {
        let window = self.get_current_window(user_id, resource_type).await?;
        
        if window.consumed + amount > window.limit {
            return Err(QuotaWindowError::QuotaExceeded);
        }
        
        sqlx::query(
            "UPDATE quota_windows SET consumed = consumed + $1 WHERE id = $2"
        )
        .bind(amount)
        .bind(window.id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
