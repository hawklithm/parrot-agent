use async_trait::async_trait;
use chrono::Utc;
use models::event_bus::{EventBus, SystemEvent};
use models::saga::{
    RetryPolicy, SagaContext, SagaInstance, SagaOrchestrator, SagaRepository, SagaStatus,
    SagaStep, SagaStepExecution, StepExecutionStatus,
};
use std::sync::Arc;
use uuid::Uuid;

/// Default saga orchestrator implementation
pub struct DefaultSagaOrchestrator {
    saga_repo: Arc<dyn SagaRepository>,
    event_bus: Arc<dyn EventBus>,
}

impl DefaultSagaOrchestrator {
    pub fn new(saga_repo: Arc<dyn SagaRepository>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            saga_repo,
            event_bus,
        }
    }

    /// Execute a single saga step
    async fn execute_step(
        &self,
        saga_id: Uuid,
        step: &SagaStep,
        context: &mut SagaContext,
    ) -> Result<serde_json::Value, String> {
        let step_execution = SagaStepExecution {
            id: Uuid::new_v4(),
            saga_id,
            step_name: step.step_name.clone(),
            status: StepExecutionStatus::Running,
            started_at: Utc::now(),
            completed_at: None,
            result: None,
            error_message: None,
            compensation_result: None,
            retry_count: 0,
        };

        let mut execution = self
            .saga_repo
            .create_step_execution(step_execution)
            .await?;

        // Execute step with retry policy
        let result = self
            .execute_with_retry(saga_id, step, context, &step.retry_policy)
            .await;

        match result {
            Ok(value) => {
                execution.status = StepExecutionStatus::Succeeded;
                execution.completed_at = Some(Utc::now());
                execution.result = Some(value.clone());
                self.saga_repo.update_step_execution(execution).await?;
                context.completed_steps.push(step.step_name.clone());
                Ok(value)
            }
            Err(e) => {
                execution.status = StepExecutionStatus::Failed;
                execution.completed_at = Some(Utc::now());
                execution.error_message = Some(e.clone());
                self.saga_repo.update_step_execution(execution).await?;
                Err(e)
            }
        }
    }

    /// Execute step with retry policy
    async fn execute_with_retry(
        &self,
        _saga_id: Uuid,
        step: &SagaStep,
        context: &mut SagaContext,
        retry_policy: &RetryPolicy,
    ) -> Result<serde_json::Value, String> {
        let mut retry_count = 0;
        let mut delay_ms = retry_policy.initial_delay_ms;

        loop {
            match self.execute_step_logic(step, context).await {
                Ok(result) => return Ok(result),              Err(e) => {
                    if retry_count >= retry_policy.max_retries {
                        return Err(format!(
                            "Step '{}' failed after {} retries: {}",
                            step.step_name, retry_count, e
                        ));
                    }

                    retry_count += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    delay_ms = (delay_ms as f64 * retry_policy.backoff_multiplier) as u64;
                }
            }
        }
    }

    /// Execute step business logic (placeholder - will be overridden by specific sagas)
    async fn execute_step_logic(
        &self,
        step: &SagaStep,
        _context: &mut SagaContext,
    ) -> Result<serde_json::Value, String> {
        // Placeholder: In production, this would dispatch to step-specific handlers
        eprintln!("Executing step: {}", step.step_name);
        Ok(serde_json::json!({"success": true}))
    }

    /// Compensate a single step
    async fn compensate_step(
        &self,
        saga_id: Uuid,
        step_name: &str,
        context: &SagaContext,
    ) -> Result<(), String> {
        let step_execution = SagaStepExecution {
            id: Uuid::new_v4(),
            saga_id,
            step_name: step_name.to_string(),
            status: StepExecutionStatus::Compensating,
            started_at: Utc::now(),
            completed_at: None,
            result: None,
            error_message: None,
            compensation_result: None,
            retry_count: 0,
        };

        let mut execution = self
            .saga_repo
            .create_step_execution(step_execution)
            .await?;

        // Execute compensation logic
        let result = self.compensate_step_logic(step_name, context).await;

        match result {
            Ok(value) => {
                execution.status = StepExecutionStatus::Compensated;
                execution.completed_at = Some(Utc::now());
                execution.compensation_result = Some(value);
                self.saga_repo.update_step_execution(execution).await?;
                Ok(())
            }
            Err(e) => {
                execution.status = StepExecutionStatus::Failed;
                execution.completed_at = Some(Utc::now());
                execution.error_message = Some(e.clone());
                self.saga_repo.update_step_execution(execution).await?;
                Err(e)
            }
        }
    }

    /// Compensation logic (placeholder)
    async fn compensate_step_logic(
        &self,
        step_name: &str,
        _context: &SagaContext,
    ) -> Result<serde_json::Value, String> {
        eprintln!("Compensating step: {}", step_name);
        Ok(serde_json::json!({"compensated": true}))
    }
}

