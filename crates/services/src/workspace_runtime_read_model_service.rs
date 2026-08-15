/// Workspace Runtime Read Model Service
/// 
/// Workspace 运行时读模型

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeReadModelError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type RuntimeReadModelResult<T> = Result<T, RuntimeReadModelError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRuntimeState {
    pub workspace_id: Uuid,
    pub status: String,
    pub active_processes: i32,
    pub resource_usage: ResourceUsage,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_mb: i64,
    pub disk_mb: i64,
}

pub struct WorkspaceRuntimeReadModelService {
    pool: PgPool,
}

impl WorkspaceRuntimeReadModelService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn get_runtime_state(
        &self,
        workspace_id: Uuid,
    ) -> RuntimeReadModelResult<Option<WorkspaceRuntimeState>> {
        let row = sqlx::query(
            r#"
            SELECT workspace_id, status, active_processes, 
                   cpu_percent, memory_mb, disk_mb, last_updated
            FROM workspace_runtime_states
            WHERE workspace_id = $1
            "#
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| WorkspaceRuntimeState {
            workspace_id: r.get("workspace_id"),
            status: r.get("status"),
            active_processes: r.get("active_processes"),
            resource_usage: ResourceUsage {
                cpu_percent: r.get("cpu_percent"),
                memory_mb: r.get("memory_mb"),
                disk_mb: r.get("disk_mb"),
            },
            last_updated: r.get("last_updated"),
        }))
    }
    
    pub async fn update_runtime_state(
        &self,
        workspace_id: Uuid,
        status: String,
        active_processes: i32,
        resource_usage: ResourceUsage,
    ) -> RuntimeReadModelResult<()> {
        sqlx::query(
            r#"
            INSERT INTO workspace_runtime_states 
            (workspace_id, status, active_processes, cpu_percent, memory_mb, disk_mb, last_updated)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (workspace_id)
            DO UPDATE SET status = $2, active_processes = $3, 
                         cpu_percent = $4, memory_mb = $5, disk_mb = $6, last_updated = $7
            "#
        )
        .bind(workspace_id)
        .bind(&status)
        .bind(active_processes)
        .bind(resource_usage.cpu_percent)
        .bind(resource_usage.memory_mb)
        .bind(resource_usage.disk_mb)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
}
