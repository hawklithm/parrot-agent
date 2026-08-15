/// Plan Review Context Service
/// 
/// 计划审查上下文管理

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PlanReviewContextError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type PlanReviewContextResult<T> = Result<T, PlanReviewContextError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewContext {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub context_data: serde_json::Value,
    pub reviewer_notes: Vec<String>,
    pub status: ReviewStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewStatus {
    Pending,
    InReview,
    Approved,
    Rejected,
}

pub struct PlanReviewContextService {
    pool: PgPool,
}

impl PlanReviewContextService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_context(
        &self,
        plan_id: Uuid,
        context_data: serde_json::Value,
    ) -> PlanReviewContextResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO plan_review_contexts 
            (id, plan_id, context_data, reviewer_notes, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(plan_id)
        .bind(&context_data)
        .bind(Vec::<String>::new())
        .bind(format!("{:?}", ReviewStatus::Pending))
        .bind(chrono::Utc::now())
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn add_note(
        &self,
        context_id: Uuid,
        note: String,
    ) -> PlanReviewContextResult<()> {
        sqlx::query(
            r#"
            UPDATE plan_review_contexts 
            SET reviewer_notes = array_append(reviewer_notes, $1),
                updated_at = $2
            WHERE id = $3
            "#
        )
        .bind(&note)
        .bind(chrono::Utc::now())
        .bind(context_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn update_status(
        &self,
        context_id: Uuid,
        status: ReviewStatus,
    ) -> PlanReviewContextResult<()> {
        sqlx::query(
            r#"
            UPDATE plan_review_contexts 
            SET status = $1, updated_at = $2
            WHERE id = $3
            "#
        )
        .bind(format!("{:?}", status))
        .bind(chrono::Utc::now())
        .bind(context_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
