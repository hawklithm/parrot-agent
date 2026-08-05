use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use uuid::Uuid;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Error)]
pub enum WorkspaceOperationError {
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),

    #[error("Operation not found: {0}")]
    OperationNotFound(Uuid),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type WorkspaceOperationResult<T> = Result<T, WorkspaceOperationError>;

/// Operation phase
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    WorkspaceProvision,
    WorkspaceTeardown,
    RuntimeStart,
    RuntimeStop,
    RuntimeRestart,
    CommandExecution,
    BranchReconcile,
}

impl std::fmt::Display for OperationPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationPhase::WorkspaceProvision => write!(f, "workspace_provision"),
            OperationPhase::WorkspaceTeardown => write!(f, "workspace_teardown"),
            OperationPhase::RuntimeStart => write!(f, "runtime_start"),
            OperationPhase::RuntimeStop => write!(f, "runtime_stop"),
            OperationPhase::RuntimeRestart => write!(f, "runtime_restart"),
            OperationPhase::CommandExecution => write!(f, "command_execution"),
            OperationPhase::BranchReconcile => write!(f, "branch_reconcile"),
        }
    }
}

/// Operation status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OperationStatus {
    InProgress,
    Completed,
    Failed,
}

impl std::fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationStatus::InProgress => write!(f, "in_progress"),
            OperationStatus::Completed => write!(f, "completed"),
            OperationStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Workspace operation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceOperation {
    pub id: Uuid,
    pub company_id: Uuid,
    pub execution_workspace_id: Uuid,
 pub phase: OperationPhase,
    pub command: Option<String>,
    pub status: OperationStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub metadata: Option<JsonValue>,
    pub error_message: Option<String>,
}

impl WorkspaceOperation {
    /// Calculate duration from started_at to completed_at
    pub fn calculate_duration(&self) -> Option<i64> {
        self.completed_at.map(|completed| {
            let duration = completed.signed_duration_since(self.started_at);
            duration.num_milliseconds()
        })
    }

    /// Check if operation is still in progress
    pub fn is_in_progress(&self) -> bool {
        self.status == OperationStatus::InProgress
    }

    /// Check if operation succeeded
    pub fn is_success(&self) -> bool {
        self.status == OperationStatus::Completed
    }

    /// Check if operation failed
    pub fn is_failed(&self) -> bool {
        self.status == OperationStatus::Failed
    }
}

/// Create operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOperationRequest {
    pub company_id: Uuid,
    pub execution_workspace_id: Uuid,
    pub phase: OperationPhase,
    pub command: Option<String>,
    pub metadata: Option<JsonValue>,
}

/// Complete operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteOperationRequest {
    pub operation_id: Uuid,
    pub success: bool,
    pub error_message: Option<String>,
}

/// List operations query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListOperationsQuery {
    pub execution_workspace_id: Uuid,
    pub phase: Option<OperationPhase>,
    pub status: Option<OperationStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Workspace operation service trait
#[async_trait]
pub trait WorkspaceOperationService: Send + Sync {
    /// Create a new operation recorder
    fn create_recorder(&self, company_id: Uuid, execution_workspace_id: Uuid) -> OperationRecorder;

    /// Record a workspace operation
    async fn record_operation(
        &self,
        request: CreateOperationRequest,
    ) -> WorkspaceOperationResult<WorkspaceOperation>;

    /// Complete an operation
    async fn complete_operation(
        &self,
        request: CompleteOperationRequest,
    ) -> WorkspaceOperationResult<WorkspaceOperation>;

    /// Get operation by ID
    async fn get_operation(&self, operation_id: Uuid) -> WorkspaceOperationResult<WorkspaceOperation>;

    /// List operations for a workspace
    async fn list_operations(
        &self,
        query: ListOperationsQuery,
    ) -> WorkspaceOperationResult<Vec<WorkspaceOperation>>;

    /// Get operation statistics
    async fn get_statistics(
        &self,
        execution_workspace_id: Uuid,
        phase: Option<OperationPhase>,
    ) -> WorkspaceOperationResult<OperationStatistics>;
}

/// Operation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStatistics {
    pub total_count: i64,
    pub completed_count: i64,
    pub failed_count: i64,
    pub in_progress_count: i64,
    pub avg_duration_ms: Option<f64>,
    pub p50_duration_ms: Option<i64>,
    pub p95_duration_ms: Option<i64>,
    pub p99_duration_ms: Option<i64>,
}

/// Operation recorder for convenient operation tracking
pub struct OperationRecorder {
    company_id: Uuid,
    execution_workspace_id: Uuid,
    current_operation_id: Option<Uuid>,
}

impl OperationRecorder {
    pub fn new(company_id: Uuid, execution_workspace_id: Uuid) -> Self {
        Self {
            company_id,
            execution_workspace_id,
            current_operation_id: None,
        }
    }

