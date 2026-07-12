use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::auth_keys::CliAuthChallenge;

/// CLI 认证挑战 Repository trait
#[async_trait]
pub trait CliAuthChallengeRepository: Send + Sync {
    /// 创建挑战记录
    async fn create(&self, challenge: CliAuthChallenge) -> Result<CliAuthChallenge, crate::RepositoryError>;

    /// 按 ID 查询
    async fn find_by_id(&self, id: Uuid) -> Result<Option<CliAuthChallenge>, crate::RepositoryError>;

    /// 按挑战码（token）查询
    async fn find_by_challenge_code(
        &self,
        challenge_code: &str,
    ) -> Result<Option<CliAuthChallenge>, crate::RepositoryError>;

    /// 列出某用户的待处理挑战
    async fn list_pending_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<CliAuthChallenge>, crate::RepositoryError>;

    /// 批准挑战：设置状态、批准人、关联 API Key
    async fn approve(
        &self,
        id: Uuid,
        approved_by_user_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<(), crate::RepositoryError>;

    /// 取消挑战
    async fn cancel(&self, id: Uuid) -> Result<(), crate::RepositoryError>;
}

/// PostgreSQL 实现
pub struct PgCliAuthChallengeRepository {
    pool: PgPool,
}

impl PgCliAuthChallengeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CliAuthChallengeRepository for PgCliAuthChallengeRepository {
    async fn create(&self, challenge: CliAuthChallenge) -> Result<CliAuthChallenge, crate::RepositoryError> {
        sqlx::query(
            r#"INSERT INTO cli_auth_challenges
               (id, user_id, company_id, challenge_code, device_name, requested_access,
                status, approved_at, approved_by_user_id, api_key_id, expires_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        )
        .bind(challenge.id)
        .bind(challenge.user_id)
        .bind(challenge.company_id)
        .bind(&challenge.challenge_code)
        .bind(&challenge.device_name)
        .bind(&challenge.requested_access)
        .bind(&challenge.status)
        .bind(challenge.approved_at)
        .bind(challenge.approved_by_user_id)
        .bind(challenge.api_key_id)
        .bind(challenge.expires_at)
        .bind(challenge.created_at)
        .bind(challenge.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(challenge)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<CliAuthChallenge>, crate::RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(
            "SELECT * FROM cli_auth_challenges WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn find_by_challenge_code(
        &self,
        challenge_code: &str,
    ) -> Result<Option<CliAuthChallenge>, crate::RepositoryError> {
        let row = sqlx::query_as::<_, CliAuthChallenge>(
            "SELECT * FROM cli_auth_challenges WHERE challenge_code = $1",
        )
        .bind(challenge_code)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn list_pending_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<CliAuthChallenge>, crate::RepositoryError> {
        let rows = sqlx::query_as::<_, CliAuthChallenge>(
            "SELECT * FROM cli_auth_challenges WHERE user_id = $1 AND status = 'pending' ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn approve(
        &self,
        id: Uuid,
        approved_by_user_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<(), crate::RepositoryError> {
        sqlx::query(
            r#"UPDATE cli_auth_challenges
               SET status = 'approved', approved_at = NOW(), approved_by_user_id = $2,
                   api_key_id = $3, updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(approved_by_user_id)
        .bind(api_key_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn cancel(&self, id: Uuid) -> Result<(), crate::RepositoryError> {
        sqlx::query(
            "UPDATE cli_auth_challenges SET status = 'cancelled', updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
