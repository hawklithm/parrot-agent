/// Workspace Instance Cleanup Service
/// 
/// Workspace 实例清理管理

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceCleanupError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("cleanup failed: {0}")]
    CleanupFailed(String),
}

pub type WorkspaceCleanupResult<T> = Result<T, WorkspaceCleanupError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupTask {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub cleanup_type: CleanupType,
    pub status: CleanupStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CleanupType {
    TempFiles,
    Logs,
    Cache,
    FullCleanup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CleanupStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct WorkspaceInstanceCleanupService {
    pool: PgPool,
    max_age_days: i64,
}

impl WorkspaceInstanceCleanupService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            max_age_days: 7,
        }
    }
    
    pub fn with_max_age_days(mut self, days: i64) -> Self {
        self.max_age_days = days;
        self
    }
    
    pub async fn schedule_cleanup(
        &self,
        workspace_id: Uuid,
        cleanup_type: CleanupType,
    ) -> WorkspaceCleanupResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: uuid::Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO workspace_cleanup_tasks 
            (id, workspace_id, cleanup_type, status, started_at, details)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(workspace_id)
        .bind(format!("{:?}", cleanup_type))
        .bind(format!("{:?}", CleanupStatus::Pending))
        .bind(chrono::Utc::now())
        .bind(serde_json::json!({}))
        .fetch_one(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn execute_cleanup(
        &self,
        task_id: Uuid,
    ) -> WorkspaceCleanupResult<()> {
        // 标记为运行中
        sqlx::query(
            r#"
            UPDATE workspace_cleanup_tasks 
            SET status = $1
            WHERE id = $2
            "#
        )
        .bind(format!("{:?}", CleanupStatus::Running))
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        
        // 执行清理（简化实现）
        // 实际实现应该调用文件系统操作
        
        // 标记为完成
        sqlx::query(
            r#"
            UPDATE workspace_cleanup_tasks 
            SET status = $1, completed_at = $2
            WHERE id = $3
            "#
        )
        .bind(format!("{:?}", CleanupStatus::Completed))
        .bind(chrono::Utc::now())
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn cleanup_old_instances(&self) -> WorkspaceCleanupResult<Vec<Uuid>> {
        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(self.max_age_days);
        
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM workspaces
            WHERE last_accessed_at < $1
              AND status = 'inactive'
            "#
        )
        .bind(cutoff_date)
        .fetch_all(&self.pool)
        .await?;
        
        let mut cleaned = Vec::new();
        
        for row in rows {
            let workspace_id: Uuid = row.get("id");
            let task_id = self.schedule_cleanup(workspace_id, CleanupType::FullCleanup).await?;
            self.execute_cleanup(task_id).await?;
            cleaned.push(workspace_id);
        }
        
        Ok(cleaned)
    }
    
    pub async fn get_cleanup_status(
        &self,
        task_id: Uuid,
    ) -> WorkspaceCleanupResult<CleanupTask> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, cleanup_type, status, started_at, completed_at, details
            FROM workspace_cleanup_tasks
            WHERE id = $1
            "#
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(CleanupTask {
            id: row.get("id"),
            workspace_id: row.get("workspace_id"),
            cleanup_type: parse_cleanup_type(row.get("cleanup_type")),
            status: parse_cleanup_status(row.get("status")),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            details: row.get("details"),
        })
    }
}

fn parse_cleanup_type(s: &str) -> CleanupType {
    match s {
        "TempFiles" => CleanupType::TempFiles,
        "Logs" => CleanupType::Logs,
        "Cache" => CleanupType::Cache,
        "FullCleanup" => CleanupType::FullCleanup,
        _ => CleanupType::TempFiles,
    }
}

fn parse_cleanup_status(s: &str) -> CleanupStatus {
    match s {
        "Pending" => CleanupStatus::Pending,
        "Running" => CleanupStatus::Running,
        "Completed" => CleanupStatus::Completed,
        "Failed" => CleanupStatus::Failed,
        _ => CleanupStatus::Pending,
    }
}
