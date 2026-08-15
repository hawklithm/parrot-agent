/// Agent Start Lock Service
/// 
/// Agent并发启动控制

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum AgentStartLockError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("lock acquisition failed")]
    LockFailed,
    
    #[error("deadlock detected")]
    Deadlock,
}

pub type AgentStartLockResult<T> = Result<T, AgentStartLockError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartLock {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub holder: String,
}

pub struct AgentStartLockService {
    pool: PgPool,
    lock_timeout: Duration,
}

impl AgentStartLockService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            lock_timeout: Duration::from_secs(30),
        }
    }
    
    pub async fn acquire_lock(
        &self,
        agent_id: Uuid,
        holder: String,
    ) -> AgentStartLockResult<Uuid> {
        let lock_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.lock_timeout.as_secs() as i64);
        
        // 尝试获取锁
        let result = sqlx::query(
            r#"
            INSERT INTO agent_start_locks 
            (id, agent_id, acquired_at, expires_at, holder)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (agent_id) 
            DO NOTHING
            "#
        )
        .bind(lock_id)
        .bind(agent_id)
        .bind(now)
        .bind(expires_at)
        .bind(&holder)
        .execute(&self.pool)
        .await?;
        
        if result.rows_affected() == 0 {
            return Err(AgentStartLockError::LockFailed);
        }
        
        Ok(lock_id)
    }
    
    pub async fn release_lock(&self, lock_id: Uuid) -> AgentStartLockResult<()> {
        sqlx::query("DELETE FROM agent_start_locks WHERE id = $1")
            .bind(lock_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    pub async fn cleanup_expired_locks(&self) -> AgentStartLockResult<i64> {
        let result = sqlx::query(
            "DELETE FROM agent_start_locks WHERE expires_at < $1"
        )
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() as i64)
    }
    
    pub async fn detect_deadlocks(&self) -> AgentStartLockResult<Vec<Uuid>> {
        // 简化实现：查找长时间持有的锁
        let rows = sqlx::query(
            r#"
            SELECT id FROM agent_start_locks 
            WHERE acquired_at < $1
            "#
        )
        .bind(chrono::Utc::now() - chrono::Duration::minutes(5))
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows.into_iter().map(|r| r.get("id")).collect())
    }
}
