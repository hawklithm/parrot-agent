/// Issue Liveness Service
/// 
/// Issue活跃度检测和超时管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum IssueLivenessError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type IssueLivenessResult<T> = Result<T, IssueLivenessError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LivenessStatus {
    Active,
    Idle,
    Stale,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueLivenessCheck {
    pub issue_id: Uuid,
    pub status: LivenessStatus,
    pub last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    pub idle_duration_seconds: i64,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

pub struct IssueLivenessService {
    pool: PgPool,
    idle_threshold_seconds: i64,
    stale_threshold_seconds: i64,
    abandoned_threshold_seconds: i64,
}

impl IssueLivenessService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            idle_threshold_seconds: 300,      // 5分钟
            stale_threshold_seconds: 3600,    // 1小时
            abandoned_threshold_seconds: 86400, // 24小时
        }
    }
    
    pub fn with_thresholds(
        mut self,
        idle_secs: i64,
        stale_secs: i64,
        abandoned_secs: i64,
    ) -> Self {
        self.idle_threshold_seconds = idle_secs;
        self.stale_threshold_seconds = stale_secs;
        self.abandoned_threshold_seconds = abandoned_secs;
        self
    }
    
    pub async fn check_liveness(&self, issue_id: Uuid) -> IssueLivenessResult<IssueLivenessCheck> {
        let row = sqlx::query(
            r#"
            SELECT id, last_activity_at, status
            FROM issues
            WHERE id = $1
            "#
        )
        .bind(issue_id)
        .fetch_one(&self.pool)
        .await?;
        
        let last_activity: Option<chrono::DateTime<chrono::Utc>> = row.get("last_activity_at");
        let issue_status: String = row.get("status");
        
        let now = chrono::Utc::now();
        let idle_duration = if let Some(last) = last_activity {
            (now - last).num_seconds()
        } else {
            i64::MAX
        };
        
        let status = if issue_status == "completed" || issue_status == "closed" {
            LivenessStatus::Active // 已完成的Issue视为正常
        } else if idle_duration > self.abandoned_threshold_seconds {
            LivenessStatus::Abandoned
        } else if idle_duration > self.stale_threshold_seconds {
            LivenessStatus::Stale
        } else if idle_duration > self.idle_threshold_seconds {
            LivenessStatus::Idle
        } else {
            LivenessStatus::Active
        };
        
        Ok(IssueLivenessCheck {
            issue_id,
            status,
            last_activity_at: last_activity,
            idle_duration_seconds: idle_duration,
            checked_at: now,
        })
    }
    
    pub async fn check_all_active_issues(&self) -> IssueLivenessResult<Vec<IssueLivenessCheck>> {
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM issues
            WHERE status NOT IN ('completed', 'closed')
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut checks = Vec::new();
        
        for row in rows {
            let issue_id: Uuid = row.get("id");
            if let Ok(check) = self.check_liveness(issue_id).await {
                checks.push(check);
            }
        }
        
        Ok(checks)
    }
    
    pub async fn get_stale_issues(&self) -> IssueLivenessResult<Vec<Uuid>> {
        let checks = self.check_all_active_issues().await?;
        
        Ok(checks.into_iter()
            .filter(|c| c.status == LivenessStatus::Stale || c.status == LivenessStatus::Abandoned)
            .map(|c| c.issue_id)
            .collect())
    }
    
    pub async fn update_activity(&self, issue_id: Uuid) -> IssueLivenessResult<()> {
        sqlx::query(
            r#"
            UPDATE issues 
            SET last_activity_at = $1
            WHERE id = $2
            "#
        )
        .bind(chrono::Utc::now())
        .bind(issue_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_liveness_stats(&self) -> IssueLivenessResult<HashMap<LivenessStatus, usize>> {
        let checks = self.check_all_active_issues().await?;
        
        let mut stats = HashMap::new();
        stats.insert(LivenessStatus::Active, 0);
        stats.insert(LivenessStatus::Idle, 0);
        stats.insert(LivenessStatus::Stale, 0);
        stats.insert(LivenessStatus::Abandoned, 0);
        
        for check in checks {
            *stats.entry(check.status).or_insert(0) += 1;
        }
        
        Ok(stats)
    }
}
