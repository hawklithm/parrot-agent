use async_trait::async_trait;
use repositories::{ActivityLogRepository, AgentRepository, BudgetPolicyRepository};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

use crate::{AgentService, CreateAgentInput, ServiceError};
use models::{Agent, AgentStatus, Approval, ApprovalType};
use models::budget::{BudgetPolicy, BudgetScopeType, BudgetWindowKind};
use repositories::activity_log_repository::{Activity, ActorType, ActivityAction, ResourceType};

/// Hire Agent Payload - 从审批 payload 解析的数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HireAgentPayload {
    pub name: String,
    pub role: models::AgentRole,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub reports_to: Option<Uuid>,
    pub capabilities: Option<String>,
    pub adapter_type: String,
    pub adapter_config: serde_json::Value,
    pub runtime_config: Option<serde_json::Value>,
    pub permissions: Option<models::AgentPermissions>,
    pub budget_monthly_cents: Option<i32>,
    pub default_environment_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
    pub desired_skills: Option<Vec<String>>,
    pub instructions_bundle: Option<serde_json::Value>,
    /// 如果已有 pending_approval Agent，这里存储其 ID
    pub agent_id: Option<Uuid>,
}

impl HireAgentPayload {
    /// 从 JSON payload 解析
    pub fn from_json(payload: &serde_json::Value) -> Result<Self, ServiceError> {
        let name = payload
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServiceError::InvalidInput("Missing 'name' in payload".to_string()))?
            .to_string();

        let role = payload
            .get("role")
            .and_then(|v| v.as_str())
            .and_then(|s| serde_json::from_str::<models::AgentRole>(&format!("\"{}\"", s)).ok())
            .ok_or_else(|| ServiceError::InvalidInput("Missing or invalid 'role' in payload".to_string()))?;

