use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StatusCardFinalizationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("finalization not found: {0}")]
    NotFound(Uuid),
    #[error("card already finalized: {0}")]
    AlreadyFinalized(Uuid),
}

pub type FinalizationResult<T> = Result<T, StatusCardFinalizationError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CardFinalization {
    pub id: Uuid,
    pub card_id: Uuid,
    pub final_status: String,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub finalized_at: chrono::DateTime<chrono::Utc>,
    pub finalized_by: Uuid,
}

#[async_trait]
pub trait StatusCardFinalizationService: Send + Sync {
    async fn finalize_card(
        &self,
        card_id: Uuid,
        final_status: String,
        summary: String,
        artifacts: Vec<String>,
        finalized_by: Uuid,
    ) -> FinalizationResult<CardFinalization>;
    
    async fn get_finalization(&self, card_id: Uuid) -> FinalizationResult<Option<CardFinalization>>;
    async fn is_finalized(&self, card_id: Uuid) -> FinalizationResult<bool>;
    async fn list_finalizations(&self, finalized_by: Uuid) -> FinalizationResult<Vec<CardFinalization>>;
}

pub struct StatusCardFinalizationServiceImpl {
    pool: PgPool,
}

impl StatusCardFinalizationServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatusCardFinalizationService for StatusCardFinalizationServiceImpl {
    async fn finalize_card(
        &self,
        card_id: Uuid,
        final_status: String,
        summary: String,
        artifacts: Vec<String>,
        finalized_by: Uuid,
    ) -> FinalizationResult<CardFinalization> {
        // Check if already finalized
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM card_finalizations WHERE card_id = $1"
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await?;
        
        if existing.is_some() {
            return Err(StatusCardFinalizationError::AlreadyFinalized(card_id));
        }
        
        let finalization_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO card_finalizations (id, card_id, final_status, summary, artifacts, finalized_at, finalized_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(finalization_id)
        .bind(card_id)
        .bind(&final_status)
        .bind(&summary)
        .bind(serde_json::to_value(&artifacts).unwrap())
        .bind(now)
        .bind(finalized_by)
        .execute(&self.pool)
        .await?;
        
        Ok(CardFinalization {
            id: finalization_id,
            card_id,
            final_status,
            summary,
            artifacts,
            finalized_at: now,
            finalized_by,
        })
    }
    
    async fn get_finalization(&self, card_id: Uuid) -> FinalizationResult<Option<CardFinalization>> {
        let row = sqlx::query_as::<_, CardFinalization>(
            r#"
            SELECT id, card_id, final_status, summary, artifacts, finalized_at, finalized_by
            FROM card_finalizations
            WHERE card_id = $1
            "#
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn is_finalized(&self, card_id: Uuid) -> FinalizationResult<bool> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM card_finalizations WHERE card_id = $1"
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.is_some())
    }
    
    async fn list_finalizations(&self, finalized_by: Uuid) -> FinalizationResult<Vec<CardFinalization>> {
        let rows = sqlx::query_as::<_, CardFinalization>(
            r#"
            SELECT id, card_id, final_status, summary, artifacts, finalized_at, finalized_by
            FROM card_finalizations
            WHERE finalized_by = $1
            ORDER BY finalized_at DESC
            "#
        )
        .bind(finalized_by)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
}
