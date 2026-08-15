/// Environment Custom Image Runtime Service
/// 
/// 自定义镜像运行时管理

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CustomImageRuntimeError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("runtime error: {0}")]
    RuntimeError(String),
}

pub type CustomImageRuntimeResult<T> = Result<T, CustomImageRuntimeError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomImageRuntime {
    pub id: Uuid,
    pub image_id: Uuid,
    pub container_id: String,
    pub status: RuntimeStatus,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub stopped_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

pub struct EnvironmentCustomImageRuntimeService {
    pool: PgPool,
}

impl EnvironmentCustomImageRuntimeService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn start_runtime(
        &self,
        image_id: Uuid,
        container_id: String,
    ) -> CustomImageRuntimeResult<Uuid> {
        let id = Uuid::new_v4();
        
        let _result: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO custom_image_runtimes 
            (id, image_id, container_id, status, started_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(id)
        .bind(image_id)
        .bind(&container_id)
        .bind(format!("{:?}", RuntimeStatus::Starting))
        .bind(chrono::Utc::now())
        .fetch_one(&self.pool)
        .await?;
        
        // 实际应该启动Docker容器
        
        sqlx::query(
            "UPDATE custom_image_runtimes SET status = $1 WHERE id = $2"
        )
        .bind(format!("{:?}", RuntimeStatus::Running))
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(id)
    }
    
    pub async fn stop_runtime(&self, id: Uuid) -> CustomImageRuntimeResult<()> {
        sqlx::query(
            r#"
            UPDATE custom_image_runtimes 
            SET status = $1, stopped_at = $2
            WHERE id = $3
            "#
        )
        .bind(format!("{:?}", RuntimeStatus::Stopped))
        .bind(chrono::Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    pub async fn get_runtime_status(
        &self,
        id: Uuid,
    ) -> CustomImageRuntimeResult<Option<CustomImageRuntime>> {
        let row = sqlx::query(
            r#"
            SELECT id, image_id, container_id, status, started_at, stopped_at
            FROM custom_image_runtimes
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row.map(|r| CustomImageRuntime {
            id: r.get("id"),
            image_id: r.get("image_id"),
            container_id: r.get("container_id"),
            status: parse_status(r.get("status")),
            started_at: r.get("started_at"),
            stopped_at: r.get("stopped_at"),
        }))
    }
}

fn parse_status(s: &str) -> RuntimeStatus {
    match s {
        "Starting" => RuntimeStatus::Starting,
        "Running" => RuntimeStatus::Running,
        "Stopping" => RuntimeStatus::Stopping,
        "Stopped" => RuntimeStatus::Stopped,
        "Failed" => RuntimeStatus::Failed,
        _ => RuntimeStatus::Stopped,
    }
}
