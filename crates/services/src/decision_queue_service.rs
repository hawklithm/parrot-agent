use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DecisionQueueError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("queue entry not found: {0}")]
    NotFound(Uuid),
    #[error("queue full")]
    QueueFull,
}

pub type DecisionQueueResult<T> = Result<T, DecisionQueueError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DecisionQueueEntry {
    pub id: Uuid,
    pub decision_id: Uuid,
    pub priority: i32,
    pub agent_id: Uuid,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait DecisionQueueService: Send + Sync {
    async fn enqueue(
        &self,
        decision_id: Uuid,
        agent_id: Uuid,
        priority: i32,
        deadline: Option<chrono::DateTime<chrono::Utc>>,
    ) -> DecisionQueueResult<Uuid>;
    
    async fn dequeue(&self, agent_id: Uuid) -> DecisionQueueResult<Option<DecisionQueueEntry>>;
    async fn peek(&self, agent_id: Uuid) -> DecisionQueueResult<Option<DecisionQueueEntry>>;
    async fn remove(&self, entry_id: Uuid) -> DecisionQueueResult<()>;
    async fn get_queue_length(&self, agent_id: Uuid) -> DecisionQueueResult<i64>;
    async fn clear_expired(&self) -> DecisionQueueResult<i64>;
}

pub struct DecisionQueueServiceImpl {
    pool: PgPool,
    max_queue_size: usize,
}

impl DecisionQueueServiceImpl {
    pub fn new(pool: PgPool, max_queue_size: usize) -> Self {
        Self {
            pool,
            max_queue_size,
        }
    }
}

#[async_trait]
impl DecisionQueueService for DecisionQueueServiceImpl {
    async fn enqueue(
        &self,
        decision_id: Uuid,
        agent_id: Uuid,
        priority: i32,
        deadline: Option<chrono::DateTime<chrono::Utc>>,
    ) -> DecisionQueueResult<Uuid> {
        let current_size: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM decision_queue WHERE agent_id = $1"
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;
        
        if current_size.0 >= self.max_queue_size as i64 {
            return Err(DecisionQueueError::QueueFull);
        }
        
        let entry_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO decision_queue (id, decision_id, priority, agent_id, deadline, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#
        )
        .bind(entry_id)
        .bind(decision_id)
        .bind(priority)
        .bind(agent_id)
        .bind(deadline)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(entry_id)
    }
    
    async fn dequeue(&self, agent_id: Uuid) -> DecisionQueueResult<Option<DecisionQueueEntry>> {
        let mut tx = self.pool.begin().await?;
        
        let entry = sqlx::query_as::<_, DecisionQueueEntry>(
            r#"
            SELECT id, decision_id, priority, agent_id, deadline, created_at
            FROM decision_queue
            WHERE agent_id = $1
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#
        )
        .bind(agent_id)
        .fetch_optional(&mut *tx)
        .await?;
        
        if let Some(ref e) = entry {
            sqlx::query("DELETE FROM decision_queue WHERE id = $1")
                .bind(e.id)
                .execute(&mut *tx)
                .await?;
        }
        
        tx.commit().await?;
        Ok(entry)
    }
    
    async fn peek(&self, agent_id: Uuid) -> DecisionQueueResult<Option<DecisionQueueEntry>> {
        let entry = sqlx::query_as::<_, DecisionQueueEntry>(
            r#"
            SELECT id, decision_id, priority, agent_id, deadline, created_at
            FROM decision_queue
            WHERE agent_id = $1
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            "#
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(entry)
    }
    
    async fn remove(&self, entry_id: Uuid) -> DecisionQueueResult<()> {
        sqlx::query("DELETE FROM decision_queue WHERE id = $1")
            .bind(entry_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    async fn get_queue_length(&self, agent_id: Uuid) -> DecisionQueueResult<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM decision_queue WHERE agent_id = $1"
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(row.0)
    }
    
    async fn clear_expired(&self) -> DecisionQueueResult<i64> {
        let now = chrono::Utc::now();
        
        let result = sqlx::query(
            "DELETE FROM decision_queue WHERE deadline IS NOT NULL AND deadline < $1"
        )
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as i64)
    }
}
