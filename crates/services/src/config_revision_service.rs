use async_trait::async_trait;
use models::{Agent, AgentConfigRevision};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ConfigRevisionError {
    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),

    #[error("Config revision not found: {0}")]
    RevisionNotFound(Uuid),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type ConfigRevisionResult<T> = Result<T, ConfigRevisionError>;

/// ConfigSnapshot - 配置快照结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigSnapshot {
    pub adapter_type: String,
    pub adapter_config: Value,
    pub runtime_config: Value,
    pub permissions: Value,
    pub budget_monthly_cents: i32,
}

impl ConfigSnapshot {
    /// 从Agent创建配置快照
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            adapter_type: agent.adapter_type.clone(),
            adapter_config: agent.adapter_config.0.clone(),
            runtime_config: agent.runtime_config.0.clone(),
            permissions: serde_json::to_value(&agent.permissions.0).unwrap_or_default(),
            budget_monthly_cents: agent.budget_monthly_cents,
        }
    }
}

/// ConfigRevisionService - 配置版本控制服务接口
#[async_trait]
pub trait ConfigRevisionService: Send + Sync {
    /// 捕获Agent当前配置快照
    async fn capture_snapshot(&self, agent_id: Uuid) -> ConfigRevisionResult<AgentConfigRevision>;

    /// 查询Agent的配置版本列表
    async fn list_revisions(
        &self,
        agent_id: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ConfigRevisionResult<Vec<AgentConfigRevision>>;

    /// 获取特定配置版本
    async fn get_revision(&self, revision_id: Uuid) -> ConfigRevisionResult<AgentConfigRevision>;

    /// 比较两个配置版本的差异
    async fn compare_revisions(
        &self,
        revision1_id: Uuid,
        revision2_id: Uuid,
    ) -> ConfigRevisionResult<ConfigDiff>;

    /// 统计Agent的版本总数
    async fn count_revisions(&self, agent_id: Uuid) -> ConfigRevisionResult<i64>;
}

/// ConfigDiff - 配置差异结构
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigDiff {
    pub revision1_id: Uuid,
    pub revision2_id: Uuid,
    pub changes: Vec<ConfigChange>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigChange {
    pub field: String,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
}
