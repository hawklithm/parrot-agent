use async_trait::async_trait;
use models::AgentConfigRevision;
use uuid::Uuid;

use crate::RepositoryResult;

/// ConfigRevisionRepository - 配置版本记录仓储接口
#[async_trait]
pub trait ConfigRevisionRepository: Send + Sync {
    /// 创建配置版本快照
    async fn create(&self, revision: AgentConfigRevision) -> RepositoryResult<AgentConfigRevision>;

    /// 按Agent ID查询配置版本列表（按创建时间降序）
    async fn list_by_agent(
        &self,
        agent_id: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> RepositoryResult<Vec<AgentConfigRevision>>;

    /// 按ID查询单个配置版本
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<AgentConfigRevision>;

    /// 统计Agent的配置版本总数
    async fn count_by_agent(&self, agent_id: Uuid) -> RepositoryResult<i64>;
}
