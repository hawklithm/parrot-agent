use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Saga trait for orchestrating multi-step transactions
#[async_trait]
pub trait Saga: Send + Sync {
    async fn execute(&self, context: &mut SagaContext) -> Result<(), String>;
    async fn compensate(&self, context: &SagaContext) -> Result<(), String>;
    fn status(&self) -> SagaStatus;
}

/// Saga step definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStep {
    pub step_name: String,
    pub timeout_seconds: u64,
    pub retry_policy: RetryPolicy,
}

/// Retry policy for saga steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_multiplier: f64,
    pub initial_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_multiplier: 2.0,
            initial_delay_ms: 1000,
        }
    }
}

/// Saga status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SagaStatus {
    Pending,
    InProgress,
    Compensating,
    Succeeded,
    Failed,
    Compensated,
}

/// Saga context for passing state between steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaContext {
    pub saga_id: Uuid,
    pub company_id: Uuid,
    pub initiator_id: Uuid,
    pub state: serde_json::Value,
    pub completed_steps: Vec<String>,
}

/// Saga instance record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaInstance {
    pub id: Uuid,
    pub saga_name: String,
    pub company_id: Uuid,
    pub status: SagaStatus,
    pub current_step: Option<String>,
    pub context: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Saga step execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaStepExecution {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub step_name: String,
    pub status: StepExecutionStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub compensation_result: Option<serde_json::Value>,
    pub retry_count: i32,
}

/// Step execution status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepExecutionStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Compensating,
    Compensated,
}

/// Saga orchestrator trait
#[async_trait]
pub trait SagaOrchestrator: Send + Sync {
    async fn start_saga(
        &self,
        saga_name: String,
        company_id: Uuid,
        initiator_id: Uuid,
        initial_context: serde_json::Value,
    ) -> Result<SagaInstance, String>;

    async fn get_saga_status(&self, saga_id: Uuid) -> Result<SagaInstance, String>;

    async fn retry_saga(&self, saga_id: Uuid) -> Result<(), String>;

    async fn compensate_saga(&self, saga_id: Uuid) -> Result<(), String>;
}

/// Saga repository trait
#[async_trait]
pub trait SagaRepository: Send + Sync {
    async fn create_instance(&self, instance: SagaInstance) -> Result<SagaInstance, String>;
    async fn update_instance(&self, instance: SagaInstance) -> Result<SagaInstance, String>;
    async fn get_instance(&self, saga_id: Uuid) -> Result<Option<SagaInstance>, String>;

    async fn create_step_execution(
        &self,
        execution: SagaStepExecution,
    ) -> Result<SagaStepExecution, String>;
    async fn update_step_execution(
        &self,
        execution: SagaStepExecution,
    ) -> Result<SagaStepExecution, String>;
    async fn list_step_executions(
        &self,
        saga_id: Uuid,
    ) -> Result<Vec<SagaStepExecution>, String>;
}
