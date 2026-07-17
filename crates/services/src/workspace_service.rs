use async_trait::async_trait;
use models::execution_environment::{
    ExecutionWorkspace, WorkspaceStatus,
    CreateExecutionWorkspaceInput, UpdateExecutionWorkspaceInput,
};
use repositories::{ExecutionWorkspaceRepository, RepositoryError};
use crate::lease_service::{LeaseService, AcquireLeaseRequest};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Lease service error: {0}")]
    LeaseService(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),

    #[error("Workspace not ready: {0}")]
    WorkspaceNotReady(String),

    #[error("Invalid state transition: from {0:?} to {1:?}")]
    InvalidStateTransition(WorkspaceStatus, WorkspaceStatus),

    #[error("Orchestration failed: {0}")]
    OrchestrationFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Workspace provisioning result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProvisionResult {
    pub workspace: ExecutionWorkspace,
    pub lease_id: Option<Uuid>,
}

/// Runtime service entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeServiceEntry {
    pub name: String,
    pub command: Vec<String>,
    pub env_vars: Option<serde_json::Value>,
    pub restart_policy: RestartPolicy,
}

/// Restart policy for runtime services
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    Never,
    Always,
    OnFailure,
    ExponentialBackoff,
}

/// Workspace runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRuntimeConfig {
    pub services: Vec<RuntimeServiceEntry>,
    pub env_vars: Option<serde_json::Value>,
}

/// Execution workspace service trait
#[async_trait]
pub trait ExecutionWorkspaceService: Send + Sync {
    /// Create a new workspace
    async fn create_workspace(
        &self,
        input: CreateExecutionWorkspaceInput,
    ) -> Result<ExecutionWorkspace, WorkspaceServiceError>;

    /// Get workspace by ID
    async fn get_workspace(&self, id: Uuid) -> Result<Option<ExecutionWorkspace>, WorkspaceServiceError>;

