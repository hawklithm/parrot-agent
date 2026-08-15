/// Issue Rewake Throttle Service
/// 
/// Issue 重新唤醒限流管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IssueRewakeThrottleError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("throttled: {0}")]
    Throttled(String),
}

pub type IssueRewakeThrottleResult<T> = Result<T, IssueRewakeThrottleError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewakeAttempt {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub attempted_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub allowed: bool,
}

pub struct IssueRewakeThrottleService {
    pool: PgPool,
    max_rewakes_per_hour: usize,
    min_interval_seconds: i64,
}

impl IssueRewakeThrottleService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            max_rewakes_per_hour: 5,
            min_interval_seconds: 60, // 1分钟
        }
    }
    
    pub fn with_limits(
        mut self,
        max_per_hour: usize,
        min_interval_secs: i64,
    ) -> Self {
        self.max_rewakes_per_hour = max_per_hour;
        self.min_interval_seconds = min_interval_secs;
        self
    }
    
    pub async fn can_rewake(&self, issue_id: Uuid) -> IssueRewakeThrottleResult<bool> {
        // 检查最近1小时的rewake次数
        let one_hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
        
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM rewake_attempts
            WHERE issue_id = $1 
              AND attempted_at > $2
              AND allowed = true
            "#
        )
        .bind(issue_id)
        .bind(one_hour_ago)
        .fetch_one(&self.pool)
        .await?;
        
        if count >= self.max_rewakes_per_hour as i64 {
            return Ok(false);
        }
        
        // 检查最近一次rewake的时间间隔
        let last_rewake: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            r#"
            SELECT attempted_at
            FROM rewake_attempts
            WHERE issue_id = $1 AND allowed = true
            ORDER BY attempted_at DESC
            LIMIT 1
            "#
        )
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(last) = last_rewake {
            let elapsed = (chrono::Utc::now() - last).num_seconds();
            if elapsed < self.min_interval_seconds {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
    
    pub async fn record_rewake_attempt(
        &self,
        issue_id: Uuid,
        reason: String,
    ) -> IssueRewakeThrottleResult<Uuid> {
        let allowed = self.can_rewake(issue_id).await?;
        
        if !allowed {
            return Err(IssueRewakeThrottleError::Throttled(
                format!("Issue {} is throttled", issue_id)
            ));
        }
        
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO rewake_attempts 
            (id, issue_id, attempted_at, reason, allowed)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(issue_id)
        .bind(chrono::Utc::now())
        .bind(&reason)
        .bind(allowed)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_rewake_history(&self, issue_id: Uuid) -> IssueRewakeThrottleResult<Vec<RewakeAttempt>> {
        let rows = sqlx::query(
            r#"
            SELECT id, issue_id, attempted_at, reason, allowed
            FROM rewake_attempts
            WHERE issue_id = $1
            ORDER BY attempted_at DESC
            LIMIT 100
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;
        
        let attempts = rows.into_iter().map(|row| {
            RewakeAttempt {
                id: row.get("id"),
                issue_id: row.get("issue_id"),
                attempted_at: row.get("attempted_at"),
                reason: row.get("reason"),
                allowed: row.get("allowed"),
            }
        }).collect();
        
        Ok(attempts)
    }
    
    pub async fn reset_throttle(&self, issue_id: Uuid) -> IssueRewakeThrottleResult<()> {
        sqlx::query(
            r#"
            DELETE FROM rewake_attempts
            WHERE issue_id = $1
            "#
        )
        .bind(issue_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
