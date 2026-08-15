/// Invite Rate Limit Service
/// 
/// 邀请限流管理

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InviteRateLimitError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("rate limit exceeded: {0}")]
    RateLimitExceeded(String),
}

pub type InviteRateLimitResult<T> = Result<T, InviteRateLimitError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_invites_per_hour: i32,
    pub max_invites_per_day: i32,
    pub max_invites_per_month: i32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_invites_per_hour: 10,
            max_invites_per_day: 50,
            max_invites_per_month: 200,
        }
    }
}

pub struct InviteRateLimitService {
    pool: PgPool,
    config: RateLimitConfig,
}

impl InviteRateLimitService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            config: RateLimitConfig::default(),
        }
    }
    
    pub fn with_config(mut self, config: RateLimitConfig) -> Self {
        self.config = config;
        self
    }
    
    pub async fn check_rate_limit(&self, user_id: Uuid) -> InviteRateLimitResult<bool> {
        let now = chrono::Utc::now();
        let one_hour_ago = now - chrono::Duration::hours(1);
        let one_day_ago = now - chrono::Duration::days(1);
        let one_month_ago = now - chrono::Duration::days(30);
        
        // 检查每小时限制
        let hourly: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invites WHERE inviter_id = $1 AND created_at > $2"
        )
        .bind(user_id)
        .bind(one_hour_ago)
        .fetch_one(&self.pool)
        .await?;
        
        if hourly >= self.config.max_invites_per_hour as i64 {
            return Err(InviteRateLimitError::RateLimitExceeded(
                format!("Hourly limit ({}) exceeded", self.config.max_invites_per_hour)
            ));
        }
        
        // 检查每日限制
        let daily: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invites WHERE inviter_id = $1 AND created_at > $2"
        )
        .bind(user_id)
        .bind(one_day_ago)
        .fetch_one(&self.pool)
        .await?;
        
        if daily >= self.config.max_invites_per_day as i64 {
            return Err(InviteRateLimitError::RateLimitExceeded(
                format!("Daily limit ({}) exceeded", self.config.max_invites_per_day)
            ));
        }
        
        // 检查每月限制
        let monthly: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM invites WHERE inviter_id = $1 AND created_at > $2"
        )
        .bind(user_id)
        .bind(one_month_ago)
        .fetch_one(&self.pool)
        .await?;
        
        if monthly >= self.config.max_invites_per_month as i64 {
            return Err(InviteRateLimitError::RateLimitExceeded(
                format!("Monthly limit ({}) exceeded", self.config.max_invites_per_month)
            ));
        }
        
        Ok(true)
    }
    
    pub async fn record_invite(&self, user_id: Uuid) -> InviteRateLimitResult<()> {
        self.check_rate_limit(user_id).await?;
        
        sqlx::query(
            "INSERT INTO invite_rate_limits (id, user_id, created_at) VALUES ($1, $2, $3)"
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
