/// Live Events Service
/// 
/// 实时事件推送

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LiveEventsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type LiveEventsResult<T> = Result<T, LiveEventsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveEvent {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub target_users: Vec<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct LiveEventsService {
    pool: PgPool,
}

impl LiveEventsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn publish_event(
        &self,
        event_type: String,
        payload: serde_json::Value,
        target_users: Vec<Uuid>,
    ) -> LiveEventsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO live_events 
            (id, event_type, payload, target_users, created_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&event_type)
        .bind(&payload)
        .bind(&target_users)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_user_events(
        &self,
        user_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> LiveEventsResult<Vec<LiveEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT id, event_type, payload, target_users, created_at
            FROM live_events
            WHERE $1 = ANY(target_users)
              AND created_at > $2
            ORDER BY created_at DESC
            "#
        )
        .bind(user_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        
        let events = rows.into_iter().map(|row| {
            LiveEvent {
                id: row.get("id"),
                event_type: row.get("event_type"),
                payload: row.get("payload"),
                target_users: row.get("target_users"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(events)
    }
}
