use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use uuid::Uuid;

/// Saga状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SagaStatus {
    /// 待执行
    Pending,
    /// 执行中
    InProgress,
    /// 已完成
    Completed,
    /// 补偿中
    Compensating,
    /// 失败
    Failed,
    /// 已补偿
    Compensated,
}

/// Saga实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaInstance {
    pub id: Uuid,
    pub saga_name: String,
    pub company_id: Uuid,
    pub status: SagaStatus,
    pub current_step: usize,
    /// Saga上下文（跨步骤共享数据）
    pub context: JsonValue,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

impl SagaInstance {
    pub fn new(saga_name: String, company_id: Uuid, context: JsonValue) -> Self {
        Self {
            id: Uuid::new_v4(),
            saga_name,
            company_id,
            status: SagaStatus::Pending,
            current_step: 0,
            context,
            started_at: Utc::now(),
            completed_at: None,
            error_message: None,
        }
    }

    /// 判断是否可以继续执行
    pub fn can_proceed(&self) -> bool {
        matches!(self.status, SagaStatus::Pending | SagaStatus::InProgress)
    }

    /// 判断是否需要补偿
    pub fn needs_compensation(&self) -> bool {
        matches!(self.status, SagaStatus::Failed)
    }
}

/// Saga步骤执行结果
#[derive(Debug)]
pub enum SagaStepResult {
    /// 成功，返回更新后的上下文
    Success(JsonValue),
    /// 失败，包含错误信息
    Failure(String),
}

/// Saga步骤定义
pub struct SagaStep {
    pub step_name: String,
    /// 执行函数（接收上下文，返回结果）
    pub execute_fn: Arc<dyn Fn(JsonValue) -> futures::future::BoxFuture<'static, SagaStepResult> + Send + Sync>,
    /// 补偿函数（可选，用于回滚）
    pub compensate_fn: Option<Arc<dyn Fn(JsonValue) -> futures::future::BoxFuture<'static, Result<(), String>> + Send + Sync>>,
}

impl SagaStep {
    pub fn new<F, C>(step_name: String, execute_fn: F, compensate_fn: Option<C>) -> Self
    where
        F: Fn(JsonValue) -> futures::future::BoxFuture<'static, SagaStepResult> + Send + Sync + 'static,
        C: Fn(JsonValue) -> futures::future::BoxFuture<'static, Result<(), String>> + Send + Sync + 'static,
    {
        Self {
            step_name,
            execute_fn: Arc::new(execute_fn),
            compensate_fn: compensate_fn.map(|f| Arc::new(f) as Arc<dyn Fn(JsonValue) -> futures::future::BoxFuture<'static, Result<(), String>> + Send + Sync>),
        }
    }

    /// 执行步骤
    pub async fn execute(&self, context: JsonValue) -> SagaStepResult {
        (self.execute_fn)(context).await
    }

    /// 执行补偿
    pub async fn compensate(&self, context: JsonValue) -> Result<(), String> {
        if let Some(compensate_fn) = &self.compensate_fn {
            compensate_fn(context).await
        } else {
            Ok(())
        }
    }
}

/// Saga trait - 定义一个完整的Saga
#[async_trait]
pub trait Saga: Send + Sync {
    /// Saga名称
    fn saga_name(&self) -> &str;

    /// 获取所有步骤
    fn steps(&self) -> Vec<SagaStep>;

    /// 执行Saga（默认实现为顺序执行所有步骤）
    async fn execute(&self, mut context: JsonValue) -> Result<JsonValue, String> {
        let steps = self.steps();
        for step in &steps {
            match step.execute(context.clone()).await {
                SagaStepResult::Success(new_context) => {
                    context = new_context;
                }
                SagaStepResult::Failure(err) => {
                    return Err(format!("Step {} failed: {}", step.step_name, err));
                }
            }
        }
        Ok(context)
    }

    /// 补偿Saga（逆序执行所有补偿函数）
    async fn compensate(&self, context: JsonValue, failed_step_index: usize) -> Result<(), String> {
        let steps = self.steps();

        // 逆序补偿已执行的步骤
        for i in (0..failed_step_index).rev() {
            if let Some(step) = steps.get(i) {
                if let Err(e) = step.compensate(context.clone()).await {
                    // 补偿失败时记录错误但继续尝试补偿其他步骤
                    eprintln!("Compensation failed for step {}: {}", step.step_name, e);
                }
            }
        }

        Ok(())
    }
}

/// Saga错误类型
#[derive(Debug, thiserror::Error)]
pub enum SagaError {
    #[error("Saga execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Saga compensation failed: {0}")]
    CompensationFailed(String),