    /// List workspaces by company
    async fn list_workspaces(
        &self,
        company_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ExecutionWorkspace>, WorkspaceServiceError>;

    /// Update workspace status
    async fn update_status(
        &self,
        id: Uuid,
        status: WorkspaceStatus,
    ) -> Result<ExecutionWorkspace, WorkspaceServiceError>;

    /// Delete workspace
    async fn delete_workspace(&self, id: Uuid) -> Result<(), WorkspaceServiceError>;

    /// Provision workspace (create + acquire lease + setup)
    async fn provision_workspace(
        &self,
        input: CreateExecutionWorkspaceInput,
        environment_id: Uuid,
    ) -> Result<WorkspaceProvisionResult, WorkspaceServiceError>;

    /// Ensure workspace is available
    async fn ensure_available(&self, id: Uuid) -> Result<ExecutionWorkspace, WorkspaceServiceError>;

    /// Teardown workspace (cleanup + release lease)
    async fn teardown_workspace(&self, id: Uuid) -> Result<(), WorkspaceServiceError>;
}

/// Workspace orchestrator for coordinating workspace lifecycle
pub struct WorkspaceOrchestrator<W, L>
where
    W: ExecutionWorkspaceRepository,
    L: LeaseService,
{
    workspace_repo: Arc<W>,
    lease_service: Arc<L>,
}

impl<W, L> WorkspaceOrchestrator<W, L>
where
    W: ExecutionWorkspaceRepository,
    L: LeaseService,
{
    pub fn new(workspace_repo: Arc<W>, lease_service: Arc<L>) -> Self {
        Self {
            workspace_repo,
            lease_service,
        }
    }

    /// Orchestrate workspace provisioning
    pub async fn orchestrate_provision(
        &self,
        input: CreateExecutionWorkspaceInput,
        environment_id: Uuid,
    ) -> Result<WorkspaceProvisionResult, WorkspaceServiceError> {
        // Step 1: Create workspace record with Provisioning status
        let company_id = input.company_id; // 先取出 company_id，避免 input 被 move
        let workspace = self.workspace_repo.create(input).await?;

        // Step 2: Acquire environment lease
        let lease_request = AcquireLeaseRequest {
            environment_id,
            execution_workspace_id: Some(workspace.id),
            issue_id: workspace.source_issue_id,
            heartbeat_run_id: None,
        };
        let lease = match self.lease_service.acquire_lease(company_id, lease_request).await {
            Ok(lease) => Some(lease),
            Err(e) => {
                // Rollback: mark workspace as error
                let _ = self.workspace_repo.update(
                    workspace.id,
                    UpdateExecutionWorkspaceInput {
                        name: None,
                        status: Some(WorkspaceStatus::Error),
                        cwd: None,
                        provider_ref: None,
                        base_ref: None,
                        branch_name: None,
                        metadata: None,
                    },
                ).await;
                return Err(WorkspaceServiceError::OrchestrationFailed(
                    format!("Failed to acquire lease: {}", e)
                ));
            }
        };

        // Step 3: Update workspace to Ready status
        let updated_workspace = self.workspace_repo.update(
            workspace.id,
            UpdateExecutionWorkspaceInput {
                name: None,
                status: Some(WorkspaceStatus::Ready),
                cwd: None,
                provider_ref: lease.as_ref().map(|l| l.id.to_string()),
                base_ref: None,
                branch_name: None,
                metadata: None,
            },
        ).await?;

        Ok(WorkspaceProvisionResult {
            workspace: updated_workspace,
            lease_id: lease.map(|l| l.id),
        })
    }

    /// Orchestrate workspace teardown
    pub async fn orchestrate_teardown(&self, workspace_id: Uuid) -> Result<(), WorkspaceServiceError> {
        // Get workspace
        let workspace = self
            .workspace_repo
            .get_by_id(workspace_id)
            .await?
            .ok_or(WorkspaceServiceError::WorkspaceNotFound(workspace_id))?;

        // Update status to Teardown
        self.workspace_repo.update(
            workspace_id,
            UpdateExecutionWorkspaceInput {
                name: None,
                status: Some(WorkspaceStatus::Teardown),
                cwd: None,
                provider_ref: None,
                base_ref: None,
                branch_name: None,
                metadata: None,
            },
        ).await?;

        // Release lease if exists
        if let Some(provider_ref) = workspace.provider_ref {
            if let Ok(lease_id) = Uuid::parse_str(&provider_ref) {
                let _ = self.lease_service.release_lease(lease_id, workspace.company_id).await;
            }
        }

        // Delete workspace record
        self.workspace_repo.delete(workspace_id).await?;

        Ok(())
    }
}

/// Default implementation of ExecutionWorkspaceService
pub struct DefaultExecutionWorkspaceService<W, L>
where
    W: ExecutionWorkspaceRepository,
    L: LeaseService,
{
    workspace_repo: Arc<W>,
    orchestrator: WorkspaceOrchestrator<W, L>,
}

impl<W, L> DefaultExecutionWorkspaceService<W, L>
where
    W: ExecutionWorkspaceRepository,
    L: LeaseService,
{
    pub fn new(workspace_repo: Arc<W>, lease_service: Arc<L>) -> Self {
        let orchestrator = WorkspaceOrchestrator::new(workspace_repo.clone(), lease_service);
        Self {
            workspace_repo,
            orchestrator,
        }
    }

    fn validate_state_transition(
        &self,
        from: WorkspaceStatus,
        to: WorkspaceStatus,
    ) -> Result<(), WorkspaceServiceError> {
        let valid = match (from, to) {
            // Provisioning can go to Ready or Error
            (WorkspaceStatus::Provisioning, WorkspaceStatus::Ready) => true,
            (WorkspaceStatus::Provisioning, WorkspaceStatus::Error) => true,

            // Ready can go to Running or Teardown
            (WorkspaceStatus::Ready, WorkspaceStatus::Running) => true,
            (WorkspaceStatus::Ready, WorkspaceStatus::Teardown) => true,

            // Running can go to Ready or Teardown
            (WorkspaceStatus::Running, WorkspaceStatus::Ready) => true,
            (WorkspaceStatus::Running, WorkspaceStatus::Teardown) => true,

            // Error can go to Teardown or Archived
            (WorkspaceStatus::Error, WorkspaceStatus::Teardown) => true,
            (WorkspaceStatus::Error, WorkspaceStatus::Archived) => true,

            // Teardown can go to Archived
            (WorkspaceStatus::Teardown, WorkspaceStatus::Archived) => true,

            // Same status is always valid
            _ if from == to => true,

            _ => false,
        };

        if valid {
            Ok(())
        } else {
            Err(WorkspaceServiceError::InvalidStateTransition(from, to))
        }
    }
}

#[async_trait]
impl<W, L> ExecutionWorkspaceService for DefaultExecutionWorkspaceService<W, L>
where
    W: ExecutionWorkspaceRepository + 'static,
    L: LeaseService + 'static,
{
    async fn create_workspace(
        &self,
        input: CreateExecutionWorkspaceInput,
    ) -> Result<ExecutionWorkspace, WorkspaceServiceError> {
        let workspace = self.workspace_repo.create(input).await?;
        Ok(workspace)
    }

    async fn get_workspace(&self, id: Uuid) -> Result<Option<ExecutionWorkspace>, WorkspaceServiceError> {
        let workspace = self.workspace_repo.get_by_id(id).await?;
        Ok(workspace)
    }

    async fn list_workspaces(
        &self,
        company_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ExecutionWorkspace>, WorkspaceServiceError> {
        let workspaces = self
            .workspace_repo
            .list_by_company(company_id, limit, offset)
            .await?;
        Ok(workspaces)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: WorkspaceStatus,
    ) -> Result<ExecutionWorkspace, WorkspaceServiceError> {
        // Get current workspace to validate transition
        let current = self
            .workspace_repo
            .get_by_id(id)
            .await?
            .ok_or(WorkspaceServiceError::WorkspaceNotFound(id))?;

        // Validate state transition
        self.validate_state_transition(current.status, status)?;

        // Update status
        let workspace = self
            .workspace_repo
            .update(
                id,
                UpdateExecutionWorkspaceInput {
                    name: None,
                    status: Some(status),
                    cwd: None,
                    provider_ref: None,
                    base_ref: None,
                    branch_name: None,
                    metadata: None,
                },
            )
            .await?;

        Ok(workspace)
    }

    async fn delete_workspace(&self, id: Uuid) -> Result<(), WorkspaceServiceError> {
        self.workspace_repo.delete(id).await?;
        Ok(())
    }

    async fn provision_workspace(
        &self,
        input: CreateExecutionWorkspaceInput,
        environment_id: Uuid,
    ) -> Result<WorkspaceProvisionResult, WorkspaceServiceError> {
        let result = self.orchestrator.orchestrate_provision(input, environment_id).await?;
        Ok(result)
    }

    async fn ensure_available(&self, id: Uuid) -> Result<ExecutionWorkspace, WorkspaceServiceError> {
        let workspace = self
            .workspace_repo
            .get_by_id(id)
            .await?
            .ok_or(WorkspaceServiceError::WorkspaceNotFound(id))?;

        if !workspace.is_ready() && !workspace.is_running() {
            return Err(WorkspaceServiceError::WorkspaceNotReady(
                format!("Workspace status: {:?}", workspace.status),
            ));
        }

        Ok(workspace)
    }

    async fn teardown_workspace(&self, id: Uuid) -> Result<(), WorkspaceServiceError> {
        self.orchestrator.orchestrate_teardown(id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
use super::*;

    #[test]
    fn test_valid_state_transitions() {
        let service = DefaultExecutionWorkspaceService {
            workspace_repo: Arc::new(()),
            orchestrator: WorkspaceOrchestrator {
                workspace_repo: Arc::new(()),
                lease_service: Arc::new(()),
            },
        };

        // Valid transitions
        assert!(service
            .validate_state_transition(WorkspaceStatus::Provisioning, WorkspaceStatus::Ready)
            .is_ok());
        assert!(service
            .validate_state_transition(WorkspaceStatus::Ready, WorkspaceStatus::Running)
            .is_ok());
        assert!(service
            .validate_state_transition(WorkspaceStatus::Running, WorkspaceStatus::Teardown)
            .is_ok());
    }

    #[test]
    fn test_invalid_state_transitions() {
        let service = DefaultExecutionWorkspaceService {
            workspace_repo: Arc::new(()),
            orchestrator: WorkspaceOrchestrator {
                workspace_repo: Arc::new(()),
                lease_service: Arc::new(()),
            },
        };

        // Invalid transitions
        assert!(service
            .validate_state_transition(WorkspaceStatus::Archived, WorkspaceStatus::Ready)
            .is_err());
        assert!(service
            .validate_state_transition(WorkspaceStatus::Running, WorkspaceStatus::Provisioning)
            .is_err());
    }
}
