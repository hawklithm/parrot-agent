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

    /// 列出用户名下的 API Keys；`include_inactive` 为真时连同已撤销/已过期的一并返回。
    async fn list_by_user(
        &self,
        user_id: Uuid,
        include_inactive: bool,
    ) -> Result<Vec<BoardApiKey>, RepositoryError>;
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
              AND (expires_at IS NULL OR expires_at > NOW())
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

    /// 撤销 API Key：仅允许撤销 `revoked_by_user_id` 自己名下的 Key。
    ///
    /// 归属校验内联在 `WHERE` 里（Paperclip 先在 `getBoardApiKeyForUser`
    /// 校验归属再撤销），因此他人的 Key 与不存在的 Key 一样返回 `NotFound`。
    async fn revoke(&self, key_id: Uuid, revoked_by_user_id: Uuid) -> Result<(), RepositoryError> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE board_api_keys
            SET is_revoked = true, revoked_at = $1, last_used_at = $1,
                revoked_by_user_id = $2, updated_at = $3
            WHERE id = $4 AND user_id = $2 AND is_revoked = false
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

    async fn list_by_user(
        &self,
        user_id: Uuid,
        include_inactive: bool,
    ) -> Result<Vec<BoardApiKey>, RepositoryError> {
        // 默认只返回「未撤销且未过期」的 Key；`include_inactive` 时返回全部
        // （对齐 Paperclip `listBoardApiKeys` 的 `includeInactive` 语义）。
        let rows = sqlx::query_as::<_, BoardApiKey>(
            r#"
            SELECT id, user_id, name, key_hash, key_prefix, last_used_at, expires_at,
                   is_revoked, revoked_at, revoked_by_user_id, created_at, updated_at
            FROM board_api_keys
            WHERE user_id = $1
              AND ($2 OR (is_revoked = false AND (expires_at IS NULL OR expires_at > NOW())))
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .bind(include_inactive)
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

/// Board API token 的展示前缀长度（`key_prefix` 列是 Parrot 独有的展示字段）。
pub const KEY_PREFIX_DISPLAY_LEN: usize = 16;

/// 生成 Board API token：`pcp_board_<48 hex>`。
///
/// 对齐 Paperclip `createBoardApiToken`（`pcp_board_` + 24 随机字节的十六进制），
/// 与 CLI 挑战 secret 的 `pcp_cli_auth_` 前缀区分。
pub fn create_board_api_token() -> String {
    use rand::Rng;
    let mut raw = [0u8; 24];
    rand::thread_rng().fill(&mut raw);
    format!("pcp_board_{}", hex::encode(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_api_key() {
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

    #[test]
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

        // 多次验证应消耗相同时间（防止时序攻击）。
        // 核心风险是错误 token 因提前短路（如首字符比较）而显著变快。
        // 采用单边断言（wrong >= 60% correct）+ 宽松上界（correct <= 3x wrong），
        // 50k 迭代摊平调度噪声，负载下也不易抖动；仍可捕获数量级的时间泄漏。
        let iterations = 50_000;
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            verify_api_key(token, &hash);
        }
        let duration_correct = start.elapsed();

        let start = std::time::Instant::now();
        for _ in 0..iterations {
            verify_api_key("wrong_token_with_same_length", &hash);
        }
        let duration_wrong = start.elapsed();

        let correct_us = duration_correct.as_micros() as f64;
        let wrong_us = duration_wrong.as_micros() as f64;
        assert!(
            wrong_us >= correct_us * 0.6,
            "Wrong-token verification too fast (timing leak?): correct={}us wrong={}us",
            correct_us,
            wrong_us
        );
        assert!(
            correct_us <= wrong_us * 3.0,
            "Correct-token verification unexpectedly slow: correct={}us wrong={}us",
            correct_us,
            wrong_us
        );
    }

    /// Board token 必须与 Paperclip `createBoardApiToken` 同形：
    /// `pcp_board_` + 48 位十六进制，且每次都不同。
    #[test]
    fn test_create_board_api_token_shape() {
        let token = create_board_api_token();
        assert!(token.starts_with("pcp_board_"), "unexpected prefix: {token}");

        let random_part = token.trim_start_matches("pcp_board_");
        assert_eq!(random_part.len(), 48, "24 random bytes as hex");
        assert!(
            random_part.chars().all(|c| c.is_ascii_hexdigit()),
            "random part must be hex: {random_part}"
        );
        assert_ne!(token, create_board_api_token());
    }
}
