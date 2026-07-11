use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::auth_keys::AgentApiKey;
use super::board_api_key_repository::{RepositoryError, RepositoryResult};

/// AgentApiKey Repository trait（扩展现有trait）
#[async_trait]
pub trait AgentApiKeyRepositoryExt: Send + Sync {
    /// 通过密钥哈希查找API Key
    async fn find_by_key_hash(&self, key_hash: &str) -> RepositoryResult<Option<AgentApiKey>>;

    /// 创建新的Agent API Key
    async fn create(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        scope: sqlx::types::JsonValue,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepositoryResult<AgentApiKey>;

    /// 撤销API Key
    async fn revoke(&self, key_id: Uuid) -> RepositoryResult<()>;

    /// 记录API Key使用
    async fn record_usage(&self, key_id: Uuid) -> RepositoryResult<()>;

    /// 列出Agent的所有API Keys
    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<AgentApiKey>>;
}

/// PostgreSQL实现
pub struct PgAgentApiKeyRepositoryExt {
    pool: PgPool,
}

impl PgAgentApiKeyRepositoryExt {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentApiKeyRepositoryExt for PgAgentApiKeyRepositoryExt {
    async fn find_by_key_hash(&self, key_hash: &str) -> RepositoryResult<Option<AgentApiKey>> {
        let row = sqlx::query_as::<_, AgentApiKey>(
            r#"
            SELECT id, agent_id, company_id, name, key_hash, key_prefix, scope,
                   last_used_at, expires_at, is_revoked, revoked_at, created_at, updated_at
            FROM agent_api_keys
            WHERE key_hash = $1 AND is_revoked = false
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn create(
        &self,
        agent_id: Uuid,
        company_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        scope: sqlx::types::JsonValue,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepositoryResult<AgentApiKey> {
        let key = AgentApiKey::new(
            agent_id,
            company_id,
            name,
            key_hash,
            key_prefix,
            scope,
            expires_at,
        );

        sqlx::query(
            r#"
            INSERT INTO ageeys (
                id, agent_id, company_id, name, key_hash, key_prefix, scope,
                last_used_at, expires_at, is_revoked, revoked_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(key.id)
        .bind(key.agent_id)
        .bind(key.company_id)
        .bind(&key.name)
        .bind(&key.key_hash)
        .bind(&key.key_prefix)
        .bind(&key.scope)
        .bind(key.last_used_at)
        .bind(key.expires_at)
        .bind(key.is_revoked)
        .bind(key.revoked_at)
        .bind(key.created_at)
        .bind(key.updated_a     .execute(&self.pool)
        .await?;

        Ok(key)
    }

    async fn revoke(&self, key_id: Uuid) -> RepositoryResult<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE agent_api_keys
            SET is_revoked = true, revoked_at = $1, updated_at = $2
            WHERE id = $3 AND is_revoked = false
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(key_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "AgentApiKey {} not found or already revoked",
                key_id
            )));
        }

        Ok(())
    }

    async fn record_usage(&self, key_id: Uuid) -> RepositoryResult<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE agent_api_keys
            SET last_used_at = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(key_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<AgentApiKey>> {
        let rows = sqlx::query_as::<_, AgentApiKey>(
            r#"
            SELECT id, agent_id, company_id, name, key_hash, key_prefix, scope,
                   last_used_at, expires_at, is_revoked, revoked_at, created_at, updated_at
            FROM agent_api_keys
            WHERE agent_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 注意：集成测试需要PostgreSQL数据库连接
    // 这里提供单元测试框架，实际测试需要使用sqlx::test宏
}