        let adapter_type = payload
            .get("adapterType")
            .or_else(|| payload.get("adapter_type"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServiceError::InvalidInput("Missing 'adapterType' in payload".to_string()))?
            .to_string();

        let title = payload.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        let icon = payload.get("icon").and_then(|v| v.as_str()).map(|s| s.to_string());
        let reports_to = payload
            .get("reportsTo")
            .or_else(|| payload.get("reports_to"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let capabilities = payload.get("capabilities").and_then(|v| v.as_str()).map(|s| s.to_string());
        let adapter_config = payload
            .get("adapterConfig")
            .or_else(|| payload.get("adapter_config"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let runtime_config = payload
            .get("runtimeConfig")
            .or_else(|| payload.get("runtime_config"))
            .cloned();
        let permissions = payload.get("permissions").and_then(|v| serde_json::from_value(v.clone()).ok());
        let budget_monthly_cents = payload
            .get("budgetMonthlyCents")
            .or_else(|| payload.get("budget_monthly_cents"))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32);
        let default_environment_id = payload
            .get("defaultEnvironmentId")
            .or_else(|| payload.get("default_environment_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let metadata = payload.get("metadata").cloned();
        let desired_skills = payload
            .get("desiredSkills")
            .or_else(|| payload.get("ls"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect());
        let instructions_bundle = payload
            .get("instructionsBundle")
            .or_else(|| payload.get("instructions_bundle"))
            .cloned();
        let agent_id = payload
            .get("agentId")
            .or_else(|| payload.get("agent_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        Ok(Self {
            name,
            role,
            title,
            icon,
            reports_to,
            capabilities,
            adapter_type,
            adapter_config,
            runtime_config,
            permissions,
            budget_monthly_cents,
            default_environment_id,
            metadata,
            desired_skills,
            instructions_bundle,
            agent_id,
        })
    }
}

/// Approval Execution Result
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalExecutionResult {
    pub agent_id: Uuid,
    pub agent: Agent,
    pub budget_created: bool,
}

/// Approval Executor - 执行审批通过后的动作
#[async_trait]
pub trait ApprovalExecutor: Send + Sync {
    /// 执行 hire_agent 审批通过后的动作
    async fn execute_hire_agent(
        &self,
        approval: &Approval,
        decided_by_user_id: Uuid,
    ) -> Result<ApprovalExecutionResult, ServiceError>;
}

/// Default Approval Executor Implementation
pub struct DefaultApprovalExecutor {
    pool: PgPool,
    agent_service: Arc<dyn AgentService>,
    agent_repo: Arc<dyn AgentRepository>,
    budget_repo: Arc<dyn BudgetPolicyRepository>,
}

impl DefaultApprovalExecutor {
    pub fn new(
        pool: PgPool,
        agent_service: Arc<dyn AgentService>,
        agent_repo: Arc<dyn AgentRepository>,
        budget_repo: Arc<dyn BudgetPolicyRepository>,
    ) -> Self {
        Self {
            pool,
            agent_service,
            agent_repo,
            budget_repo,
        }
    }

    /// 激活已存在的 pending_approval Agent
    async fn activate_pending_agent(&self, agent_id: Uuid) -> Result<Agent, ServiceError> {
        let agent = self.agent_repo.get_by_id(agent_id).await?;

        if agent.status != AgentStatus::PendingApproval {
            return Err(ServiceError::InvalidInput(format!(
                "Agent {} is not in pending_approval status (current: {:?})",
                agent_id, agent.status
            )));
        }

        // 只调用 AgentService，让它负责记录日志
        let updated_agent = self.agent_service.set_status(agent_id, AgentStatus::Idle).await?;

        tracing::info!(
            agent_id = %agent_id,
            agent_name = %agent.name,
            "Agent activated from approval"
        );

        Ok(updated_agent)
    }

    /// 创建全新的 Agent
    async fn create_new_agent(
        &self,
        company_id: Uuid,
        payload: &HireAgentPayload,
        decided_by_user_id: Uuid,
    ) -> Result<Agent, ServiceError> {
        let input = CreateAgentInput {
            company_id,
            name: payload.name.clone(),
            role: payload.role.clone(),
            status: Some(AgentStatus::Idle),
            adapter_type: payload.adapter_type.clone(),
            adapter_config: payload.adapter_config.clone(),
            runtime_config: payload.runtime_config.clone(),
            permissions: payload.permissions.clone(),
            budget_monthly_cents: payload.budget_monthly_cents,
            reports_to: payload.reports_to,
        };

        // AgentService 负责记录创建日志
        let agent = self.agent_service.create(input).await?;

        tracing::info!(
            agent_id = %agent.id,
            agent_name = %agent.name,
            agent_role = ?agent.role,
            "Agent created from approval"
        );

        Ok(agent)
    }

    /// 创建预算策略
    async fn create_budget_policy(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        budget_monthly_cents: i32,
        decided_by_user_id: Uuid,
    ) -> Result<bool, ServiceError> {
        if budget_monthly_cents <= 0 {
            return Ok(false);
        }

        let policy = models::budget::BudgetPolicy {
            id: Uuid::new_v4(),
            company_id,
            scope_type: models::budget::BudgetScopeType::Agent,
            scope_id: agent_id,
            metric: models::budget::BudgetMetric::BilledCents,
            window_kind: models::budget::BudgetWindowKind::CalendarMonthUtc,
            amount: budget_monthly_cents as i64,
            warn_percent: 80,
            hard_stop_enabled: true,
            notify_enabled: true,
            is_active: true,
            created_by_user_id: Some(decided_by_user_id),
            updated_by_user_id: Some(decided_by_user_id),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // BudgetService 负责记录预算日志
        self.budget_repo.upsert(&policy).await?;

        tracing::info!(
            agent_id = %agent_id,
            budget_monthly_cents = %budget_monthly_cents,
            "Budget policy created from approval"
        );

        Ok(true)
    }
}

#[async_trait]
impl ApprovalExecutor for DefaultApprovalExecutor {
    async fn execute_hire_agent(
        &self,
        approval: &Approval,
        decided_by_user_id: Uuid,
    ) -> Result<ApprovalExecutionResult, ServiceError> {
        let payload = HireAgentPayload::from_json(&approval.payload)?;

        // 创建或激活 Agent
        let agent = if let Some(agent_id) = payload.agent_id {
            self.activate_pending_agent(agent_id).await?
        } else {
            self.create_new_agent(approval.company_id, &payload, decided_by_user_id).await?
        };

        // 创建预算策略（如果需要）
        let budget_created = if let Some(budget) = payload.budget_monthly_cents {
            self.create_budget_policy(approval.company_id, agent.id, budget, decided_by_user_id).await?
        } else {
            false
        };

        Ok(ApprovalExecutionResult {
            agent_id: agent.id,
            agent,
            budget_created,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hire_agent_payload_minimal() {
        let payload = serde_json::json!({
            "name": "Test Agent",
            "role": "engineer",
            "adapterType": "anthropic",
        });

        let result = HireAgentPayload::from_json(&payload);
        assert!(result.is_ok());

        let parsed = result.unwrap();
        assert_eq!(parsed.name, "Test Agent");
        assert_eq!(parsed.adapter_type, "anthropic");
        assert!(parsed.agent_id.is_none());
    }
}
