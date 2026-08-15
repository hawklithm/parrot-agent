/// Email Service
/// 
/// 邮件发送服务

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("send failed: {0}")]
    SendFailed(String),
}

pub type EmailResult<T> = Result<T, EmailError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Email {
    pub id: Uuid,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub status: EmailStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmailStatus {
    Pending,
    Sending,
    Sent,
    Failed,
}

pub struct EmailService {
    pool: PgPool,
}

impl EmailService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn queue_email(
        &self,
        to: String,
        subject: String,
        body: String,
    ) -> EmailResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO emails 
            (id, to_address, subject, body, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(&to)
        .bind(&subject)
        .bind(&body)
        .bind(format!("{:?}", EmailStatus::Pending))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn send_email(&self, email_id: Uuid) -> EmailResult<()> {
        // 更新状态为sending
        sqlx::query(
            "UPDATE emails SET status = $1 WHERE id = $2"
        )
        .bind(format!("{:?}", EmailStatus::Sending))
        .bind(email_id)
        .execute(&self.pool)
        .await?;
        
        // 简化实现：实际应调用SMTP或邮件API
        
        // 更新状态为sent
        sqlx::query(
            "UPDATE emails SET status = $1, sent_at = $2 WHERE id = $3"
        )
        .bind(format!("{:?}", EmailStatus::Sent))
        .bind(chrono::Utc::now())
        .bind(email_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_pending_emails(&self) -> EmailResult<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT id FROM emails WHERE status = $1 ORDER BY created_at"
        )
        .bind(format!("{:?}", EmailStatus::Pending))
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }
}
