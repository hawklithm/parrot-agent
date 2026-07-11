use async_trait::async_trait;
use chrono::Utc;
use models::AgentConfigRevision;
use repositories::{AgentRepository, ConfigRevisionRepository, RepositoryError};
use serde_json::Value;
use sqlx::types::Json;
use std::sync::Arc;
use uuid::Uuid;

use super::config_revision_service::{
    ConfigChange, ConfigDiff, ConfigRevisionError, ConfigRevisionResult, ConfigRevisionService,
    ConfigSnapshot,
};

/// 标准实现的ConfigRevisionService
pub struct ConfigRevisionServiceImpl<A, C>
where
    A: AgentRepository,
    C: ConfigRevisionRepository,
{
    agent_repo: Arc<A>,
    config_repo: Arc<C>,
}

impl<A, C> ConfigRevisionServiceImpl<A, C>
where
    A: AgentRepository,
    C: ConfigRevisionRepository,
{
    pub fn new(agent_repo: Arc<A>, config_repo: Arc<C>) -> Self {
        Self {
            agent_repo,
            config_repo,
        }
    }

    /// 比较两个配置快照的差异
    fn compute_diff(snapshot1: &ConfigSnapshot, snapshot2: &ConfigSnapshot) -> Vec<ConfigChange> {
        let mut changes = Vec::new();

        // 比较adapter_type
        if snapshot1.adapter_type != snapshot2.adapter_type {
            changes.push(ConfigChange {
                field: "adapter_type".to_string(),
                old_value: Some(Value::String(snapshot1.adapter_type.clone())),
                new_value: Some(Value::String(snapshot2.adapter_type.clone())),
            });
        }

        // 比较adapter_config
        if snapshot1.adapter_config != snapshot2.adapter_config {
            changes.push(ConfigChange {
                field: "adapter_config".to_string(),
                old_value: Some(snapshot1.adapter_config.clone()),
                new_value: Some(snapshot2.adapter_config.clone()),
            });
        }

        // 比较runtime_config
        if snapshot1.runtime_config != snapshot2.runtime_config {
            changes.push(ConfigChange {
                field: "runtime_config".to_string(),
                old_value: Some(snapshot1.runtime_config.clone()),
                new_value: Some(snapshot2.runtime_config.clone()),
            });
        }

        // 比较permissions
        if snapshot1.permissions != snapshot2.permissions {
            changes.push(ConfigChange {
                field: "permissions".to_string(),
                old_value: Some(snapshot1.permissions.clone()),
                new_value: Some(snapshot2.permissions.clone()),
            });
        }

        // 比较budget
        if snapshot1.budget_monthly_cents != snapshot2.budget_monthly_cents {
            changes.push(ConfigChange {
                field: "budget_monthly_cents".to_string(),
                old_value: Some(Value::Number(snapshot1.budget_monthly_cents.into())),
                new_value: Some(Value::Number(snapshot2.budget_monthly_cents.into())),
            });
        }

        changes
    }
}