    /// Start recording an operation
    pub async fn start<S: WorkspaceOperationService>(
        &mut self,
        service: &S,
        phase: OperationPhase,
        command: Option<String>,
        metadata: Option<JsonValue>,
    ) -> WorkspaceOperationResult<Uuid> {
        let operation = service
            .record_operation(CreateOperationRequest {
                company_id: self.company_id,
                execution_workspace_id: self.execution_workspace_id,
                phase,
                command,
                metadata,
            })
            .await?;

        self.current_operation_id = Some(operation.id);
        Ok(operation.id)
    }

    /// Complete the current operation
    pub async fn complete<S: WorkspaceOperationService>(
        &mut self,
        service: &S,
        success: bool,
        error_message: Option<String>,
    ) -> WorkspaceOperationResult<()> {
        if let Some(operation_id) = self.current_operation_id.take() {
            service
                .complete_operation(CompleteOperationRequest {
                    operation_id,
                    success,
                    error_message,
                })
                .await?;
        }
        Ok(())
    }

    /// Get current operation ID
    pub fn current_operation_id(&self) -> Option<Uuid> {
        self.current_operation_id
    }
}

/// Default implementation of workspace operation service
pub struct DefaultWorkspaceOperationService {
    pool: Option<PgPool>,
    memory: Arc<Mutex<HashMap<Uuid, WorkspaceOperation>>>,
}

impl DefaultWorkspaceOperationService {
    pub fn new() -> Self {
        Self { pool: None, memory: Arc::new(Mutex::new(HashMap::new())) }
    }
    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool), memory: Arc::new(Mutex::new(HashMap::new())) }
    }
    fn pool(&self) -> WorkspaceOperationResult<&PgPool> {
        self.pool.as_ref().ok_or_else(|| WorkspaceOperationError::InternalError("database pool is not configured".into()))
    }
    fn from_row(row: &sqlx::postgres::PgRow) -> WorkspaceOperationResult<WorkspaceOperation> {
        let phase = match row.get::<String, _>("phase").as_str() {
            "workspace_provision" => OperationPhase::WorkspaceProvision, "workspace_teardown" => OperationPhase::WorkspaceTeardown,
            "runtime_start" => OperationPhase::RuntimeStart, "runtime_stop" => OperationPhase::RuntimeStop,
            "runtime_restart" => OperationPhase::RuntimeRestart, "command_execution" => OperationPhase::CommandExecution,
            "branch_reconcile" => OperationPhase::BranchReconcile, other => return Err(WorkspaceOperationError::InternalError(format!("unknown operation phase {other}"))),
        };
        let status = match row.get::<String, _>("status").as_str() { "running" | "in_progress" => OperationStatus::InProgress, "completed" | "succeeded" => OperationStatus::Completed, "failed" => OperationStatus::Failed, other => return Err(WorkspaceOperationError::InternalError(format!("unknown operation status {other}"))) };
        let started_at = row.get::<DateTime<Utc>, _>("started_at");
        let completed_at = row.get::<Option<DateTime<Utc>>, _>("finished_at");
        Ok(WorkspaceOperation { id: row.get("id"), company_id: row.get("company_id"), execution_workspace_id: row.get("execution_workspace_id"), phase, command: row.get("command"), status, started_at, completed_at, duration_ms: completed_at.map(|v| v.signed_duration_since(started_at).num_milliseconds()), metadata: row.get("metadata"), error_message: row.get("stderr_excerpt") })
    }
}

impl Default for DefaultWorkspaceOperationService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkspaceOperationService for DefaultWorkspaceOperationService {
    fn create_recorder(&self, company_id: Uuid, execution_workspace_id: Uuid) -> OperationRecorder {
        OperationRecorder::new(company_id, execution_workspace_id)
    }

    async fn record_operation(
        &self,
        request: CreateOperationRequest,
    ) -> WorkspaceOperationResult<WorkspaceOperation> {
        if self.pool.is_none() {
            let operation = WorkspaceOperation {
                id: Uuid::new_v4(),
                company_id: request.company_id,
                execution_workspace_id: request.execution_workspace_id,
                phase: request.phase,
                command: request.command,
                status: OperationStatus::InProgress,
                started_at: Utc::now(),
                completed_at: None,
                duration_ms: None,
                metadata: request.metadata,
                error_message: None,
            };
            self.memory.lock().await.insert(operation.id, operation.clone());
            return Ok(operation);
        }
        let pool = self.pool()?;
        let row = sqlx::query("INSERT INTO workspace_operations (company_id, execution_workspace_id, phase, command, metadata) VALUES ($1,$2,$3,$4,$5) RETURNING id, company_id, execution_workspace_id, phase, command, status, metadata, started_at, finished_at, stderr_excerpt").bind(request.company_id).bind(request.execution_workspace_id).bind(request.phase.to_string()).bind(request.command).bind(request.metadata).fetch_one(pool).await?;
        Self::from_row(&row)
    }

