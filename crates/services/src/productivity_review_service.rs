/// Productivity Review Service
/// 
/// 生产力审查

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ProductivityReviewError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type ProductivityReviewResult<T> = Result<T, ProductivityReviewError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductivityMetrics {
    pub user_id: Uuid,
    pub period_start: chrono::DateTime<chrono::Utc>,
    pub period_end: chrono::DateTime<chrono::Utc>,
    pub tasks_completed: i32,
    pub avg_completion_time_hours: f64,
    pub quality_score: f64,
}

pub struct ProductivityReviewService {
    pool: PgPool,
}

impl ProductivityReviewService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn calculate_metrics(
        &self,
        user_id: Uuid,
        days: i32,
    ) -> ProductivityReviewResult<ProductivityMetrics> {
        let end = chrono::Utc::now();
        let start = end - chrono::Duration::days(days as i64);
        
        let row = sqlx::query(
            r#"
            SELECT 
                COUNT(*) as tasks_completed,
                AVG(EXTRACT(EPOCH FROM (completed_at - created_at))/3600) as avg_hours,
                AVG(quality_score) as quality
            FROM tasks
            WHERE user_id = $1
              AND completed_at BETWEEN $2 AND $3
            "#
        )
        .bind(user_id)
        .bind(start)
        .bind(end)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(ProductivityMetrics {
            user_id,
            period_start: start,
            period_end: end,
            tasks_completed: row.get::<i64, _>("tasks_completed") as i32,
            avg_completion_time_hours: row.get::<Option<f64>, _>("avg_hours").unwrap_or(0.0),
            quality_score: row.get::<Option<f64>, _>("quality").unwrap_or(0.0),
        })
    }
    
    pub async fn generate_report(
        &self,
        user_id: Uuid,
    ) -> ProductivityReviewResult<serde_json::Value> {
        let weekly = self.calculate_metrics(user_id, 7).await?;
        let monthly = self.calculate_metrics(user_id, 30).await?;
        
        Ok(serde_json::json!({
            "weekly": weekly,
            "monthly": monthly,
            "trend": if weekly.tasks_completed > 0 {
                "improving"
            } else {
                "stable"
            }
        }))
    }
}
