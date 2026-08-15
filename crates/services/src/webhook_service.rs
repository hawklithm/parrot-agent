/// Webhook Service
/// 
/// Webhook管理和分发

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("delivery failed: {0}")]
    DeliveryFailed(String),
}

pub type WebhookResult<T> = Result<T, WebhookError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: Uuid,
    pub url: String,
    pub event_type: String,
    pub secret: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub payload: serde_json::Value,
    pub status: DeliveryStatus,
    pub attempts: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Pending,
    Delivering,
    Delivered,
    Failed,
}

pub struct WebhookService {
    pool: PgPool,
}

impl WebhookService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn register_webhook(
        &self,
        url: String,
        event_type: String,
        secret: Option<String>,
    ) -> WebhookResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO webhooks 
            (id, url, event_type, secret, active)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&url)
        .bind(&event_type)
        .bind(secret)
        .bind(true)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn trigger_webhooks(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> WebhookResult<Vec<Uuid>> {
        let webhooks = sqlx::query(
            "SELECT id FROM webhooks WHERE event_type = $1 AND active = true"
        )
        .bind(event_type)
        .fetch_all(&self.pool)
        .await?;
        
        let mut delivery_ids = Vec::new();
        
        for webhook in webhooks {
            let webhook_id: Uuid = webhook.get("id");
            let delivery_id = Uuid::new_v4();
            
            sqlx::query(
                r#"
                INSERT INTO webhook_deliveries 
                (id, webhook_id, payload, status, attempts, created_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#
            )
            .bind(delivery_id)
            .bind(webhook_id)
            .bind(&payload)
            .bind(format!("{:?}", DeliveryStatus::Pending))
            .bind(0)
            .bind(chrono::Utc::now())
            .execute(&self.pool)
            .await?;
            
            delivery_ids.push(delivery_id);
        }
        
        Ok(delivery_ids)
    }
    
    pub async fn deliver(&self, delivery_id: Uuid) -> WebhookResult<()> {
        // 简化实现：实际应发送HTTP POST请求
        sqlx::query(
            "UPDATE webhook_deliveries SET status = $1, attempts = attempts + 1 WHERE id = $2"
        )
        .bind(format!("{:?}", DeliveryStatus::Delivered))
        .bind(delivery_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
