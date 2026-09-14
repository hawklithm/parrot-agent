use models::AgentApiKey;
use sqlx::PgPool;
use uuid::Uuid;
use crate::RepositoryError as RepoError;

/// Agent API Key Repository trait
#[async_trait::async_trait]
pub trait AgentApiKeyRepository: Send + Sync {
    /// 通过密钥哈希查找 API Key
    async fn find_by_key_hash(&self, key_hash: &str) -> Result<Option<AgentApiKey>, RepoError>;

    /// 创建新的 API Key
    async fn create(&self, api_key: AgentApiKey) -> Result<AgentApiKey, RepoError>;

    /// 按 ID 加载 API Key（含已撤销的）。
    ///
    /// DELETE 路由需要先读出来判断 `key.agent_id == agent.id`
    /// （Paperclip `routes/agents.ts:4197-4205`），再决定 404 还是撤销，
    /// 因此这里不过滤 `revoked_at`。
    async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentApiKey>, RepoError>;

    /// 撤销指定 Agent 名下的 API Key。
    ///
    /// 返回 `None` 表示该 key 不存在**或不属于该 agent**——Paperclip
    /// `revokeKey` 的 `where(and(eq(id), eq(agentId)))` 语义
    /// （`services/agents.ts:1140-1147`），调用方据此回 404。
    async fn revoke(&self, agent_id: Uuid, id: Uuid) -> Result<Option<AgentApiKey>, RepoError>;

    /// 更新最后使用时间
    async fn update_last_used(&self, id: Uuid) -> Result<(), RepoError>;

    /// 列出 Agent 的所有 API Keys
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentApiKey>, RepoError>;

    /// 撤销某个 Agent 的全部 API Key（用于 Agent 终止/删除时使其无法再鉴权）
    async fn revoke_by_agent(&self, agent_id: Uuid) -> Result<u64, RepoError>;
}

/// PostgreSQL 实现
#[derive(Clone)]
pub struct PgAgentApiKeyRepository {
    pool: PgPool,
}

impl PgAgentApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AgentApiKeyRepository for PgAgentApiKeyRepository {
    async fn find_by_key_hash(&self, key_hash: &str) -> Result<Option<AgentApiKey>, RepoError> {
        let result = sqlx::query_as::<_, AgentApiKey>(
            r#"
            SELECT id, agent_id, company_id, name, key_hash, scope, responsible_user_id,
                   last_used_at, revoked_at, created_at
            FROM agent_api_keys
            WHERE key_hash = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn create(&self, api_key: AgentApiKey) -> Result<AgentApiKey, RepoError> {
        let result = sqlx::query_as::<_, AgentApiKey>(
            r#"
            INSERT INTO agent_api_keys
                (id, agent_id, company_id, name, key_hash, scope, responsible_user_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, agent_id, company_id, name, key_hash, scope, responsible_user_id,
                      last_used_at, revoked_at, created_at
            "#,
        )
        .bind(api_key.id)
        .bind(api_key.agent_id)
        .bind(api_key.company_id)
        .bind(api_key.name)
        .bind(api_key.key_hash)
        .bind(&api_key.scope)
        .bind(api_key.responsible_user_id)
        .bind(api_key.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<AgentApiKey>, RepoError> {
        let result = sqlx::query_as::<_, AgentApiKey>(
            r#"
            SELECT id, agent_id, company_id, name, key_hash, scope, responsible_user_id,
                   last_used_at, revoked_at, created_at
            FROM agent_api_keys
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn revoke(&self, agent_id: Uuid, id: Uuid) -> Result<Option<AgentApiKey>, RepoError> {
        let result = sqlx::query_as::<_, AgentApiKey>(
            r#"
            UPDATE agent_api_keys
            SET revoked_at = NOW()
            WHERE id = $1 AND agent_id = $2
            RETURNING id, agent_id, company_id, name, key_hash, scope, responsible_user_id,
                      last_used_at, revoked_at, created_at
            "#,
        )
        .bind(id)
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result)
    }

    async fn update_last_used(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query(
            r#"
            UPDATE agent_api_keys
            SET last_used_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<AgentApiKey>, RepoError> {
        let results = sqlx::query_as::<_, AgentApiKey>(
            r#"
            SELECT id, agent_id, company_id, name, key_hash, scope, responsible_user_id,
                   last_used_at, revoked_at, created_at
            FROM agent_api_keys
            WHERE agent_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(results)
    }

    async fn revoke_by_agent(&self, agent_id: Uuid) -> Result<u64, RepoError> {
        let result = sqlx::query(
            r#"
            UPDATE agent_api_keys
            SET revoked_at = NOW()
            WHERE agent_id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(RepoError::DatabaseError)?;

        Ok(result.rows_affected())
    }
}