#[async_trait]
impl SagaOrchestrator for DefaultSagaOrchestrator {
    async fn start_saga(
        &self,
        saga_name: String,
        company_id: Uuid,
        initiator_id: Uuid,
        initial_context: serde_json::Value,
    ) -> Result<SagaInstance, String> {
        let saga_instance = SagaInstance {
            id: Uuid::new_v4(),
            saga_name: saga_name.clone(),
            company_id,
            status: SagaStatus::Pending,
            current_step: None,
            context: initial_context.clone(),
            started_at: Utc::now(),
            completed_at: None,
            error_message: None,
        };

        let mut instance = self.saga_repo.create_instance(saga_instance).await?;

        // Create saga context
        let mut context = SagaContext {
            saga_id: instance.id,
            company_id,
            initiator_id,
            state: initial_context,
            completed_steps: vec![],
        };

        // Update status to in_progress
        instance.status = SagaStatus::InProgress;
        instance = self.saga_repo.update_instance(instance).await?;

        // Execute saga steps (placeholder - in production, load step definitions)
        let steps = self.load_saga_steps(&saga_name)?;

        for step in &steps {
            instance.current_step = Some(step.step_name.clone());
            instance = self.saga_repo.update_instance(instance.clone()).await?;

            match self.execute_step(instance.id, step, &mut context).await {
                Ok(_) => continue,
                Err(e) => {
                    instance.status = SagaStatus::Failed;
                    instance.error_message = Some(e.clone());
                    instance.completed_at = Some(Utc::now());
                    instance = self.saga_repo.update_instance(instance).await?;

                    // Trigger compensation
                    let _ = self.compensate_saga(instance.id).await;

                    return Err(e);
                }
            }
        }

        // Mark as succeeded
        instance.status = SagaStatus::Succeeded;
        instance.completed_at = Some(Utc::now());
        instance.current_step = None;
        instance = self.saga_repo.update_instance(instance).await?;

        Ok(instance)
    }

    async fn get_saga_status(&self, saga_id: Uuid) -> Result<SagaInstance, String> {
        self.saga_repo
            .get_instance(saga_id)
            .await?
            .ok_or_else(|| format!("Saga {} not found", saga_id))
    }

    async fn retry_saga(&self, saga_id: Uuid) -> Result<(), String> {
        let mut instance = self.get_saga_status(saga_id).await?;

        if instance.status != SagaStatus::Failed {
            return Err(format!(
                "Saga {} is not in failed state (current: {:?})",
                saga_id, instance.status
            ));
        }

        // Reset status
        instance.status = SagaStatus::InProgress;
        instance.error_message = None;
        self.saga_repo.update_instance(instance).await?;

        // Restart from failed step (placeholder - in production, resume from last successful step)
        Ok(())
    }

    async fn compensate_saga(&self, saga_id: Uuid) -> Result<(), String> {
        let mut instance = self.get_saga_status(saga_id).await?;

        instance.status = SagaStatus::Compensating;
        instance = self.saga_repo.update_instance(instance).await?;

        // Get completed step executions
        let step_executions = self.saga_repo.list_step_executions(saga_id).await?;

        // Compensate in reverse order
        let completed_steps: Vec<_> = step_executions
            .into_iter()
            .filter(|e| e.status == StepExecutionStatus::Succeeded)
            .collect();

        let context = SagaContext {
            saga_id: instance.id,
            company_id: instance.company_id,
            initiator_id: Uuid::nil(), // Placeholder
            state: instance.context.clone(),
            completed_steps: vec![],
        };

        for step_execution in completed_steps.iter().rev() {
            if let Err(e) = self
                .compensate_step(saga_id, &step_execution.step_name, &context)
                .await
            {
                eprintln!(
                    "Failed to compensate step '{}': {}",
                    step_execution.step_name, e
                );
            }
        }

        instance.status = SagaStatus::Compensated;
        instance.completed_at = Some(Utc::now());
        self.saga_repo.update_instance(instance).await?;

        Ok(())
    }
}

