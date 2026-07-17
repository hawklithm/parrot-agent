use async_trait::async_trait;
use uuid::Uuid;
use thiserror::Error;

use crate::built_in_agent_service::{
    BuiltInAgentDefinition, BuiltInAgentKey, BuiltInAgentMetadataRegistry, BuiltInAgentStatus,
};
use repositories;

#[derive(Debug, Error)]
pub enum BuiltInAgentError {
    #[error("Built-in agent not found: {0}")]
    NotFound(BuiltInAgentKey),

    #[error("Agent repository error: {0}")]
    RepositoryError(String),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Provision failed: {0}")]
    ProvisionFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type BuiltInAgentResult<T> = Result<T, BuiltInAgentError>;

/// 内置Agent服务接口
#[async_trait]
pub trait BuiltInAgentService: Send + Sync {
    /// 初始化（Provision）内置Agent
    ///
    /// 查找定义 -> 创建/获取Agent -> 绑定资源
    ///
    /// `input` 允许调用方覆盖内置 Agent 的默认配置（适配器类型、配置、预算）。
    /// 如果 Agent 已存在，传入 `input` 会更新其配置。
    async fn provision(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent>;

    /// 获取内置Agent的当前状态
    async fn get_status(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<BuiltInAgentStatus>;

    /// 重置内置Agent
    ///
    /// 清除资源 + 恢复初始状态
    async fn reset(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<()>;

    /// 协调（Reconcile）内置Agent资源
    ///
    /// 检测并修复资源漂移
    async fn reconcile(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<ReconcileResult>;

    /// 列举所有可用的内置Agent定义
    fn list_definitions(&self) -> Vec<&BuiltInAgentDefinition>;

    /// 获取特定内置Agent的定义
    fn get_definition(&self, key: BuiltInAgentKey) -> Option<&BuiltInAgentDefinition>;
}

/// 资源协调结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReconcileResult {
    pub agent_updated: bool,
    pub instructions_materialized: bool,
    pub skills_synced: bool,
    pub routines_synced: bool,
    pub changes: Vec<String>,
}

impl Default for ReconcileResult {
    fn default() -> Self {
        Self {
            agent_updated: false,
            instructions_materialized: false,
            skills_synced: false,
            routines_synced: false,
            changes: Vec::new(),
        }
    }
}

/// Provision 输入参数，允许用户覆盖内置 Agent 的默认配置
#[derive(Debug, Clone)]
pub struct ProvisionInput {
    /// 自定义适配器类型（覆盖定义中的默认值）
    pub adapter_type: Option<String>,
    /// 自定义适配器配置（覆盖定义中的默认值）
    pub adapter_config: Option<serde_json::Value>,
    /// 自定义月度预算（覆盖定义中的默认值）
    pub budget_monthly_cents: Option<i32>,
}

/// 默认的内置Agent服务实现
pub struct DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    registry: BuiltInAgentMetadataRegistry,
    agent_repo: std::sync::Arc<A>,
}

impl<A> DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    pub fn new(agent_repo: std::sync::Arc<A>) -> Self {
        Self {
            registry: BuiltInAgentMetadataRegistry::new(),
            agent_repo,
        }
    }

    /// 查找公司的唯一根Agent
    async fn find_single_root_agent(&self, company_id: Uuid) -> BuiltInAgentResult<Option<Uuid>> {
        let agents = self
            .agent_repo
            .list_by_company(company_id, repositories::ListAgentsOptions::default())
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        // 查找没有上级的Agent（根Agent）
        let root_agents: Vec<_> = agents
            .iter()
            .filter(|a| a.reports_to.is_none())
            .collect();

        if root_agents.len() == 1 {
            Ok(Some(root_agents[0].id))
        } else {
            Ok(None)
        }
    }

    /// 根据定义和用户输入创建Agent
    async fn create_agent_from_definition(
        &self,
        company_id: Uuid,
        definition: &BuiltInAgentDefinition,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent> {
        // 解析默认上级
        let reports_to = if let Some(ref manager) = definition.default_manager {
            if manager == "single_root_agent" {
                self.find_single_root_agent(company_id).await?
            } else {
                None
            }
        } else {
            None
        };

        // 确定适配器类型：用户输入 > 定义默认
        let adapter_type = input
            .and_then(|i| i.adapter_type.clone())
            .or_else(|| {
                definition
                    .allowed_adapter_types
                    .as_ref()
                    .and_then(|types| types.first())
                    .cloned()
            })
            .unwrap_or_else(|| "process".to_string());

        // 确定适配器配置：用户输入 > 空对象
        let adapter_config = input
            .and_then(|i| i.adapter_config.clone())
            .unwrap_or(serde_json::json!({}));

        // 确定预算：用户输入 > 定义默认 > 0
        let budget = input
            .and_then(|i| i.budget_monthly_cents)
            .or(definition.default_budget_monthly_cents)
            .unwrap_or(0);

        let agent = models::Agent {
            id: Uuid::new_v4(),
            company_id,
            name: definition.display_name.clone(),
            role: definition.default_role,
            status: definition.default_status.unwrap_or(models::AgentStatus::Idle),
            adapter_type,
            adapter_config: sqlx::types::Json(adapter_config),
            runtime_config: sqlx::types::Json(serde_json::json!({})),
            permissions: sqlx::types::Json(
                definition
                    .default_permissions
                    .clone()
                    .unwrap_or_default(),
            ),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: Some(true),
                built_in_key: Some(definition.key.as_str().to_string()),
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: budget,
            reports_to,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.agent_repo
            .create(agent)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))
    }

    /// 查找已存在的内置Agent
    async fn find_existing_agent(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<Option<models::Agent>> {
        let agents = self
            .agent_repo
            .list_by_company(company_id, repositories::ListAgentsOptions {
                include_terminated: true,
                limit: None,
                offset: None,
            })
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        Ok(agents
            .into_iter()
            .find(|a| {
                a.metadata
                    .0
                    .built_in_key
                    .as_ref()
                    .map(|k| k == key.as_str())
                    .unwrap_or(false)
            }))
    }

    /// 物化指令文件（stub - 完整实现需要文件系统/工作区服务）
    ///
    /// 在完整实现中，此方法将：
    /// 1. 从定义中获取指令内容
    /// 2. 写入工作区指定路径
    /// 3. 更新 Agent 的指令路径配置
    async fn materialize_instructions(
        &self,
        _agent: &mut models::Agent,
        _definition: &BuiltInAgentDefinition,
    ) -> BuiltInAgentResult<()> {
        // TODO: 实现指令文件物化
        // 1. 创建工作区目录
        // 2. 写入指令文件（入口文件 + 子文件）
        // 3. 更新 adapter_config 中的指令路径
        Ok(())
    }
}

#[async_trait]
impl<A> BuiltInAgentService for DefaultBuiltInAgentService<A>
where
    A: repositories::AgentRepository,
{
    async fn provision(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
        input: Option<&ProvisionInput>,
    ) -> BuiltInAgentResult<models::Agent> {
        // 获取定义
        let definition = self
            .registry
            .get_definition(key)
            .ok_or(BuiltInAgentError::NotFound(key))?;

        // 检查是否已存在
        if let Some(existing) = self.find_existing_agent(company_id, key).await? {
            // 已存在的Agent：更新配置（如果提供了自定义参数）
            if let Some(input) = input {
                let mut updated = existing;
                if let Some(ref adapter_type) = input.adapter_type {
                    updated.adapter_type = adapter_type.clone();
                }
                if let Some(ref adapter_config) = input.adapter_config {
                    updated.adapter_config = sqlx::types::Json(adapter_config.clone());
                }
                if let Some(budget) = input.budget_monthly_cents {
                    updated.budget_monthly_cents = budget;
                }
                updated.updated_at = chrono::Utc::now();

                let saved = self
                    .agent_repo
                    .update(updated)
                    .await
                    .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

                // 物化指令文件
                let mut mutable_agent = saved;
                self.materialize_instructions(&mut mutable_agent, definition).await?;

                return Ok(mutable_agent);
            }
            return Ok(existing);
        }

        // 创建新Agent（传入用户自定义配置）
        let mut agent = self
            .create_agent_from_definition(company_id, definition, input)
            .await?;

        // 物化指令文件
        self.materialize_instructions(&mut agent, definition).await?;

        // TODO: 物化技能、例程
        // - materialize_skill(): 创建/同步 Skill
        // - materialize_routine(): 创建/更新 Routine

        // TODO: 审批流程检查
        // 如果公司需要 board approval，创建 Approval 记录
        // 当前简化实现，直接返回已创建的Agent

        Ok(agent)
    }

    async fn get_status(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<BuiltInAgentStatus> {
        let agent = self.find_existing_agent(company_id, key).await?;
        Ok(crate::built_in_agent_service::derive_built_in_agent_status(
            agent.as_ref(),
            None,
        ))
    }

    async fn reset(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<()> {
        // 查找Agent
        let agent = self
            .find_existing_agent(company_id, key)
            .await?
            .ok_or(BuiltInAgentError::NotFound(key))?;

        // 重置为初始状态
        let definition = self
            .registry
            .get_definition(key)
            .ok_or(BuiltInAgentError::NotFound(key))?;

        let mut updated_agent = agent;
        updated_agent.status = definition.default_status.unwrap_or(models::AgentStatus::Idle);
        updated_agent.adapter_config = sqlx::types::Json(serde_json::json!({}));
        updated_agent.runtime_config = sqlx::types::Json(serde_json::json!({}));

        self.agent_repo
            .update(updated_agent)
            .await
            .map_err(|e| BuiltInAgentError::RepositoryError(e.to_string()))?;

        // TODO: 清理指令文件、技能、例程资源

        Ok(())
    }

    async fn reconcile(
        &self,
        company_id: Uuid,
        key: BuiltInAgentKey,
    ) -> BuiltInAgentResult<ReconcileResult> {
        let mut result = ReconcileResult::default();

        // 检查Agent是否存在
        let agent = self.find_existing_agent(company_id, key).await?;
        if agent.is_none() {
            result.changes.push("Agent not provisioned".to_string());
            return Ok(result);
        }

        // TODO: 检测并修复资源漂移
        // - 检查指令文件是否存在且最新
        // - 检查技能是否已绑定
        // - 检查例程是否已创建

        result.changes.push("Reconciliation completed (stub)".to_string());
        Ok(result)
    }

    fn list_definitions(&self) -> Vec<&BuiltInAgentDefinition> {
        self.registry.list_definitions()
    }

    fn get_definition(&self, key: BuiltInAgentKey) -> Option<&BuiltInAgentDefinition> {
        self.registry.get_definition(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconcile_result_default() {
        let result = ReconcileResult::default();
        assert!(!result.agent_updated);
        assert!(!result.instructions_materialized);
        assert!(!result.skills_synced);
        assert!(!result.routines_synced);
        assert!(result.changes.is_empty());
    }
}
