/// Routine Coordinator Service
/// 
/// Routine 协调和执行管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RoutineCoordinatorError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("routine not found: {0}")]
    NotFound(Uuid),
    
    #[error("coordination failed: {0}")]
    CoordinationFailed(String),
}

pub type RoutineCoordinatorResult<T> = Result<T, RoutineCoordinatorError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineExecution {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub status: ExecutionStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

pub struct RoutineCoordinatorService {
    pool: PgPool,
}

impl RoutineCoordinatorService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn schedule_execution(
        &self,
        routine_id: Uuid,
    ) -> RoutineCoordinatorResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO routine_executions 
            (id, routine_id, status, started_at)
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(routine_id)
        .bind(format!("{:?}", ExecutionStatus::Pending))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn start_execution(
        &self,
        execution_id: Uuid,
    ) -> RoutineCoordinatorResult<()> {
        sqlx::query(
            r#"
            UPDATE routine_executions 
            SET status = $1
            WHERE id = $2
            "#
        )
        .bind(format!("{:?}", ExecutionStatus::Running))
        .bind(execution_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn complete_execution(
        &self,
        execution_id: Uuid,
        result: serde_json::Value,
    ) -> RoutineCoordinatorResult<()> {
        sqlx::query(
            r#"
            UPDATE routine_executions 
            SET status = $1, completed_at = $2, result = $3
            WHERE id = $4
            "#
        )
        .bind(format!("{:?}", ExecutionStatus::Completed))
        .bind(chrono::Utc::now())
        .bind(&result)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn fail_execution(
        &self,
        execution_id: Uuid,
        error: String,
    ) -> RoutineCoordinatorResult<()> {
        sqlx::query(
            r#"
            UPDATE routine_executions 
            SET status = $1, completed_at = $2, result = $3
            WHERE id = $4
            "#
        )
        .bind(format!("{:?}", ExecutionStatus::Failed))
        .bind(chrono::Utc::now())
        .bind(serde_json::json!({"error": error}))
        .bind(execution_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> RoutineCoordinatorResult<RoutineExecution> {
        let row = sqlx::query(
            r#"
            SELECT id, routine_id, status, started_at, completed_at, result
            FROM routine_executions
            WHERE id = $1
            "#
        )
        .bind(execution_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(RoutineExecution {
            id: row.get("id"),
            routine_id: row.get("routine_id"),
            status: parse_status(row.get("status")),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            result: row.get("result"),
        })
    }
    
    pub async fn list_executions(
        &self,
        routine_id: Uuid,
        limit: i64,
    ) -> RoutineCoordinatorResult<Vec<RoutineExecution>> {
        let rows = sqlx::query(
            r#"
            SELECT id, routine_id, status, started_at, completed_at, result
            FROM routine_executions
            WHERE routine_id = $1
            ORDER BY started_at DESC
            LIMIT $2
            "#
        )
        .bind(routine_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        let executions = rows.into_iter().map(|row| {
            RoutineExecution {
                id: row.get("id"),
                routine_id: row.get("routine_id"),
                status: parse_status(row.get("status")),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                result: row.get("result"),
            }
        }).collect();
        
        Ok(executions)
    }
}

fn parse_status(s: &str) -> ExecutionStatus {
    match s {
        "Pending" => ExecutionStatus::Pending,
        "Running" => ExecutionStatus::Running,
        "Completed" => ExecutionStatus::Completed,
        "Failed" => ExecutionStatus::Failed,
        "Cancelled" => ExecutionStatus::Cancelled,
        _ => ExecutionStatus::Pending,
    }
}