    #[error("Invalid saga state: {0}")]
    InvalidState(String),

    #[error("Saga not found: {0}")]
    NotFound(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),
}

pub type SagaResult<T> = Result<T, SagaError>;

// ==================== SagaOrchestrator ====================

use std::collections::HashMap;
use tokio::sync::RwLock;

/// Saga编排器 - 管理Saga定义和执行
pub struct SagaOrchestrator {
    /// Saga注册表：saga_name -> Arc<dyn Saga>
    registry: Arc<RwLock<HashMap<String, Arc<dyn Saga>>>>,
    /// Saga仓库（持久化）
    repository: Arc<dyn crate::SagaRepository>,
}

impl SagaOrchestrator {
    pub fn new(repository: Arc<dyn crate::SagaRepository>) -> Self {
        Self {
            registry: Arc::new(RwLock::new(HashMap::new())),
            repository,
        }
    }

    /// 注册Saga定义
    pub async fn register_saga(&self, saga: Arc<dyn Saga>) -> SagaResult<()> {
        let saga_name = saga.saga_name().to_string();
        let mut registry = self.registry.write().await;

        if registry.contains_key(&saga_name) {
            return Err(SagaError::InvalidState(format!(
                "Saga {} already registered",
                saga_name
            )));
        }

        registry.insert(saga_name, saga);
        Ok(())
    }

    /// 取消注册Saga
    pub async fn unregister_saga(&self, saga_name: &str) -> SagaResult<()> {
        let mut registry = self.registry.write().await;
        registry.remove(saga_name);
        Ok(())
    }

    /// 启动新Saga实例
    pub async fn execute_saga(
        &self,
        saga_name: &str,
        company_id: Uuid,
        context: JsonValue,
    ) -> SagaResult<Uuid> {
        // 查找Saga定义
        let saga = {
            let registry = self.registry.read().await;
            registry
                .get(saga_name)
                .cloned()
                .ok_or_else(|| SagaError::NotFound(format!("Saga {} not found", saga_name)))?
      };

        // 创建Saga实例
        let instance = SagaInstance::new(saga_name.to_string(), company_id, context);
        let instance_id = instance.id;

        // 持久化实例
        self.repository
            .create(&instance)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

        // 更新状态为InProgress
        self.repository
            .update_status(instance_id, SagaStatus::InProgress, None)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

        // 异步执行Saga（后台任务）
        let saga_clone = Arc::clone(&saga);
        let repository_clone = Arc::clone(&self.repository);
        tokio::spawn(async move {
            Self::execute_saga_steps(saga_clone, instance_id, repository_clone).await
        });

        Ok(instance_id)
    }

    /// 执行Saga步骤（step-by-step）
    async fn execute_saga_steps(
        saga: Arc<dyn Saga>,
        instance_id: Uuid,
        repository: Arc<dyn crate::SagaRepository>,
    ) -> SagaResult<()> {
        // 获取实例
        let mut instance = repository
            .get_by_id(instance_id)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))?
            .ok_or_else(|| SagaError::NotFound(format!("Instance {} not found", instance_id)))?;

        let steps = saga.steps();
        let mut current_context = instance.context.clone();

