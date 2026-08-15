/// Run Liveness Service
/// 
/// Run活跃度检测和超时管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RunLivenessError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type RunLivenessResult<T> = Result<T, RunLivenessError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RunLivenessStatus {
    Running,
    Idle,
    Stalled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLivenessCheck {
    pub run_id: Uuid,
    pub status: RunLivenessStatus,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub idle_duration_seconds: i64,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

pub struct RunLivenessService {
    pool: PgPool,
    idle_threshold_seconds: i64,
    stalled_threshold_seconds: i64,
    timeout_threshold_seconds: i64,
}

impl RunLivenessService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            idle_threshold_seconds: 60,      // 1分钟
            stalled_threshold_seconds: 300,  // 5分钟
            timeout_threshold_seconds: 3600, // 1小时
        }
    }
    
    pub fn with_thresholds(
        mut self,
        idle_secs: i64,
        stalled_secs: i64,
        timeout_secs: i64,
    ) -> Self {
        self.idle_threshold_seconds = idle_secs;
        self.stalled_threshold_seconds = stalled_secs;
        self.timeout_threshold_seconds = timeout_secs;
        self
    }
    
    pub async fn check_liveness(&self, run_id: Uuid) -> RunLivenessResult<RunLivenessCheck> {
        let row = sqlx::query(
            r#"
            SELECT id, last_heartbeat_at, status
            FROM runs
            WHERE id = $1
            "#
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await?;
        
        let last_heartbeat: Option<chrono::DateTime<chrono::Utc>> = row.get("last_heartbeat_at");
        let run_status: String = row.get("status");
        
        let now = chrono::Utc::now();
        let idle_duration = if let Some(last) = last_heartbeat {
            (now - last).num_seconds()
        } else {
            i64::MAX
        };
        
        let status = if run_status == "completed" || run_status == "failed" {
            RunLivenessStatus::Running // 已结束的Run视为正常
        } else if idle_duration > self.timeout_threshold_seconds {
            RunLivenessStatus::TimedOut
        } else if idle_duration > self.stalled_threshold_seconds {
            RunLivenessStatus::Stalled
        } else if idle_duration > self.idle_threshold_seconds {
            RunLivenessStatus::Idle
        } else {
            RunLivenessStatus::Running
        };
        
        Ok(RunLivenessCheck {
            run_id,
            status,
            last_heartbeat_at: last_heartbeat,
            idle_duration_seconds: idle_duration,
            checked_at: now,
        })
    }
    
    pub async fn check_all_active_runs(&self) -> RunLivenessResult<Vec<RunLivenessCheck>> {
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM runs
            WHERE status IN ('running', 'pending')
            "#
        )
        .fetch_all(&self.pool)
        .await?;
        
        let mut checks = Vec::new();
        
        for row in rows {
            let run_id: Uuid = row.get("id");
            if let Ok(check) = self.check_liveness(run_id).await {
                checks.push(check);
            }
        }
        
        Ok(checks)
    }
    
    pub async fn get_stalled_runs(&self) -> RunLivenessResult<Vec<Uuid>> {
        let checks = self.check_all_active_runs().await?;
        
        Ok(checks.into_iter()
            .filter(|c| c.status == RunLivenessStatus::Stalled || c.status == RunLivenessStatus::TimedOut)
            .map(|c| c.run_id)
            .collect())
    }
    
    pub async fn update_heartbeat(&self, run_id: Uuid) -> RunLivenessResult<()> {
        sqlx::query(
            r#"
            UPDATE runs 
            SET last_heartbeat_at = $1
            WHERE id = $2
            "#
        )
        .bind(chrono::Utc::now())
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn mark_as_timed_out(&self, run_id: Uuid) -> RunLivenessResult<()> {
        sqlx::query(
            r#"
            UPDATE runs 
            SET status = 'timed_out', completed_at = $1
            WHERE id = $2
            "#
        )
        .bind(chrono::Utc::now())
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_liveness_stats(&self) -> RunLivenessResult<HashMap<RunLivenessStatus, usize>> {
        let checks = self.check_all_active_runs().await?;
        
        let mut stats = HashMap::new();
        stats.insert(RunLivenessStatus::Running, 0);
        stats.insert(RunLivenessStatus::Idle, 0);
        stats.insert(RunLivenessStatus::Stalled, 0);
        stats.insert(RunLivenessStatus::TimedOut, 0);
        
        for check in checks {
            *stats.entry(check.status).or_insert(0) += 1;
        }
        
        Ok(stats)
    }
}
