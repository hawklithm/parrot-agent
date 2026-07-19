use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "saga_status", rename_all = "snake_case")]
pub enum SagaStatus {
    Pending,
    InProgress,
    Compensating,
    Succeeded,
    Failed,
    Compensated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStep {
    pub step_name: String,
    pub action: SagaAction,
    pub compensation: Option<SagaAction>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SagaAction {
    CreateAgent { agent_id: Uuid },
    CreateApproval { approval_id: Uuid },
    MaterializeInstructions { agent_id: Uuid, bundle_id: Uuid },
    CreateBudgetPolicy { agent_id: Uuid, policy_id: Uuid },
    CheckoutIssue { issue_id: Uuid, agent_id: Uuid },
    AcquireLease { environment_id: Uuid, agent_id: Uuid },
    CreateWorkspace { workspace_id: Uuid, environment_id: Uuid },
    StartRuntimeServices { workspace_id: Uuid },
    WakeupAgent { agent_id: Uuid },
    CreateRoutineRun { routine_id: Uuid, run_id: Uuid },
    CreateIssue { issue_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SagaInstance {
    pub id: Uuid,
    pub saga_name: String,
    pub company_id: Uuid,
    pub initiator_id: Option<Uuid>,
    pub status: SagaStatus,
    pub current_step: Option<String>,
    pub context: sqlx::types::Json<serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SagaStepExecution {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub step_name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<sqlx::types::Json<serde_json::Value>>,
    pub compensation_result: Option<sqlx::types::Json<serde_json::Value>>,
}

#[async_trait]
pub trait Saga: Send + Sync {
    async fn execute(&self, context: serde_json::Value) -> ServiceResult<serde_json::Value>;
    async fn compensate(&self, context: serde_json::Value, step_name: String) -> ServiceResult<()>;
    fn status(&self) -> SagaStatus;
}

#[async_trait]
pub trait SagaOrchestrator: Send + Sync {
    async fn start_saga(&self, saga_name: String, context: serde_json::Value) -> ServiceResult<Uuid>;
    async fn get_saga_status(&self, saga_id: Uuid) -> ServiceResult<SagaInstance>;
    async fn retry_saga(&self, saga_id: Uuid) -> ServiceResult<()>;
    async fn execute_step(&self, saga_id: Uuid, step: SagaStep) -> ServiceResult<serde_json::Value>;
    async fn compensate_step(&self, saga_id: Uuid, step: SagaStep) -> ServiceResult<()>;
}

pub struct SagaOrchestratorImpl<R, E> {
    repository: Arc<R>,
    #[allow(dead_code)]
    event_bus: Arc<E>,
}

impl<R, E> SagaOrchestratorImpl<R, E> {
    pub fn new(repository: Arc<R>, event_bus: Arc<E>) -> Self {
        Self { repository, event_bus }
    }
}

#[async_trait]
impl<R, E> SagaOrchestrator for SagaOrchestratorImpl<R, E>
where
    R: SagaRepository + Send + Sync,
    E: Send + Sync,
{
    async fn start_saga(&self, saga_name: String, context: serde_json::Value) -> ServiceResult<Uuid> {
        let saga_id = Uuid::new_v4();
        let instance = SagaInstance {
            id: saga_id,
            saga_name,
            company_id: Uuid::nil(),
            initiator_id: None,
            status: SagaStatus::Pending,
            current_step: None,
            context: sqlx::types::Json(context),
            started_at: Utc::now(),
            completed_at: None,
        };

        self.repository.create_saga(instance).await?;
        Ok(saga_id)
    }

    async fn get_saga_status(&self, saga_id: Uuid) -> ServiceResult<SagaInstance> {
        self.repository.find_saga_by_id(saga_id).await
    }

    async fn retry_saga(&self, saga_id: Uuid) -> ServiceResult<()> {
        let saga = self.repository.find_saga_by_id(saga_id).await?;

        if saga.status != SagaStatus::Failed {
            return Err(ServiceError::InvalidState(
                format!("Cannot retry saga in status {:?}", saga.status)
            ));
        }

        self.repository.update_saga_status(saga_id, SagaStatus::Pending).await?;
        Ok(())
    }

    async fn execute_step(&self, saga_id: Uuid, step: SagaStep) -> ServiceResult<serde_json::Value> {
        let execution_id = Uuid::new_v4();
        let started_at = Utc::now();

        self.repository.create_step_execution(SagaStepExecution {
            id: execution_id,
            saga_id,
            step_name: step.step_name.clone(),
            status: "in_progress".to_string(),
            started_at,
            completed_at: None,
            result: None,
            compensation_result: None,
        }).await?;

        let timeout = tokio::time::Duration::from_secs(step.timeout_seconds);
        let result = match tokio::time::timeout(timeout, self.execute_action(step.action)).await {
            Ok(Ok(result)) => {
                self.repository.update_step_execution(
                    execution_id,
                    "succeeded".to_string(),
                    Some(sqlx::types::Json(result.clone())),
                ).await?;
                Ok(result)
            }
            Ok(Err(e)) => {
                self.repository.update_step_execution(
                    execution_id,
                    "failed".to_string(),
                    Some(sqlx::types::Json(serde_json::json!({"error": e.to_string()}))),
                ).await?;
                Err(e)
            }
            Err(_) => {
                let error = ServiceError::Timeout(format!("Step {} timed out", step.step_name));
                self.repository.update_step_execution(
                    execution_id,
                    "failed".to_string(),
                    Some(sqlx::types::Json(serde_json::json!({"error": error.to_string()}))),
                ).await?;
                Err(error)
            }
        };

        result
    }

    async fn compensate_step(&self, saga_id: Uuid, step: SagaStep) -> ServiceResult<()> {
        if let Some(compensation) = step.compensation {
            let execution_id = Uuid::new_v4();

            self.repository.create_step_execution(SagaStepExecution {
                id: execution_id,
                saga_id,
                step_name: format!("{}_compensation", step.step_name),
                status: "compensating".to_string(),
                started_at: Utc::now(),
                completed_at: None,
                result: None,
                compensation_result: None,
            }).await?;

            match self.execute_action(compensation).await {
                Ok(result) => {
                    self.repository.update_step_execution(
                        execution_id,
                        "compensated".to_string(),
                        Some(sqlx::types::Json(result)),
                    ).await?;
                    Ok(())
                }
                Err(e) => {
                    self.repository.update_step_execution(
                        execution_id,
                        "compensation_failed".to_string(),
                        Some(sqlx::types::Json(serde_json::json!({"error": e.to_string()}))),
                    ).await?;
                    Err(e)
                }
            }
        } else {
            Ok(())
        }
    }
}

impl<R, E> SagaOrchestratorImpl<R, E> {
    async fn execute_action(&self, action: SagaAction) -> ServiceResult<serde_json::Value> {
        match action {
            SagaAction::CreateAgent { agent_id } => {
                Ok(serde_json::json!({"agent_id": agent_id}))
            }
            SagaAction::CreateApproval { approval_id } => {
                Ok(serde_json::json!({"approval_id": approval_id}))
            }
            SagaAction::MaterializeInstructions { agent_id, bundle_id } => {
                Ok(serde_json::json!({"agent_id": agent_id, "bundle_id": bundle_id}))
            }
            SagaAction::CreateBudgetPolicy { agent_id, policy_id } => {
                Ok(serde_json::json!({"agent_id": agent_id, "policy_id": policy_id}))
            }
            SagaAction::CheckoutIssue { issue_id, agent_id } => {
                Ok(serde_json::json!({"issue_id": issue_id, "agent_id": agent_id}))
            }
            SagaAction::AcquireLease { environment_id, agent_id } => {
                Ok(serde_json::json!({"environment_id": environment_id, "agent_id": agent_id}))
            }
            SagaAction::CreateWorkspace { workspace_id, environment_id } => {
                Ok(serde_json::json!({"workspace_id": workspace_id, "environment_id": environment_id}))
            }
            SagaAction::StartRuntimeServices { workspace_id } => {
                Ok(serde_json::json!({"workspace_id": workspace_id}))
            }
            SagaAction::WakeupAgent { agent_id } => {
                Ok(serde_json::json!({"agent_id": agent_id}))
            }
            SagaAction::CreateRoutineRun { routine_id, run_id } => {
                Ok(serde_json::json!({"routine_id": routine_id, "run_id": run_id}))
            }
            SagaAction::CreateIssue { issue_id } => {
                Ok(serde_json::json!({"issue_id": issue_id}))
            }
        }
    }
}

#[async_trait]
pub trait SagaRepository: Send + Sync {
    async fn create_saga(&self, saga: SagaInstance) -> ServiceResult<()>;
    async fn find_saga_by_id(&self, saga_id: Uuid) -> ServiceResult<SagaInstance>;
    async fn update_saga_status(&self, saga_id: Uuid, status: SagaStatus) -> ServiceResult<()>;
    async fn create_step_execution(&self, execution: SagaStepExecution) -> ServiceResult<()>;
    async fn update_step_execution(
        &self,
        execution_id: Uuid,
        status: String,
        result: Option<sqlx::types::Json<serde_json::Value>>,
    ) -> ServiceResult<()>;
    async fn find_incomplete_sagas(&self) -> ServiceResult<Vec<SagaInstance>>;
}
