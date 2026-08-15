use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum FeedbackServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("feedback not found: {0}")]
    NotFound(Uuid),
    #[error("invalid feedback: {0}")]
    Invalid(String),
}

pub type FeedbackResult<T> = Result<T, FeedbackServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Feedback {
    pub id: Uuid,
    pub user_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub rating: i32,
    pub comment: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFeedbackRequest {
    pub user_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub rating: i32,
    pub comment: Option<String>,
}

#[async_trait]
pub trait FeedbackService: Send + Sync {
    async fn create_feedback(&self, req: CreateFeedbackRequest) -> FeedbackResult<Feedback>;
    async fn get_feedback(&self, feedback_id: Uuid) -> FeedbackResult<Option<Feedback>>;
    async fn list_feedback(&self, target_id: Uuid) -> FeedbackResult<Vec<Feedback>>;
    async fn delete_feedback(&self, feedback_id: Uuid) -> FeedbackResult<()>;
    async fn get_average_rating(&self, target_id: Uuid) -> FeedbackResult<f64>;
}

pub struct FeedbackServiceImpl {
    pool: PgPool,
}

impl FeedbackServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FeedbackService for FeedbackServiceImpl {
    async fn create_feedback(&self, req: CreateFeedbackRequest) -> FeedbackResult<Feedback> {
        let feedback_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO feedback (id, user_id, target_type, target_id, rating, comment, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(feedback_id)
        .bind(req.user_id)
        .bind(&req.target_type)
        .bind(req.target_id)
        .bind(&req.comment)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(Feedback {
            id: feedback_id,
            user_id: req.user_id,
            target_type: req.target_type,
            target_id: req.target_id,
            rating: req.rating,
            comment: req.comment,
            created_at: now,
        })
    }
    
    async fn get_feedback(&self, feedback_id: Uuid) -> FeedbackResult<Option<Feedback>> {
        let row = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, target_type, target_id, rating, comment, created_at
            FROM feedback
            WHERE id = $1
            "#
        )
        .bind(feedback_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn list_feedback(&self, target_id: Uuid) -> FeedbackResult<Vec<Feedback>> {
        let rows = sqlx::query_as::<_, Feedback>(
            r#"
            SELECT id, user_id, target_type, target_id, rating, comment, created_at
            FROM feedback
            WHERE target_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(target_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn delete_feedback(&self, feedback_id: Uuid) -> FeedbackResult<()> {
        sqlx::query("DELETE FROM feedback WHERE id = $1")
            .bind(feedback_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    async fn get_average_rating(&self, target_id: Uuid) -> FeedbackResult<f64> {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT AVG(rating) FROM feedback WHERE target_id = $1"
        )
        .bind(target_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row.0.unwrap_or(0.0))
    }
}
