use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SummarySlotServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("slot not found: {0}")]
    NotFound(Uuid),
    #[error("slot conflict: {0}")]
    Conflict(String),
}

pub type SlotResult<T> = Result<T, SummarySlotServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SummarySlot {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub slot_type: SlotType,
    pub content: String,
    pub priority: i32,
    pub status: SlotStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum SlotType {
    Summary,
    Highlight,
    Warning,
    Achievement,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum SlotStatus {
    Active,
    Archived,
    Deprecated,
}

#[derive(Debug, Clone)]
pub struct CreateSlotRequest {
    pub agent_id: Uuid,
    pub slot_type: SlotType,
    pub content: String,
    pub priority: i32,
}

#[async_trait]
pub trait SummarySlotService: Send + Sync {
    async fn create_slot(&self, req: CreateSlotRequest) -> SlotResult<SummarySlot>;
    async fn get_slot(&self, slot_id: Uuid) -> SlotResult<Option<SummarySlot>>;
    async fn update_slot(&self, slot_id: Uuid, content: String, priority: Option<i32>) -> SlotResult<()>;
    async fn archive_slot(&self, slot_id: Uuid) -> SlotResult<()>;
    async fn list_active_slots(&self, agent_id: Uuid) -> SlotResult<Vec<SummarySlot>>;
    async fn get_top_priority_slots(&self, agent_id: Uuid, limit: i32) -> SlotResult<Vec<SummarySlot>>;
}

pub struct SummarySlotServiceImpl {
    pool: PgPool,
}

impl SummarySlotServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SummarySlotService for SummarySlotServiceImpl {
    async fn create_slot(&self, req: CreateSlotRequest) -> SlotResult<SummarySlot> {
        let slot_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO summary_slots (id, agent_id, slot_type, content, priority, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(slot_id)
        .bind(req.agent_id)
        .bind(serde_json::to_value(&req.slot_type).unwrap())
        .bind(&req.content)
        .bind(req.priority)
        .bind(serde_json::to_value(&SlotStatus::Active).unwrap())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(SummarySlot {
            id: slot_id,
            agent_id: req.agent_id,
            slot_type: req.slot_type,
            content: req.content,
            priority: req.priority,
            status: SlotStatus::Active,
            created_at: now,
            updated_at: now,
        })
    }
    
    async fn get_slot(&self, slot_id: Uuid) -> SlotResult<Option<SummarySlot>> {
        let row = sqlx::query_as::<_, SummarySlot>(
            r#"
            SELECT id, agent_id, slot_type, content, priority, status, created_at, updated_at
            FROM summary_slots
            WHERE id = $1
            "#
        )
        .bind(slot_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn update_slot(&self, slot_id: Uuid, content: String, priority: Option<i32>) -> SlotResult<()> {
        let now = chrono::Utc::now();
        
        if let Some(p) = priority {
            sqlx::query(
                "UPDATE summary_slots SET content = $1, priority = $2, updated_at = $3 WHERE id = $4"
            )
            .bind(&content)
            .bind(p)
            .bind(now)
            .bind(slot_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE summary_slots SET content = $1, updated_at = $2 WHERE id = $3"
            )
            .bind(&content)
            .bind(now)
            .bind(slot_id)
            .execute(&self.pool)
            .await?;
        }
        
        Ok(())
    }
    
    async fn archive_slot(&self, slot_id: Uuid) -> SlotResult<()> {
        let now = chrono::Utc::now();
        
        sqlx::query(
            "UPDATE summary_slots SET status = $1, updated_at = $2 WHERE id = $3"
        )
        .bind(serde_json::to_value(&SlotStatus::Archived).unwrap())
        .bind(now)
        .bind(slot_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn list_active_slots(&self, agent_id: Uuid) -> SlotResult<Vec<SummarySlot>> {
        let rows = sqlx::query_as::<_, SummarySlot>(
            r#"
            SELECT id, agent_id, slot_type, content, priority, status, created_at, updated_at
            FROM summary_slots
            WHERE agent_id = $1 AND status = 'active'
            ORDER BY priority DESC, created_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn get_top_priority_slots(&self, agent_id: Uuid, limit: i32) -> SlotResult<Vec<SummarySlot>> {
        let rows = sqlx::query_as::<_, SummarySlot>(
            r#"
            SELECT id, agent_id, slot_type, content, priority, status, created_at, updated_at
            FROM summary_slots
            WHERE agent_id = $1 AND status = 'active'
            ORDER BY priority DESC, created_at DESC
            LIMIT $2
            "#
        )
        .bind(agent_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
}
