use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::models::auth_keys::BoardApiKey;

/// BoardApiKey Repository trait
#[async_trait]
pub trait BoardApiKeyRepository: Send + Sync {
    /// 通过密钥哈希查找API Key
    async fn find_by_key_hash(&self, key_hash: &str) -> Result<Option<BoardApiKey>, RepositoryError>;

    /// 创建新的API Key
    async fn create(
        &self,
        user_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<BoardApiKey, RepositoryError>;

    /// 撤销API Key
    async fn revoke(&self, key_id: Uuid, revoked_by_user_id: Uuid) -> Result<(), RepositoryError>;

    /// 记录API Key使用
    async fn record_usage(&self, key_id: Uuid) -> Result<(), RepositoryError>;

    /// 列出用户的所有API Keys
    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<BoardApiKey>, RepositoryError>;
}

/// Repository错误类型
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),
}

pub type RepositoryResult<T> = Result<T, RepositoryError>;

/// PostgreSQL实现
pub struct PgBoardApiKeyRepository {
    pool: PgPool,
}

impl PgBoardApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BoardApiKeyRepository for PgBoardApiKeyRepository {
    async fn find_by_key_hash(&self, key_hash: &str) -> Result<Option<BoardApiKey>, RepositoryError> {
        let row = sqlx::query_as::<_, BoardApiKey>(
            r#"
            SELECT id, user_id, name, key_hash, key_prefix, last_used_at, expires_at,
                   is_revoked, revoked_at, revoked_by_user_id, created_at, updated_at
            FROM board_api_keys
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
        user_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<BoardApiKey, RepositoryError> {
        let key = BoardApiKey::new(user_id, name, key_hash, key_prefix, expires_at);

        sqlx::query(
            r#"
            INSERT INTO board_api_keys (
                id, user_id, name, key_hash, key_prefix, last_used_at, expires_at,
                is_revoked, revoked_at, revoked_by_user_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(key.id)
        .bind(key.user_id)
        .bind(&key.name)
        .bind(&key.key_hash)
        .bind(&key.key_prefix)
        .bind(key.last_used_at)
        .bind(key.expires_at)
        .bind(key.is_revoked)   .bind(key.revoked_at)
        .bind(key.revoked_by_user_id)
        .bind(key.created_at)
        .bind(key.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(key)
    }

    async fn revoke(&self, key_id: Uuid, revoked_by_user_id: Uuid) -> Result<(), RepositoryError> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE board_api_keys
            SET is_revoked = true, revoked_at = $1, revoked_by_user_id = $2, updated_at = $3
            WHERE id = $4 AND is_revoked = false
            "#,
        )
        .bind(now)
        .bind(revoked_by_user_id)
        .bind(now)
        .bind(key_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound(format!(
                "BoardApiKey {} not found or already revoked",
                key_id
            )));
        }

        Ok(())
    }

    async fn record_usage(&self, key_id: Uuid) -> Result<(), RepositoryError> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE board_api_keys
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

    async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<BoardApiKey>, RepositoryError> {
        let rows = sqlx::query_as::<_, BoardApiKey>(
            r#"
            SELECT id, user_id, name, key_hash, key_prefix, last_used_at, expires_at,
                   is_revoked, revoked_at, revoked_by_user_id, created_at, updated_at
            FROM board_api_keys
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

/// API Key哈希函数（SHA-256）
pub fn hash_api_key(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// API Key验证函数（constant-time比较）
pub fn verify_api_key(token: &str, hash: &str) -> bool {
    let computed_hash = hash_api_key(token);
    computed_hash.as_bytes().ct_eq(hash.as_bytes()).into()
}

/// 生成API Key token（安全随机）
pub fn generate_api_key_token(prefix: &str) -> String {
    use rand::Rng;
    let random_bytes: [u8; 32] = rand::thread_rng().gen();
    let random_part = hex::encode(random_bytes);
    format!("{}_{}", prefix, random_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_api) {
        let token = "test_token_123";
        let hash1 = hash_api_key(token);
        let hash2 = hash_api_key(token);

        // 相同输入应产生相同哈希
        assert_eq!(hash1, hash2);

        // 不同输入应产生不同哈希
        let hash3 = hash_api_key("different_token");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_verify_api_key() {
        let token = "test_token_456";
        let hash = hash_api_key(token);

        // 正确的token应验证通过
        assert!(verify_api_key(token, &hash));

        // 错误的token应验证失败
        assert!(!verify_api_key("wrong_token", &hash));
    }

    t]
    fn test_generate_api_key_token() {
        let token1 = generate_api_key_token("pk");
        let token2 = generate_api_key_token("pk");

        // 每次生成应不同
        assert_ne!(token1, token2);

        // 应包含前缀
        assert!(token1.starts_with("pk_"));
        assert!(token2.starts_with("pk_"));
    }

    #[test]
    fn test_constant_time_comparison() {
        let token = "secret_token";
        let hash = hash_api_key(token);

        // 多次验证应消耗相同时间（防止时序攻击）
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            verify_api_key(token, &hash);
        }
        let duration_correct = start.elapsed();

        let start = std::time::Instant::now();
        for _ in 0..1000 {
            verify_api_key("wrong_token_with_same_length", &hash);
        }
        let duration_wrong = start.elapsed();

        // 时间差应在合理范围内（允许10%误差）
        let ratio = duration_correct.as_micros() as f64 / duration_wrong.as_micros() as f64;
        assert!(ratio > 0.9 && ratio < 1.1, "Timing difference too large: {}", ratio);
    }
}
