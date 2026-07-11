use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use uuid::Uuid;

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
    // TODO: Add database pool
}

impl DefaultWorkspaceOperationService {
    pub fn new() -> Self {
        Self {}
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
        let now = Utc::now();
        let operation = WorkspaceOperation {
            id: Uuid::new_v4(),
            company_id: request.company_id,
            execution_workspace_id: request.execution_workspace_id,
            phase: request.phase,
            command: request.command,
            status: OperationStatus::InProgress,
            started_at: now,
            completed_at: None,
            duration_ms: None,
            metadata: request.metadata,
            error_message: None,
        };

        // TODO: Persist to database
        Ok(operation)
    }

    async fn complete_operation(
        &self,
        request: CompleteOperationRequest,
    ) -> WorkspaceOperationResult<WorkspaceOperation> {
        // TODO: Load from database
        let mut operation = self.get_operation(request.operation_id).await?;

        let now = Utc::now();
        operation.status = if request.success {
            OperationStatus::Completed
        } else {
            OperationStatus::Failed
        };
        operation.completed_at = Some(now);
        operation.duration_ms = operation.calculate_duration();
        operation.error_message = request.error_message;

        // TODO: Update in database
        Ok(operation)
    }

    async fn get_operation(&self, operation_id: Uuid) -> WorkspaceOperationResult<WorkspaceOperation> {
        // TODO: Load from database
        Err(WorkspaceOperationError::OperationNotFound(operation_id))
    }

    async fn list_operations(
        &self,
        _query: ListOperationsQuery,
    ) -> WorkspaceOperationResult<Vec<WorkspaceOperation>> {
        // TODO: Query from database with filters
        Ok(Vec::new())
    }

    async fn get_statistics(
        &self,
        _execution_workspace_id: Uuid,
        _phase: Option<OperationPhase>,
    ) -> WorkspaceOperationResult<OperationStatistics> {
        // TODO: Calculate from database
        Ok(OperationStatistics {
            total_count: 0,
            completed_count: 0,
            failed_count: 0,
            in_progress_count: 0,
            avg_duration_ms: None,
            p50_duration_ms: None,
            p95_duration_ms: None,
            p99_duration_ms: None,
        })
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