        for (step_index, step) in steps.iter().enumerate() {
            // 记录步骤开始
            let execution = crate::repositories::saga_repository::SagaStepExecution {
                id: Uuid::new_v4(),
                saga_instance_id: instance_id,
                step_index: step_index as i32,
                step_name: step.step_name.clone(),
                status: "running".to_string(),
                started_at: Utc::now(),
                completed_at: None,
                error_message: None,
            };

            repository
                .record_step_execution(&execution)
                .await
                .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

            // 执行步骤
            match step.execute(current_context.clone()).await {
                SagaStepResult::Success(new_context) => {
                    current_context = new_context.clone();

                    // 更新步骤记录为成功
                    let mut completed_execution = execution.clone();
                    completed_execution.status = "success".to_string();
                    completed_execution.completed_at = Some(Utc::now());

                    repository
                        .record_step_execution(&completed_execution)
                        .await
                        .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

                    // 更新实例上下文和当前步骤
                    repository
                        .update_context(instance_id, new_context)
                        .await
                        .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

                    repository
                        .update_current_step(instance_id, step_index + 1)
                        .await
                        .map_err(|e| SagaError::RepositoryError(e.to_string()))?;
                }
                SagaStepResult::Failure(err) => {
                    // 记录步骤失败
                    let mut failed_execution = execution.clone();
                    failed_execution.status = "failed".to_string();
                    failed_execution.completed_at = Some(Utc::now());
                    failed_execution.error_message = Some(err.clone());

                    repository
                        .record_step_execution(&failed_execution)
                        .await
                        .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

                    // 更新实例状态为Failed
                    repository
                        .update_status(instance_id, SagaStatus::Failed, Some(err.clone()))
                        .await
                        .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

                    // 执行补偿
                    if let Err(comp_err) = saga.compensate(current_context, step_index).await {
                        repository
                            .update_status(
                                instance_id,
                                SagaStatus::Failed,
                                Some(format!("Compensation failed: {}", comp_err)),
                            )
                            .await
                            .map_err(|e| SagaError::RepositoryError(e.to_string()))?;
                    } else {
                        repository
                            .update_status(instance_id, SagaStatus::Compensated, None)
                            .await
                            .map_err(|e| SagaError::RepositoryError(e.to_string()))?;
                    }

                    return Err(SagaError::ExecutionFailed(err));
                }
            }
        }

        // 所有步骤成功，更新状态为Completed
        repository
            .update_status(instance_id, SagaStatus::Completed, None)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))?;

        Ok(())
    }

    /// 获取Saga实例状态
    pub async fn get_instance_status(&self, instance_id: Uuid) -> SagaResult<SagaInstance> {
        self.repository
            .get_by_id(instance_id)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))?
            .ok_or_else(|| SagaError::NotFound(format!("Instance {} not found", instance_id)))
    }

    /// 列出指定状态的Saga实例
    pub async fn list_instances_by_status(
        &self,
        company_id: Uuid,
        status: SagaStatus,
    ) -> SagaResult<Vec<SagaInstance>> {
        self.repository
            .list_by_status(company_id, status)
            .await
            .map_err(|e| SagaError::RepositoryError(e.to_string()))
    }
}

// 需要在repositories crate中定义SagaRepository trait
// 这里使用占位符trait
#[async_trait]
pub trait SagaRepository: Send + Sync {
    async fn create(&self, instance: &SagaInstance) -> Result<SagaInstance, String>;
    async fn update_status(&self, id: Uuid, status: SagaStatus, error_message: Option<String>) -> Result<(), String>;
    async fn update_current_step(&self, id: Uuid, current_step: usize) -> Result<(), String>;
    async fn update_context(&self, id: Uuid, context: JsonValue) -> Result<(), String>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<SagaInstance>, String>;
    async fn list_by_status(&self, company_id: Uuid, status: SagaStatus) -> Result<Vec<SagaInstance>, String>;
    async fn record_step_execution(&self, execution: &crate::repositories::saga_repository::SagaStepExecution) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_saga_instance_creation() {
        let instance = SagaInstance::new(
            "test_saga".to_string(),
            Uuid::new_v4(),
            json!({"key": "value"}),
        );

        assert_eq!(instance.saga_name, "test_saga");
        assert_eq!(instance.status, SagaStatus::Pending);
        assert_eq!(instance.current_step, 0);
        assert!(instance.can_proceed());
        assert!(!instance.needs_compensation());
    }

    #[test]
    fn test_saga_status_transitions() {
        let mut instance = SagaInstance::new(
            "test".to_string(),
            Uuid::new_v4(),
            json!({}),
        );

        instance.status = SagaStatus::InProgress;
        assert!(instance.can_proceed());

        instance.status = SagaStatus::Failed;
        assert!(!instance.can_proceed());
        assert!(instance.needs_compensation());

        instance.status = SagaStatus::Completed;
        assert!(!instance.can_proceed());
        assert!(!instance.needs_compensation());
    }
}
