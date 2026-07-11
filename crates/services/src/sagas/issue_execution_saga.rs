use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use uuid::Uuid;

use crate::saga::{Saga, SagaStep, SagaStepResult};

/// Issue执行Saga - 协调Issue从checkout到Agent唤醒的完整流程
pub struct IssueExecutionSaga<I, E, W, R, A> {
    issue_service: Arc<I>,
    environment_service: Arc<E>,
    workspace_service: Arc<W>,
    runtime_service: Arc<R>,
    agent_service: Arc<A>,
}

impl<I, E, W, R, A> IssueExecutionSaga<I, E, W, R, A> {
    pub fn new(
        issue_service: Arc<I>,
        environment_service: Arc<E>,
        workspace_service: Arc<W>,
        runtime_service: Arc<R>,
        agent_service: Arc<A>,
    ) -> Self {
        Self {
            issue_service,
            environment_service,
            workspace_service,
            runtime_service,
            agent_service,
        }
    }
}

#[async_trait]
impl<I, E, W, R, A> Saga for IssueExecutionSaga<I, E, W, R, A>
where
    I: IssueCheckoutService,
    E: EnvironmentLeaseService,
    W: WorkspaceCreationService,
    R: RuntimeStartService,
    A: AgentWakeupService,
{
    fn saga_name(&self) -> &str {
        "issue_execution"
    }

    fn steps(&self) -> Vec<SagaStep> {
        vec![
            // Step 1: Checkout Issue
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

                            match service.checkout(issue_id.unwrap(), agent_id.unwrap()).await {
                                Ok(()) => {
                                    let mut new_context = context.clone();
                                    new_context["checkout_completed"] = json!(true);
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
                                    return service.release(issue_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 2: 获取Environment Lease
            SagaStep::new(
                "acquire_lease".to_string(),
                {
                    let service = Arc::clone(&self.environment_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
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
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.environment_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(lease_id_str) = context["lease_id"].as_str() {
                                if let Ok(lease_id) = Uuid::parse_str(lease_id_str) {
                                    return service.release_lease(lease_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 3: 创建Execution Workspace
            SagaStep::new(
                "create_workspace".to_string(),
                {
                    let service = Arc::clone(&self.workspace_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let issue_id = Uuid::parse_str(context["issue_id"].as_str().unwrap_or("")).ok();
                            let lease_id = Uuid::parse_str(context["lease_id"].as_str().unwrap_or("")).ok();

                            if issue_id.is_none() || lease_id.is_none() {
                                return SagaStepResult::Failure("Missing issue_id or lease_id".to_string());
                            }

                            match service.create_workspace(issue_id.unwrap(), lease_id.unwrap()).await {
                                Ok(workspace_id) => {
                                    let mut new_context = context.clone();
                                    new_context["workspace_id"] = json!(workspace_id.to_string());
                                    SagaStepResult::Success(new_context)
                                }
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.workspace_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(workspace_id_str) = context["workspace_id"].as_str() {
                                if let Ok(workspace_id) = Uuid::parse_str(workspace_id_str) {
                                    return service.cleanup_workspace(workspace_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 4: 启动Runtime Services
            SagaStep::new(
                "start_runtime".to_string(),
                {
                    let service = Arc::clone(&self.runtime_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let workspace_id = Uuid::parse_str(context["workspace_id"].as_str().unwrap_or("")).ok();

                            if workspace_id.is_none() {
                                return SagaStepResult::Failure("Missing workspace_id".to_string());
                            }

                            match service.start_runtime(workspace_id.unwrap()).await {
                                Ok(runtime_id) => {
                                    let mut new_context = context.clone();
                                    new_context["runtime_id"] = json!(runtime_id.to_string());
                                    SagaStepResult::Success(new_context)
                                }
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.runtime_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(runtime_id_str) = context["runtime_id"].as_str() {
                                if let Ok(runtime_id) = Uuid::parse_str(runtime_id_str) {
                                    return service.stop_runtime(runtime_id).await;
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
                None, // 唤醒Agent无需补偿
            ),
        ]
    }
}

// ==================== Service trait定义 ====================

#[async_trait]
pub trait IssueCheckoutService: Send + Sync {
    async fn checkout(&self, issue_id: Uuid, agent_id: Uuid) -> Result<(), String>;
    async fn release(&self, issue_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait EnvironmentLeaseService: Send + Sync {
    async fn acquire_lease(&self, environment_id: Uuid, agent_id: Uuid) -> Result<Uuid, String>;
    async fn release_lease(&self, lease_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait WorkspaceCreationService: Send + Sync {
    async fn create_workspace(&self, issue_id: Uuid, lease_id: Uuid) -> Result<Uuid, String>;
    async fn cleanup_workspace(&self, workspace_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait RuntimeStartService: Send + Sync {
    async fn start_runtime(&self, workspace_id: Uuid) -> Result<Uuid, String>;
    async fn stop_runtime(&self, runtime_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait AgentWakeupService: Send + Sync {
    async fn wakeup(&self, agent_id: Uuid, issue_id: Uuid) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockIssueService;
    struct MockEnvironmentService;
    struct MockWorkspaceService;
    struct MockRuntimeService;
    struct MockAgentService;

    #[async_trait]
    impl IssueCheckoutService for MockIssueService {
        async fn checkout(&self, _issue_id: Uuid, _agent_id: Uuid) -> Result<(), String> {
            Ok(())
        }
        async fn release(&self, _issue_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[async_trait]
    impl EnvironmentLeaseService for MockEnvironmentService {
        async fn acquire_lease(&self, _environment_id: Uuid, _agent_id: Uuid) -> Result<Uuid, String> {
            Ok(Uuid::new_v4())
        }
        async fn release_lease(&self, _lease_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[async_trait]
    impl WorkspaceCreationService for MockWorkspaceService {
        async fn create_workspace(&self, _issue_id: Uuid, _lease_id: Uuid) -> Result<Uuid, String> {
            Ok(Uuid::new_v4())
        }
        async fn cleanup_workspace(&self, _workspace_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[async_trait]
    impl RuntimeStartService for MockRuntimeService {
        async fn start_runtime(&self, _workspace_id: Uuid) -> Result<Uuid, String> {
            Ok(Uuid::new_v4())
        }
        async fn stop_runtime(&self, _runtime_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[async_trait]
    impl AgentWakeupService for MockAgentService {
        async fn wakeup(&self, _agent_id: Uuid, _issue_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_saga_name() {
        let saga = IssueExecutionSaga::new(
            Arc::new(MockIssueService),
            Arc::new(MockEnvironmentService),
            Arc::new(MockWorkspaceService),
            Arc::new(MockRuntimeService),
            Arc::new(MockAgentService),
        );
        assert_eq!(saga.saga_name(), "issue_execution");
    }

    #[test]
    fn test_saga_steps_count() {
        let saga = IssueExecutionSaga::new(
            Arc::new(MockIssueService),
            Arc::new(MockEnvironmentService),
            Arc::new(MockWorkspaceService),
            Arc::new(MockRuntimeService),
            Arc::new(MockAgentService),
        );
        let steps = saga.steps();
        assert_eq!(steps.len(), 5, "Issue execution saga should have 5 steps");
    }
}
