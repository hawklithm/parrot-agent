/// Workspace Operation Log Store Service
/// 
/// Workspace 操作日志存储

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum OperationLogError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type OperationLogResult<T> = Result<T, OperationLogError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub operation_type: String,
    pub details: serde_json::Value,
    pub performed_by: Uuid,
    pub performed_at: chrono::DateTime<chrono::Utc>,
}

pub struct WorkspaceOperationLogStoreService {
    pool: PgPool,
}

impl WorkspaceOperationLogStoreService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn log_operation(
        &self,
        workspace_id: Uuid,
        operation_type: String,
        details: serde_json::Value,
        performed_by: Uuid,
    ) -> OperationLogResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO workspace_operation_logs 
            (id, workspace_id, operation_type, details, performed_by, performed_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(workspace_id)
        .bind(&operation_type)
        .bind(&details)
        .bind(performed_by)
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn get_logs(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> OperationLogResult<Vec<OperationLog>> {
        let rows = sqlx::query(
            r#"
            SELECT id, workspace_id, operation_type, details, performed_by, performed_at
            FROM workspace_operation_logs
            WHERE workspace_id = $1
            ORDER BY performed_at DESC
            LIMIT $2
            "#
        )
        .bind(workspace_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        
        let logs = rows.into_iter().map(|row| {
            OperationLog {
                id: row.get("id"),
                workspace_id: row.get("workspace_id"),
                operation_type: row.get("operation_type"),
                details: row.get("details"),
                performed_by: row.get("performed_by"),
                performed_at: row.get("performed_at"),
            }
        }).collect();
        
        Ok(logs)
    }
}
