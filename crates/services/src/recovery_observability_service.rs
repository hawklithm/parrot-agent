use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RecoveryObservabilityError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("recovery not found: {0}")]
    NotFound(Uuid),
}

pub type RecoveryResult<T> = Result<T, RecoveryObservabilityError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RecoveryEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub snapshot_id: Uuid,
    pub recovery_type: RecoveryType,
    pub status: RecoveryStatus,
    pub error_message: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum RecoveryType {
    Full,
    Partial,
    FileLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum RecoveryStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[async_trait]
pub trait RecoveryObservabilityService: Send + Sync {
    async fn record_recovery_start(
        &self,
        workspace_id: Uuid,
        snapshot_id: Uuid,
        recovery_type: RecoveryType,
    ) -> RecoveryResult<Uuid>;
    
    async fn record_recovery_completion(
        &self,
        recovery_id: Uuid,
        status: RecoveryStatus,
        error_message: Option<String>,
    ) -> RecoveryResult<()>;
    
    async fn get_recovery_history(&self, workspace_id: Uuid) -> RecoveryResult<Vec<RecoveryEvent>>;
    async fn get_recovery_metrics(&self) -> RecoveryResult<RecoveryMetrics>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryMetrics {
    pub total_recoveries: i64,
    pub successful_recoveries: i64,
    pub failed_recoveries: i64,
    pub average_recovery_time_seconds: f64,
}

pub struct RecoveryObservabilityServiceImpl {
    pool: PgPool,
}

impl RecoveryObservabilityServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecoveryObservabilityService for RecoveryObservabilityServiceImpl {
    async fn record_recovery_start(
        &self,
        workspace_id: Uuid,
        snapshot_id: Uuid,
        recovery_type: RecoveryType,
    ) -> RecoveryResult<Uuid> {
        let recovery_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO recovery_events (id, workspace_id, snapshot_id, recovery_type, status, started_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#
        )
        .bind(recovery_id)
        .bind(workspace_id)
        .bind(snapshot_id)
        .bind(serde_json::to_value(&recovery_type).unwrap())
        .bind(serde_json::to_value(&RecoveryStatus::InProgress).unwrap())
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(recovery_id)
    }
    
    async fn record_recovery_completion(
        &self,
        recovery_id: Uuid,
        status: RecoveryStatus,
        error_message: Option<String>,
    ) -> RecoveryResult<()> {
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            UPDATE recovery_events
            SET status = $1, error_message = $2, completed_at = $3
            WHERE id = $4
            "#
        )
        .bind(serde_json::to_value(&status).unwrap())
        .bind(error_message)
        .bind(now)
        .bind(recovery_id)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    async fn get_recovery_history(&self, workspace_id: Uuid) -> RecoveryResult<Vec<RecoveryEvent>> {
        let rows = sqlx::query_as::<_, RecoveryEvent>(
            r#"
            SELECT id, workspace_id, snapshot_id, recovery_type, status, error_message, started_at, completed_at
            FROM recovery_events
            WHERE workspace_id = $1
            ORDER BY started_at DESC
            LIMIT 100
            "#
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn get_recovery_metrics(&self) -> RecoveryResult<RecoveryMetrics> {
        let row: (i64, i64, i64, Option<f64>) = sqlx::query_as(
            r#"
            SELECT 
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE status = 'completed') as successful,
                COUNT(*) FILTER (WHERE status = 'failed') as failed,
                AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_time
            FROM recovery_events
            WHERE started_at > NOW() - INTERVAL '30 days'
            "#
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(RecoveryMetrics {
            total_recoveries: row.0,
            successful_recoveries: row.1,
            failed_recoveries: row.2,
            average_recovery_time_seconds: row.3.unwrap_or(0.0),
        })
    }
}