    async fn complete_operation(
        &self,
        request: CompleteOperationRequest,
    ) -> WorkspaceOperationResult<WorkspaceOperation> {
        let pool = self.pool()?;
        let row = sqlx::query("UPDATE workspace_operations SET status = $2, exit_code = $3, stderr_excerpt = $4, finished_at = NOW(), updated_at = NOW() WHERE id = $1 RETURNING id, company_id, execution_workspace_id, phase, command, status, metadata, started_at, finished_at, stderr_excerpt").bind(request.operation_id).bind(if request.success { "completed" } else { "failed" }).bind(if request.success { Some(0i32) } else { Some(1i32) }).bind(request.error_message).fetch_optional(pool).await?.ok_or(WorkspaceOperationError::OperationNotFound(request.operation_id))?;
        Self::from_row(&row)
    }

    async fn get_operation(&self, operation_id: Uuid) -> WorkspaceOperationResult<WorkspaceOperation> {
        let pool = self.pool()?;
        let row = sqlx::query("SELECT id, company_id, execution_workspace_id, phase, command, status, metadata, started_at, finished_at, stderr_excerpt FROM workspace_operations WHERE id = $1").bind(operation_id).fetch_optional(pool).await?.ok_or(WorkspaceOperationError::OperationNotFound(operation_id))?;
        Self::from_row(&row)
    }

    async fn list_operations(
        &self,
        query: ListOperationsQuery,
    ) -> WorkspaceOperationResult<Vec<WorkspaceOperation>> {
        let pool = self.pool()?;
        let rows = sqlx::query("SELECT id, company_id, execution_workspace_id, phase, command, status, metadata, started_at, finished_at, stderr_excerpt FROM workspace_operations WHERE execution_workspace_id = $1 AND ($2::text IS NULL OR phase = $2) AND ($3::text IS NULL OR status = $3) ORDER BY started_at DESC LIMIT $4 OFFSET $5").bind(query.execution_workspace_id).bind(query.phase.map(|p| p.to_string())).bind(query.status.map(|s| s.to_string())).bind(query.limit.unwrap_or(100).clamp(1, 1000)).bind(query.offset.unwrap_or(0).max(0)).fetch_all(pool).await?;
        rows.iter().map(Self::from_row).collect()
    }

    async fn get_statistics(
        &self,
        execution_workspace_id: Uuid,
        phase: Option<OperationPhase>,
    ) -> WorkspaceOperationResult<OperationStatistics> {
        let pool = self.pool()?;
        let row = sqlx::query("SELECT COUNT(*)::bigint AS total_count, COUNT(*) FILTER (WHERE status IN ('completed','succeeded'))::bigint AS completed_count, COUNT(*) FILTER (WHERE status='failed')::bigint AS failed_count, COUNT(*) FILTER (WHERE status IN ('running','in_progress'))::bigint AS in_progress_count, AVG(EXTRACT(EPOCH FROM (finished_at - started_at)) * 1000) FILTER (WHERE finished_at IS NOT NULL) AS avg_duration_ms FROM workspace_operations WHERE execution_workspace_id = $1 AND ($2::text IS NULL OR phase = $2)").bind(execution_workspace_id).bind(phase.map(|p| p.to_string())).fetch_one(pool).await?;
        Ok(OperationStatistics { total_count: row.get("total_count"), completed_count: row.get("completed_count"), failed_count: row.get("failed_count"), in_progress_count: row.get("in_progress_count"), avg_duration_ms: row.get("avg_duration_ms"), p50_duration_ms: None, p95_duration_ms: None, p99_duration_ms: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_operation() {
        let service = DefaultWorkspaceOperationService::new();

        let request = CreateOperationRequest {
            company_id: Uuid::new_v4(),
            execution_workspace_id: Uuid::new_v4(),
            phase: OperationPhase::WorkspaceProvision,
            command: None,
            metadata: None,
        };

        let operation = service.record_operation(request).await.unwrap();
        assert_eq!(operation.status, OperationStatus::InProgress);
        assert!(operation.completed_at.is_none());
    }

    #[tokio::test]
    async fn test_operation_recorder() {
        let service = DefaultWorkspaceOperationService::new();
        let mut recorder = service.create_recorder(Uuid::new_v4(), Uuid::new_v4());

        let operation_id = recorder
            .start(&service, OperationPhase::RuntimeStart, None, None)
            .await
            .unwrap();

        assert_eq!(recorder.current_operation_id(), Some(operation_id));
    }

    #[test]
    fn test_operation_duration_calculation() {
        let started_at = Utc::now();
        let completed_at = started_at + chrono::Duration::milliseconds(1500);

        let operation = WorkspaceOperation {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            execution_workspace_id: Uuid::new_v4(),
            phase: OperationPhase::RuntimeStart,
            command: None,
            status: OperationStatus::Completed,
            started_at,
            completed_at: Some(completed_at),
            duration_ms: None,
            metadata: None,
            error_message: None,
        };

        let duration = operation.calculate_duration().unwrap();
        assert!(duration >= 1500 && duration <= 1600);
    }
}
