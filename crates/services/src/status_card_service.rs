use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StatusCardServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("status card not found: {0}")]
    NotFound(Uuid),
}

pub type StatusCardResult<T> = Result<T, StatusCardServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StatusCard {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub run_id: Uuid,
    pub status: CardStatus,
    pub title: String,
    pub description: Option<String>,
    pub metrics: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum CardStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMetric {
    pub name: String,
    pub value: serde_json::Value,
    pub unit: Option<String>,
}

#[async_trait]
pub trait StatusCardService: Send + Sync {
    async fn create_status_card(
        &self,
        agent_id: Uuid,
        run_id: Uuid,
        title: String,
    ) -> StatusCardResult<StatusCard>;
    
    async fn get_status_card(&self, card_id: Uuid) -> StatusCardResult<Option<StatusCard>>;
    
    async fn update_status_card(
        &self,
        card_id: Uuid,
        status: Option<CardStatus>,
        description: Option<String>,
        metrics: Option<Vec<StatusMetric>>,
    ) -> StatusCardResult<()>;
    
    async fn list_status_cards(&self, run_id: Uuid) -> StatusCardResult<Vec<StatusCard>>;
    async fn delete_status_card(&self, card_id: Uuid) -> StatusCardResult<()>;
}

pub struct StatusCardServiceImpl {
    pool: PgPool,
}

impl StatusCardServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StatusCardService for StatusCardServiceImpl {
    async fn create_status_card(
        &self,
        agent_id: Uuid,
        run_id: Uuid,
        title: String,
    ) -> StatusCardResult<StatusCard> {
        let card_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO status_cards (id, agent_id, run_id, status, title, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#
        )
        .bind(card_id)
        .bind(agent_id)
        .bind(run_id)
        .bind(serde_json::to_value(&CardStatus::Active).unwrap())
        .bind(&title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(StatusCard {
            id: card_id,
            agent_id,
            run_id,
            status: CardStatus::Active,
            title,
            description: None,
            metrics: serde_json::json!([]),
            created_at: now,
            updated_at: now,
        })
    }
    
    async fn get_status_card(&self, card_id: Uuid) -> StatusCardResult<Option<StatusCard>> {
        let row = sqlx::query_as::<_, StatusCard>(
            r#"
            SELECT id, agent_id, run_id, status, title, description, metrics, created_at, updated_at
            FROM status_cards
            WHERE id = $1
            "#
        )
        .bind(card_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn update_status_card(
        &self,
        card_id: Uuid,
        status: Option<CardStatus>,
        description: Option<String>,
        metrics: Option<Vec<StatusMetric>>,
    ) -> StatusCardResult<()> {
        let now = chrono::Utc::now();
        
        if let Some(s) = status {
            sqlx::query("UPDATE status_cards SET status = $1, updated_at = $2 WHERE id = $3")
                .bind(serde_json::to_value(&s).unwrap())
                .bind(now)
                .bind(card_id)
                .execute(&self.pool)
                .await?;
        }
        
        if let Some(d) = description {
            sqlx::query("UPDATE status_cards SET description = $1, updated_at = $2 WHERE id = $3")
                .bind(d)
                .bind(now)
                .bind(card_id)
                .execute(&self.pool)
                .await?;
        }
        
        if let Some(m) = metrics {
            sqlx::query("UPDATE status_cards SET metrics = $1, updated_at = $2 WHERE id = $3")
                .bind(serde_json::to_value(&m).unwrap())
                .bind(now)
                .bind(card_id)
                .execute(&self.pool)
                .await?;
        }
        
        Ok(())
    }
    
    async fn list_status_cards(&self, run_id: Uuid) -> StatusCardResult<Vec<StatusCard>> {
        let rows = sqlx::query_as::<_, StatusCard>(
            r#"
            SELECT id, agent_id, run_id, status, title, description, metrics, created_at, updated_at
            FROM status_cards
            WHERE run_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn delete_status_card(&self, card_id: Uuid) -> StatusCardResult<()> {
        sqlx::query("DELETE FROM status_cards WHERE id = $1")
            .bind(card_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
}
