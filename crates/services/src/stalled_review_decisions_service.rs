/// Stalled Review Decisions Service
/// 
/// 停滞审查决策管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StalledReviewError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type StalledReviewResult<T> = Result<T, StalledReviewError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalledReview {
    pub id: Uuid,
    pub review_id: Uuid,
    pub stalled_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub auto_action: Option<AutoAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutoAction {
    Approve,
    Reject,
    Escalate,
}

pub struct StalledReviewDecisionsService {
    pool: PgPool,
    stall_threshold_hours: i64,
}

impl StalledReviewDecisionsService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            stall_threshold_hours: 24,
        }
    }
    
    pub async fn detect_stalled_reviews(&self) -> StalledReviewResult<Vec<StalledReview>> {
        let threshold = chrono::Utc::now() - chrono::Duration::hours(self.stall_threshold_hours);
        
        let rows = sqlx::query(
            r#"
            SELECT id, review_id, created_at, status
            FROM reviews
            WHERE status = 'pending'
              AND created_at < $1
            "#
        )
        .bind(threshold)
        .fetch_all(&self.pool)
        .await?;
        
        let stalled = rows.into_iter().map(|row| {
            StalledReview {
                id: Uuid::new_v4(),
                review_id: row.get("id"),
                stalled_at: row.get("created_at"),
                reason: "No response within threshold".to_string(),
                auto_action: Some(AutoAction::Escalate),
            }
        }).collect();
        
        Ok(stalled)
    }
    
    pub async fn apply_auto_action(&self, stalled: &StalledReview) -> StalledReviewResult<()> {
        if let Some(action) = &stalled.auto_action {
            let new_status = match action {
                AutoAction::Approve => "approved",
                AutoAction::Reject => "rejected",
                AutoAction::Escalate => "escalated",
            };
            
            sqlx::query(
                "UPDATE reviews SET status = $1 WHERE id = $2"
            )
            .bind(new_status)
            .bind(stalled.review_id)
            .execute(&self.pool)
            .await?;
        }
        
        Ok(())
    }
}
