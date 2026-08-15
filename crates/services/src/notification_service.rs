/// Notification Service
/// 
/// 通知创建、发送、订阅和历史管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("notification not found: {0}")]
    NotFound(Uuid),
    
    #[error("invalid notification: {0}")]
    Invalid(String),
}

pub type NotificationResult<T> = Result<T, NotificationError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationChannel {
    InApp,
    Email,
    Slack,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationStatus {
    Pending,
    Sent,
    Failed,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub channel: NotificationChannel,
    pub status: NotificationStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Notification {
    pub fn new(
        user_id: Uuid,
        notification_type: NotificationType,
        title: String,
        message: String,
        channel: NotificationChannel,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            notification_type,
            title,
            message,
            channel,
            status: NotificationStatus::Pending,
            created_at: chrono::Utc::now(),
            sent_at: None,
            read_at: None,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub user_id: Uuid,
    pub in_app_enabled: bool,
    pub email_enabled: bool,
    pub slack_enabled: bool,
    pub webhook_enabled: bool,
    pub notification_types: Vec<NotificationType>,
}

impl NotificationPreferences {
    pub fn default_for_user(user_id: Uuid) -> Self {
        Self {
            user_id,
            in_app_enabled: true,
            email_enabled: true,
            slack_enabled: false,
            webhook_enabled: false,
            notification_types: vec![
                NotificationType::Info,
                NotificationType::Success,
                NotificationType::Warning,
                NotificationType::Error,
                NotificationType::System,
            ],
        }
    }
}

pub struct NotificationService {
    pool: PgPool,
}

impl NotificationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    /// 创建通知
    pub async fn create_notification(&self, notification: Notification) -> NotificationResult<Uuid> {
        let id = sqlx::query_scalar(
            r#"
            INSERT INTO notifications 
            (id, user_id, notification_type, title, message, channel, status,
             created_at, sent_at, read_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING i        "#
        )
        .bind(&notification.id)
        .bind(&notification.user_id)
        .bind(format!("{:?}", notification.notification_type))
        .bind(&notification.title)
        .bind(&notification.message)
        .bind(format!("{:?}", notification.channel))
        .bind(format!("{:?}", notification.status))
        .bind(&notification.created_at)
        .bind(&notification.sent_at)
        .bind(&notification.read_at)
        .bind(serde_json::to_value(&notification.metadata).unwrap())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    /// 获取通知
    pub async fn get_notification(&self, id: Uuid) -> NotificationResult<Notification> {
        let row = sqlx::query(
            r#"
            SELECT id, user_id, notification_type, title, message, channel, status,
                   created_at, sent_at, read_at, metadata
            FROM notifications
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(Notification {
            id: row.get("id"),
            user_id: row.get("user_id"),
            notification_type: parse_type(row.get("notification_type")),
            title: row.get("title"),
            message: row.get("message"),
            channel: parse_channel(row.get("channel")),
            status: parse_status(row.get("status")),
            created_at: row.get("created_at"),
            sent_at: row.get("sent_at"),
            read_at: row.get("read_at"),
            metadata: serde_json::from_value(row.get("metadata")).unwrap_or_default(),
        })
    }
    
    /// 列出用户通知
    pub async fn list_user_notifications(
        &self,
        user_id: Uuid,
        unread_only: bool,
        limit: Option<i64>,
    ) -> NotificationResult<Vec<Notification>> {
        let mut sql = String::from(
            "SELECT id, user_id, notification_type, title, message, channel, status,
                    created_at, sent_at, read_at, metadata 
             FROM notifications WHERE user_id = $1"
        );
        
        if unread_only {
            sql.push_str(" AND read_at IS NULL");
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        
        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        
        let rows = sqlx::query(&sql)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        
        let notifications = rows.into_iter().map(|row| {
            Notification {
                id: row.get("id"),
                user_id: row.get("user_id"),
                notification_type: parse_type(row.get("notification_type")),
                title: row.get("title"),
                message: row.get("message"),
                channel: parse_channel(row.get("channel")),
                status: parse_status(row.get("status")),
                created_at: row.get("created_at"),
                sent_at: row.get("sent_at"),
                read_at: row.get("read_at"),
                metadata: serde_json::from_value(row.get("metadata")).unwrap_or_default(),
            }
        }).collect();
        
        Ok(notifications)
    }
    
    /// 标记为已读
    pub async fn mark_as_read(&self, id: Uuid) -> NotificationResult<()> {
        sqlx::query(
            r#"
            UPDATE notifications 
            SET status = 'Read', read_at = $1
            WHERE id = $2
            "#
        )
        .bind(chrono::Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// 批量标记为已读
    pub async fn mark_all_as_read(&self, user_id: Uuid) -> NotificationResult<()> {
        sqlx::query(
            r#"
            UPDATE notifications 
            SET status = 'Read', read_at = $1
            WHERE user_id = $2 AND read_at IS NULL
            "#
        )
        .bind(chrono::Utc::now())
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// 删除通知
    pub async fn delete_notification(&self, id: Uuid) -> NotificationResult<()> {
        sqlx::query("DELETE FROM notifications WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    /// 获取未读数量
    pub async fn get_unread_count(&self, user_id: Uuid) -> NotificationResult<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count)
    }
    
    /// 发送通知
    pub async fn send_notification(&self, id: Uuid) -> NotificationResult<()> {
        let notification = self.get_notification(id).await?;
        
        // 根据渠道发送通知
        match notification.channel {
            NotificationChannel::InApp => {
                // InApp通知已经创建完成
            }
            NotificationChannel::Email => {
                // TODO: 实现邮件发送
            }
            NotificationChannel::Slack => {
                // TODO: 实现Slack发送
            }
            NotificationChannel::Webhook => {
                // TODO: 实现Webhook发送
            }
        }
        
        // 更新状态
        sqlx::query(
            r#"
            UPDATE notifications 
            SET status = 'Sent', sent_at = $1
            WHERE id = $2
            "#
        )
        .bind(chrono::Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// 获取用户偏好设置
    pub async fn get_preferences(&self, user_id: Uuid) -> NotificationResult<NotificationPreferences> {
        match sqlx::query(
            r#"
            SELECT user_id, in_app_enabled, email_enabled, slack_enabled, webhook_enabled, notification_types
            FROM notification_preferences
            WHERE user_id = $1
            "#
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await? {
            Some(row) => Ok(NotificationPreferences {
                user_id: row.get("user_id"),
                in_app_enabled: row.get("in_app_enabled"),
                email_enabled: row.get("email_enabled"),
                slack_enabled: row.get("slack_enabled"),
                webhook_enabled: row.get("webhook_enabled"),
                notification_types: serde_json::from_value(row.get("notification_types")).unwrap_or_default(),
            }),
            None => Ok(NotificationPreferences::default_for_user(user_id)),
        }
    }
    
    /// 更新用户偏好设置
    pub async fn update_preferences(&self, prefs: NotificationPreferences) -> NotificationResult<()> {
        sqlx::query(
            r#"
            INSERT INTO notification_preferences 
            (user_id, in_app_enabled, email_enabled, slack_enabled, webhook_enabled, notification_types)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id) DO UPDATE SET
                in_app_enabled = EXCLUDED.in_app_enabled,
                email_enabled = EXCLUDED.email_enabled,
                slack_enabled = EXCLUDED.slack_enabled,
                webhook_enabled = EXCLUDED.webhook_enabled,
                notification_types = EXCLUDED.notification_types
            "#
        )
        .bind(&prefs.user_id)
        .bind(prefs.in_app_enabled)
        .bind(prefs.email_enabled)
        .bind(prefs.slack_enabled)
        .bind(prefs.webhook_enabled)
        .bind(serde_json::to_value(&prefs.notification_types).unwrap())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}

fn parse_type(s: &str) -> NotificationType {
    match s {
        "Info" => NotificationType::Info,
        "Success" => NotificationType::Success,
        "Warning" => NotificationType::Warning,
        "Error" => NotificationType::Error,
        "System" => NotificationType::System,
        _ => NotificationType::Info,
    }
}

fn parse_channel(s: &str) -> NotificationChannel {
    match s {
        "InApp" => NotificationChannel::InApp,
        "Email" => NotificationChannel::Email,
        "Slack" => NotificationChannel::Slack,
        "Webhook" => NotificationChannel::Webhook,
        _ => NotificationChannel::InApp,
    }
}

fn parse_status(s: &str) -> NotificationStatus {
    match s {
        "Pending" => NotificationStatus::Pending,
        "Sent" => NotificationStatus::Sent,
        "Failed" => NotificationStatus::Failed,
        "Read" => NotificationStatus::Read,
        _ => NotificationStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_notification_creation() {
        let user_id = Uuid::new_v4();
        let notification = Notification::new(
            user_id,
            NotificationType::Info,
            "Test Notification".to_string(),
            "This is a test message".to_string(),
            NotificationChannel::InApp,
        );
        
        assert_eq!(notification.user_id, user_id);
        assert_eq!(notification.notification_type, NotificationType::Info);
        assert_eq!(notification.channel, NotificationChannel::InApp);
        assert_eq!(notification.status, NotificationStatus::Pending);
    }
    
    #[test]
    fn test_default_preferences() {
        let user_id = Uuid::new_v4();
        let prefs = NotificationPreferences::default_for_user(user_id);
        
        assert_eq!(prefs.user_id, user_id);
        assert!(prefs.in_app_enabled);
        assert!(prefs.email_enabled);
        assert!(!prefs.slack_enabled);
        assert!(!prefs.webhook_enabled);
        assert_eq!(prefs.notification_types.len(), 5);
    }
}
