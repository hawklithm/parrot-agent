use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use uuid::Uuid;

use crate::saga::{Saga, SagaStep, SagaStepResult};

/// Routine触发Saga - 协调Routine触发到Agent唤醒的完整流程
pub struct RoutineTriggerSaga<R, I, E, A> {
    routine_service: Arc<R>,
    issue_service: Arc<I>,
    environment_service: Option<Arc<E>>,
    agent_service: Arc<A>,
}

impl<R, I, E, A> RoutineTriggerSaga<R, I, E, A> {
    pub fn new(routine_service: Arc<R>, issue_service: Arc<I>, agent_service: Arc<A>) -> Self {
        Self {
            routine_service,
            issue_service,
            environment_service: None,
            agent_service,
        }
    }

    pub fn with_environment_service(mut self, environment_service: Arc<E>) -> Self {
        self.environment_service = Some(environment_service);
        self
    }
}

#[async_trait]
impl<R, I, E, A> Saga for RoutineTriggerSaga<R, I, E, A>
where
    R: RoutineRunService,
    I: IssueCreationService,
    E: EnvironmentLeaseService,
    A: AgentWakeupService,
{
    fn saga_name(&self) -> &str {
        "routine_trigger"
    }

    fn steps(&self) -> Vec<SagaStep> {
        vec![
            // Step 1: 创建RoutineRun记录
            SagaStep::new(
                "create_routine_run".to_string(),
                {
                    let service = Arc::clone(&self.routine_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let routine_id = Uuid::parse_str(context["routine_id"].as_str().unwrap_or("")).ok();
                            if routine_id.is_none() {
                                return SagaStepResult::Failure("Missing routine_id".to_string());
                            }
                            match service.create_run(routine_id.unwrap()).await {
                                Ok(run_id) => {
                                    let mut new_context = context.clone();
                                    new_context["run_id"] = json!(run_id.to_string());
                                    SagaStepResult::Success(new_context)
                                }
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.routine_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(run_id_str) = context["run_id"].as_str() {
                                if let Ok(run_id) = Uuid::parse_str(run_id_str) {
                                    return service.delete_run(run_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 2: 创建关联Issue
            SagaStep::new(
                "create_issue".to_string(),
                {
                    let service = Arc::clone(&self.issue_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let routine_id = Uuid::parse_str(context["routine_id"].as_str().unwrap_or("")).ok();
                            let company_id = Uuid::parse_str(context["company_id"].as_str().unwrap_or("")).ok();
                            if routine_id.is_none() || company_id.is_none() {
                                return SagaStepResult::Failure("Missing routine_id or company_id".to_string());
                            }
                            match service.create_for_routine(company_id.unwrap(), routine_id.unwrap()).await {
                                Ok(issue_id) => {
                                    let mut new_context = context.clone();
                                    new_context["issue_id"] = json!(issue_id.to_string());
                                    SagaStepResult::Success(new_context)
                                }
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.issue_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(issue_id_str) = context["issue_id"].as_str() {
                                if let Ok(issue_id) = Uuid::parse_str(issue_id_str) {
                                    return service.delete_issue(issue_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 3: Checkout Issue并分配给Agent
            SagaStep::new(
                "checkout_issue".to_string(),
                {
                    let service = Arc::clone(&self.issue_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let issue_id = Uuid::parse_str(context["issue_id"].as_str().unwrap_or("")).ok();
                            let agent_id = Uuid::parse_str(context["agent_id"].as_str().unwrap_or("")).ok();
                            if issue_id.is_none() || agent_id.is_none() {
                                return SagaStepResult::Failure("Missing issue_id or agent_id".to_string());
                            }
                            match service.checkout_and_assign(issue_id.unwrap(), agent_id.unwrap()).await {
                                Ok(()) => SagaStepResult::Success(context),
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.issue_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(issue_id_str) = context["issue_id"].as_str() {
                                if let Ok(issue_id) = Uuid::parse_str(issue_id_str) {
                                    return service.release_issue(issue_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 4: 获取Environment Lease（可选）
            SagaStep::new(
                "acquire_lease".to_string(),
                {
                    let service_opt = self.environment_service.clone();
                    move |context| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            let needs_environment = context["needs_environment"].as_bool().unwrap_or(false);
                            if !needs_environment {
                                return SagaStepResult::Success(context);
                            }
                            if let Some(service) = service_opt {
                                let environment_id = Uuid::parse_str(context["environment_id"].as_str().unwrap_or("")).ok();
                                let agent_id = Uuid::parse_str(context["agent_id"].as_str().unwrap_or("")).ok();
                                if environment_id.is_none() || agent_id.is_none() {
                                    return SagaStepResult::Failure("Missing environment_id or agent_id".to_string());
                                }
                                match service.acquire_lease(environment_id.unwrap(), agent_id.unwrap()).await {
                                    Ok(lease_id) => {
                                        let mut new_context = context.clone();
                                        new_context["lease_id"] = json!(lease_id.to_string());
                                        SagaStepResult::Success(new_context)
                                    }
                                    Err(e) => SagaStepResult::Failure(e),
                                }
                            } else {
                                SagaStepResult::Success(context)
                            }
                        })
                    }
                },
                Some({
                    let service_opt = self.environment_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                if let Some(lease_id_str) = context["lease_id"].as_str() {
                                    if let Ok(lease_id) = Uuid::parse_str(lease_id_str) {
                                        return service.release_lease(lease_id).await;
                                    }
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 5: 唤醒Agent
            SagaStep::new(
                "wakeup_agent".to_string(),
                {
                    let service = Arc::clone(&self.agent_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let agent_id = Uuid::parse_str(context["agent_id"].as_str().unwrap_or("")).ok();
                            let issue_id = Uuid::parse_str(context["issue_id"].as_str().unwrap_or("")).ok();
                            if agent_id.is_none() || issue_id.is_none() {
                                return SagaStepResult::Failure("Missing agent_id or issue_id".to_string());
                            }
                            match service.wakeup(agent_id.unwrap(), issue_id.unwrap()).await {
                                Ok(()) => SagaStepResult::Success(context),
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                None,
            ),
        ]
    }
}

// ==================== Service traits ====================

#[async_trait]
pub trait RoutineRunService: Send + Sync {
    async fn create_run(&self, routine_id: Uuid) -> Result<Uuid, String>;
    async fn delete_run(&self, run_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait IssueCreationService: Send + Sync {
    async fn create_for_routine(&self, company_id: Uuid, routine_id: Uuid) -> Result<Uuid, String>;
    async fn delete_issue(&self, issue_id: Uuid) -> Result<(), String>;
    async fn checkout_and_assign(&self, issue_id: Uuid, agent_id: Uuid) -> Result<(), String>;
    async fn release_issue(&self, issue_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait EnvironmentLeaseService: Send + Sync {
    async fn acquire_lease(&self, environment_id: Uuid, agent_id: Uuid) -> Result<Uuid, String>;
    async fn release_lease(&self, lease_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait AgentWakeupService: Send + Sync {
    async fn wakeup(&self, agent_id: Uuid, issue_id: Uuid) -> Result<(), String>;
}
