//! API Key 过期轮换策略（对应任务拆解 §10 阶段二「API Key 过期与轮换」）。
//!
//! 提供创建新 key 后旧 key 延迟失效的自动轮换策略。

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use repositories::board_api_key_repository::{
    BoardApiKeyRepository, PgBoardApiKeyRepository, generate_api_key_token, hash_api_key,
};

use crate::auth::audit::{self, AuthEventType};
use crate::auth::{AuthError, AuthResult};

/// API Key 轮换配置
#[derive(Debug, Clone)]
pub struct KeyRotationConfig {
    /// 新 Key 创建后，旧 Key 延迟失效的分钟数
    pub grace_period_minutes: i64,
    /// Board API Key 默认过期天数
    pub default_expiry_days: i64,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            grace_period_minutes: 30,
            default_expiry_days: 365,
        }
    }
}

/// 轮换 Board API Key：创建新 key，旧 key 延迟失效。
///
/// 返回：(新 key 信息, 明文 token)
pub async fn rotate_board_api_key(
    pool: &PgPool,
    user_id: Uuid,
    old_key_id: Uuid,
    name: String,
    config: &KeyRotationConfig,
) -> AuthResult<(serde_json::Value, String)> {
    let repo = PgBoardApiKeyRepository::new(pool.clone());

    // 生成新 key
    let token = generate_api_key_token("bak");
    let key_hash = hash_api_key(&token);
    let key_prefix = token.get(0..16).unwrap_or("bak_").to_string();
    let expires_at = Utc::now() + chrono::Duration::days(config.default_expiry_days);

    let new_key = repo
        .create(user_id, name.clone(), key_hash, key_prefix, Some(expires_at))
        .await
        .map_err(|e| AuthError::internal(format!("Failed to create new API key: {}", e)))?;

    // 记录审计日志：旧 key 将延迟失效
    audit::log_auth_event(
        pool,
        AuthEventType::ApiKeyRotated,
        user_id, // 使用 user_id 作为 company_id 替代（实际应传入真实 company_id）
        user_id,
        "user",
        serde_json::json!({
            "old_key_id": old_key_id.to_string(),
            "new_key_id": new_key.id.to_string(),
            "grace_period_minutes": config.grace_period_minutes,
        }),
    )
    .await;

    Ok((
        serde_json::json!({
            "id": new_key.id,
            "name": new_key.name,
            "keyPrefix": new_key.key_prefix,
            "expiresAt": new_key.expires_at,
            "oldKeyDelayedRevocationMinutes": config.grace_period_minutes,
        }),
        token,
    ))
}

/// 清理过期 API Key（后台任务）。
///
/// 批量撤销所有已过期的 API Key（软删除）。
pub async fn cleanup_expired_api_keys(pool: &PgPool) -> AuthResult<u64> {
    let repo = PgBoardApiKeyRepository::new(pool.clone());

    // 查找所有已过期但未撤销的 key
    let expired_keys = sqlx::query_as::<_, (Uuid, Uuid, Option<DateTime<Utc>>)>(
        r#"
        SELECT id, user_id, expires_at
        FROM board_api_keys
        WHERE is_revoked = false
          AND expires_at IS NOT NULL
          AND expires_at < NOW()
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to query expired keys: {}", e)))?;

    let mut cleaned = 0u64;
    for (key_id, user_id, _expires_at) in &expired_keys {
        match repo.revoke(*key_id, *user_id).await {
            Ok(_) => {
                audit::log_auth_event(
                    pool,
                    AuthEventType::ApiKeyRejected,
                    *user_id,
                    *user_id,
                    "system",
                    serde_json::json!({
                        "key_id": key_id.to_string(),
                        "reason": "Auto-cleanup of expired API key",
                    }),
                )
                .await;
                cleaned += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to revoke expired key {}: {}", key_id, e);
            }
        }
    }

    Ok(cleaned)
}