impl DefaultSagaOrchestrator {
    /// Load saga step definitions by name (placeholder)
    fn load_saga_steps(&self, saga_name: &str) -> Result<Vec<SagaStep>, String> {
        match saga_name {
            "agent_hiring" => Ok(vec![
                SagaStep {
                    step_name: "validate_agent".to_string(),
                    timeout_seconds: 30,
                    retry_policy: RetryPolicy::default(),
                },
                SagaStep {
                    step_name: "create_environment".to_string(),
                    timeout_seconds: 60,
                    retry_policy: RetryPolicy::default(),
                },
                SagaStep {
                    step_name: "assign_initial_task".to_string(),
                    timeout_seconds: 30,
                    retry_policy: RetryPolicy::default(),
                },
                SagaStep {
                    step_name: "send_welcome_notification".to_string(),
                    timeout_seconds: 10,
                    retry_policy: RetryPolicy::default(),
                },
            ]),
            "issue_workflow" => Ok(vec![
                SagaStep {
                    step_name: "create_issue".to_string(),
                    timeout_seconds: 30,
                    retry_policy: RetryPolicy::default(),
                },
                SagaStep {
                    step_name: "checkout_to_agent".to_string(),
                    timeout_seconds: 30,
                    retry_policy: RetryPolicy::default(),
                },
                SagaStep {
                    step_name: "link_to_goal".to_string(),
                    timeout_seconds: 30,
                    retry_policy: RetryPolicy::default(),
                },
            ]),
            _ => Err(format!("Unknown saga: {}", saga_name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSagaRepository;
    struct MockEventBus;

    #[async_trait]
    impl SagaRepository for MockSagaRepository {
        async fn create_instance(&self, instance: SagaInstance) -> Result<SagaInstance, String> {
            Ok(instance)
        }

        async fn update_instance(&self, instance: SagaInstance) -> Result<SagaInstance, String> {
            Ok(instance)
        }

        async fn get_instance(&self, _saga_id: Uuid) -> Result<Option<SagaInstance>, String> {
            Ok(Some(SagaInstance {
                id: Uuid::new_v4(),
                saga_name: "test_saga".to_string(),
                company_id: Uuid::new_v4(),
                status: SagaStatus::Succeeded,
                current_step: None,
                context: serde_json::json!({}),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                error_message: None,
            }))
        }

        async fn create_step_execution(
            &self,
            execution: SagaStepExecution,
        ) -> Result<SagaStepExecution, String> {
            Ok(execution)
        }

        async fn update_step_execution(
            &self,
            execution: SagaStepExecution,
        ) -> Result<SagaStepExecution, String> {
            Ok(execution)
        }

        async fn list_step_executions(
            &self,
            _saga_id: Uuid,
        ) -> Result<Vec<SagaStepExecution>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl EventBus for MockEventBus {
        async fn publish(&self, _event: Box<dyn models::event_bus::Event>) -> Result<(), String> {
            Ok(())
        }

        async fn subscribe(
            &self,
            _handler: Box<dyn models::event_bus::EventHandler>,
        ) -> Result<(), String> {
            Ok(())
        }

        async fn unsubscribe(&self, _handler_name: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_start_saga() {
        let orchestrator = DefaultSagaOrchestrator::new(
            Arc::new(MockSagaRepository),
            Arc::new(MockEventBus),
        );

        let result = orchestrator
            .start_saga(
                "agent_hiring".to_string(),
                Uuid::new_v4(),
                Uuid::new_v4(),
                serde_json::json!({"agent_id": "test"}),
            )
            .await;

        assert!(result.is_ok());
        let instance = result.unwrap();
        assert_eq!(instance.status, SagaStatus::Succeeded);
    }

    #[tokio::test]
    async fn test_get_saga_status() {
        let orchestrator = DefaultSagaOrchestrator::new(
            Arc::new(MockSagaRepository),
            Arc::new(MockEventBus),
        );

        let result = orchestrator.get_saga_status(Uuid::new_v4()).await;
        assert!(result.is_ok());
    }
}