#[async_trait]
impl<A, C> ConfigRevisionService for ConfigRevisionServiceImpl<A, C>
where
    A: AgentRepository,
    C: ConfigRevisionRepository,
{
    async fn capture_snapshot(&self, agent_id: Uuid) -> ConfigRevisionResult<AgentConfigRevision> {
        // 查询Agent
        let agent = self
            .agent_repo
            .get_by_id(agent_id)
            .await
            .map_err(|e| match e {
                RepositoryError::NotFound(_) => ConfigRevisionError::AgentNotFound(agent_id),
                _ => ConfigRevisionError::RepositoryError(e.to_string()),
            })?;

        // 创建配置快照
        let snapshot = ConfigSnapshot::from_agent(&agent);
        let snapshot_json = serde_json::to_value(&snapshot)
            .map_err(|e| ConfigRevisionError::SerializationError(e.to_string()))?;

        // 保存到数据库
        let revision = AgentConfigRevision {
            id: Uuid::new_v4(),
            agent_id,
            snapshot: Json(snapshot_json),
            created_at: Utc::now(),
        };

        self.config_repo
            .create(revision)
            .await
            .map_err(|e| ConfigRevisionError::RepositoryError(e.to_string()))
    }

    async fn list_revisions(
        &self,
        agent_id: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> ConfigRevisionResult<Vec<AgentConfigRevision>> {
        self.config_repo
            .list_by_agent(agent_id, limit, offset)
            .await
            .map_err(|e| ConfigRevisionError::RepositoryError(e.to_string()))
    }

    async fn get_revision(&self, revision_id: Uuid) -> ConfigRevisionResult<AgentConfigRevision> {
        self.config_repo
            .get_by_id(revision_id)
            .await
            .map_err(|e| match e {
                RepositoryError::NotFound(_) => ConfigRevisionError::RevisionNotFound(revision_id),
                _ => ConfigRevisionError::RepositoryError(e.to_string()),
            })
    }

    async fn compare_revisions(
        &self,
        revision1_id: Uuid,
        revision2_id: Uuid,
    ) -> ConfigRevisionResult<ConfigDiff> {
        // 获取两个版本
        let revision1 = self.get_revision(revision1_id).await?;
        let revision2 = self.get_revision(revision2_id).await?;

        // 反序列化快照
        let snapshot1: ConfigSnapshot = serde_json::from_value(revision1.snapshot.0.clone())
            .map_err(|e| ConfigRevisionError::SerializationError(e.to_string()))?;
        let snapshot2: ConfigSnapshot = serde_json::from_value(revision2.snapshot.0.clone())
            .map_err(|e| ConfigRevisionError::SerializationError(e.to_string()))?;

        // 计算差异
        let changes = Self::compute_diff(&snapshot1, &snapshot2);

        Ok(ConfigDiff {
            revision1_id,
            revision2_id,
            changes,
        })
    }

    async fn count_revisions(&self, agent_id: Uuid) -> ConfigRevisionResult<i64> {
        self.config_repo
            .count_by_agent(agent_id)
            .await
            .map_err(|e| ConfigRevisionError::RepositoryError(e.to_string()))
    }

    async fn rollback_to_revision(
        &self,
        agent_id: Uuid,
        revision_id: Uuid,
    ) -> ConfigRevisionResult<AgentConfigRevision> {
        // 获取目标版本
        let revision = self.get_revision(revision_id).await?;

        // 验证版本属于该Agent
        if revision.agent_id != agent_id {
            return Err(ConfigRevisionError::RepositoryError(
                "Revision does not belong to this agent".to_string(),
            ));
        }

        // 获取当前Agent
        let mut agent = self
            .agent_repo
            .get_by_id(agent_id)
            .await
            .map_err(|e| match e {
                RepositoryError::NotFound(_) => ConfigRevisionError::AgentNotFound(agent_id),
                _ => ConfigRevisionError::RepositoryError(e.to_string()),
            })?;

        // 反序列化快照
        let snapshot: ConfigSnapshot = serde_json::from_value(revision.snapshot.0.clone())
            .map_err(|e| ConfigRevisionError::SerializationError(e.to_string()))?;

        // 应用配置快照到Agent
        agent.adapter_type = snapshot.adapter_type;
        agent.adapter_config = Json(snapshot.adapter_config);
        agent.runtime_config = Json(snapshot.runtime_config);
        agent.permissions = Json(
            serde_json::from_value(snapshot.permissions)
                .unwrap_or_else(|_| models::AgentPermissions::default()),
        );
        agent.budget_monthly_cents = snapshot.budget_monthly_cents;

        // 保存Agent更新
        self.agent_repo
            .update(agent)
            .await
            .map_err(|e| ConfigRevisionError::RepositoryError(e.to_string()))?;

        // 创建新的快照记录回滚操作
        self.capture_snapshot(agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff() {
        let snapshot1 = ConfigSnapshot {
            adapter_type: "claude_local".to_string(),
            adapter_config: serde_json::json!({"model": "claude-opus-4"}),
            runtime_config: serde_json::json!({}),
            permissions: serde_json::json!({}),
            budget_monthly_cents: 10000,
        };

        let snapshot2 = ConfigSnapshot {
            adapter_type: "claude_local".to_string(),
            adapter_config: serde_json::json!({"model": "claude-sonnet-4"}),
            runtime_config: serde_json::json!({}),
            permissions: serde_json::json!({}),
            budget_monthly_cents: 20000,
        };

        let changes = ConfigRevisionServiceImpl::<
            repositories::PgAgentRepository,
            repositories::PgConfigRevisionRepository,
        >::compute_diff(&snapshot1, &snapshot2);

        assert_eq!(changes.len(), 2); // adapter_config + budget
        assert!(changes.iter().any(|c| c.field == "adapter_config"));
        assert!(changes.iter().any(|c| c.field == "budget_monthly_cents"));
    }
}
