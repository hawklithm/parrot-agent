/// Run Continuations Service
/// 
/// Run继续执行管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RunContinuationsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("continuation not found: {0}")]
    NotFound(Uuid),
}

pub type RunContinuationsResult<T> = Result<T, RunContinuationsError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunContinuation {
    pub id: Uuid,
    pub run_id: Uuid,
    pub parent_run_id: Option<Uuid>,
    pub continuation_point: String,
    pub state_snapshot: serde_json::Value,
    pub reason: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct RunContinuationsService {
    pool: PgPool,
}

impl RunContinuationsService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create_continuation(
        &self,
        run_id: Uuid,
        parent_run_id: Option<Uuid>,
        continuation_point: String,
        state: serde_json::Value,
        reason: String,
    ) -> RunContinuationsResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO run_continuations 
            (id, run_id, parent_run_id, continuation_point, state_snapshot, reason, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(run_id)
        .bind(parent_run_id)
        .bind(&continuation_point)
        .bind(&state)
        .bind(&reason)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_continuation(&self, id: Uuid) -> RunContinuationsResult<RunContinuation> {
        let row = sqlx::query(
            r#"
            SELECT id, run_id, parent_run_id, continuation_point, state_snapshot, reason, created_at
            FROM run_continuations
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(RunContinuation {
            id: row.get("id"),
            run_id: row.get("run_id"),
            parent_run_id: row.get("parent_run_id"),
            continuation_point: row.get("continuation_point"),
            state_snapshot: row.get("state_snapshot"),
            reason: row.get("reason"),
            created_at: row.get("created_at"),
        })
    }
    
    pub async fn list_by_run(&self, run_id: Uuid) -> RunContinuationsResult<Vec<RunContinuation>> {
        let rows = sqlx::query(
            r#"
            SELECT id, run_id, parent_run_id, continuation_point, state_snapshot, reason, created_at
            FROM run_continuations
            WHERE run_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        
        let continuations = rows.into_iter().map(|row| {
            RunContinuation {
                id: row.get("id"),
                run_id: row.get("run_id"),
                parent_run_id: row.get("parent_run_id"),
                continuation_point: row.get("continuation_point"),
                state_snapshot: row.get("state_snapshot"),
                reason: row.get("reason"),
                created_at: row.get("created_at"),
            }
        }).collect();
        
        Ok(continuations)
    }
    
    pub async fn get_latest_continuation(&self, run_id: Uuid) -> RunContinuationsResult<Option<RunContinuation>> {
        let row = sqlx::query(
            r#"
            SELECT id, run_id, parent_run_id, continuation_point, state_snapshot, reason, created_at
            FROM run_continuations
            WHERE run_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| RunContinuation {
            id: r.get("id"),
            run_id: r.get("run_id"),
            parent_run_id: r.get("parent_run_id"),
            continuation_point: r.get("continuation_point"),
            state_snapshot: r.get("state_snapshot"),
            reason: r.get("reason"),
            created_at: r.get("created_at"),
        }))
    }
}
