use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use uuid::Uuid;

use crate::saga::{Saga, SagaStep, SagaStepResult};

/// Agent雇佣Saga - 协调Agent创建的多步骤流程
pub struct AgentHiringSaga<A, P, I, B> {
    agent_service: Arc<A>,
    approval_service: Option<Arc<P>>,
    instructions_service: Option<Arc<I>>,
    budget_service: Option<Arc<B>>,
}

impl<A, P, I, B> AgentHiringSaga<A, P, I, B> {
    pub fn new(agent_service: Arc<A>) -> Self {
        Self {
            agent_service,
            approval_service: None,
            instructions_service: None,
            budget_service: None,
        }
    }

    pub fn with_approval_service(mut self, approval_service: Arc<P>) -> Self {
        self.approval_service = Some(approval_service);
        self
    }

    pub fn with_instructions_service(mut self, instructions_service: Arc<I>) -> Self {
        self.instructions_service = Some(instructions_service);
        self
    }

    pub fn with_budget_service(mut self, budget_service: Arc<B>) -> Self {
        self.budget_service = Some(budget_service);
        self
    }
}

#[async_trait]
impl<A, P, I, B> Saga for AgentHiringSaga<A, P, I, B>
where
    A: AgentCreationService,
    P: ApprovalCreationService,
    I: InstructionsMaterializationService,
    B: BudgetPolicyService,
{
    fn saga_name(&self) -> &str {
        "agent_hiring"
    }

    fn steps(&self) -> Vec<SagaStep> {
        vec![
            // Step 1: 创建Agent记录
            SagaStep::new(
                "create_agent".to_string(),
                {
                    let service = Arc::clone(&self.agent_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            let agent_name = context["agent_name"].as_str().unwrap_or("unknown");
                            let company_id = Uuid::parse_str(context["company_id"].as_str().unwrap_or("")).ok();

                            if company_id.is_none() {
                                return SagaStepResult::Failure("Missing company_id".to_string());
                            }

                            match service.create_agent(company_id.unwrap(), agent_name).await {
                                Ok(agent_id) => {
                                    let mut new_context = context.clone();
                                    new_context["agent_id"] = json!(agent_id.to_string());
                                    SagaStepResult::Success(new_context)
                                }
                                Err(e) => SagaStepResult::Failure(e),
                            }
                        })
                    }
                },
                Some({
                    let service = Arc::clone(&self.agent_service);
                    move |context: JsonValue| {
                        let service = Arc::clone(&service);
                        Box::pin(async move {
                            if let Some(agent_id_str) = context["agent_id"].as_str() {
                                if let Ok(agent_id) = Uuid::parse_str(agent_id_str) {
                                    return service.delete_agent(agent_id).await;
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 2: 创建Approval记录（如需审批）
            SagaStep::new(
                "create_approval".to_string(),
                {
                    let service_opt = self.approval_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            let needs_approval = context["needs_approval"].as_bool().unwrap_or(false);

                            if !needs_approval {
                                return SagaStepResult::Success(context);
                            }

                            if let Some(service) = service_opt {
                                let agent_id_str = context["agent_id"].as_str().unwrap_or("");
                                let agent_id = Uuid::parse_str(agent_id_str).ok();

                                if agent_id.is_none() {
                                    return SagaStepResult::Failure("Missing agent_id".to_string());
                                }

                                match service.create_approval(agent_id.unwrap()).await {
                                    Ok(approval_id) => {
                                        let mut new_context = context.clone();
                                        new_context["approval_id"] = json!(approval_id.to_string());
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
                    let service_opt = self.approval_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                if let Some(approval_id_str) = context["approval_id"].as_str() {
                                    if let Ok(approval_id) = Uuid::parse_str(approval_id_str) {
                                        return service.delete_approval(approval_id).await;
                                    }
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 3: 物化指令集
            SagaStep::new(
                "materialize_instructions".to_string(),
                {
                    let service_opt = self.instructions_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                let agent_id_str = context["agent_id"].as_str().unwrap_or("");
                                let agent_id = Uuid::parse_str(agent_id_str).ok();

                                if agent_id.is_none() {
                                    return SagaStepResult::Failure("Missing agent_id".to_string());
                                }

                                match service.materialize_bundle(agent_id.unwrap()).await {
                                    Ok(bundle_id) => {
                                        let mut new_context = context.clone();
                                        new_context["bundle_id"] = json!(bundle_id.to_string());
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
                    let service_opt = self.instructions_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                if let Some(bundle_id_str) = context["bundle_id"].as_str() {
                                    if let Ok(bundle_id) = Uuid::parse_str(bundle_id_str) {
                                        return service.cleanup_bundle(bundle_id).await;
                                    }
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),

            // Step 4: 创建Budget Policy记录
            SagaStep::new(
                "create_budget_policy".to_string(),
                {
                    let service_opt = self.budget_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                let agent_id_str = context["agent_id"].as_str().unwrap_or("");
                                let agent_id = Uuid::parse_str(agent_id_str).ok();
                                let budget = context["budget_monthly_cents"].as_i64().unwrap_or(0) as i32;

                                if agent_id.is_none() {
                                    return SagaStepResult::Failure("Missing agent_id".to_string());
                                }

                                match service.create_policy(agent_id.unwrap(), budget).await {
                                    Ok(policy_id) => {
                                        let mut new_context = context.clone();
                                        new_context["policy_id"] = json!(policy_id.to_string());
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
                    let service_opt = self.budget_service.clone();
                    move |context: JsonValue| {
                        let service_opt = service_opt.clone();
                        Box::pin(async move {
                            if let Some(service) = service_opt {
                                if let Some(policy_id_str) = context["policy_id"].as_str() {
                                    if let Ok(policy_id) = Uuid::parse_str(policy_id_str) {
                                        return service.delete_policy(policy_id).await;
                                    }
                                }
                            }
                            Ok(())
                        })
                    }
                }),
            ),
        ]
    }
}

// ==================== Service trait定义 ====================

#[async_trait]
pub trait AgentCreationService: Send + Sync {
    async fn create_agent(&self, company_id: Uuid, name: &str) -> Result<Uuid, String>;
    async fn delete_agent(&self, agent_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait ApprovalCreationService: Send + Sync {
    async fn create_approval(&self, agent_id: Uuid) -> Result<Uuid, String>;
    async fn delete_approval(&self, approval_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait InstructionsMaterializationService: Send + Sync {
    async fn materialize_bundle(&self, agent_id: Uuid) -> Result<Uuid, String>;
    async fn cleanup_bundle(&self, bundle_id: Uuid) -> Result<(), String>;
}

#[async_trait]
pub trait BudgetPolicyService: Send + Sync {
    async fn create_policy(&self, agent_id: Uuid, budget_cents: i32) -> Result<Uuid, String>;
    async fn delete_policy(&self, policy_id: Uuid) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAgentService;

    #[async_trait]
    impl AgentCreationService for MockAgentService {
        async fn create_agent(&self, _company_id: Uuid, _name: &str) -> Result<Uuid, String> {
            Ok(Uuid::new_v4())
        }

        async fn delete_agent(&self, _agent_id: Uuid) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_saga_name() {
        let saga = AgentHiringSaga::new(Arc::new(MockAgentService));
        assert_eq!(saga.saga_name(), "agent_hiring");
    }

    #[test]
    fn test_saga_steps_count() {
        let saga = AgentHiringSaga::new(Arc::new(MockAgentService));
        let steps = saga.steps();
        assert_eq!(steps.len(), 4, "Agent hiring saga should have 4 steps");
    }
}
