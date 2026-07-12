//! CLI 认证挑战流程（对应任务拆解 §5 阶段二）。
//!
//! 提供：
//! - `create_cli_auth_challenge`：创建待批准挑战
//! - `approve_cli_auth_challenge`：批准并创建 Board API Key 返回
//! - `cancel_cli_auth_challenge`：取消挑战
//! - `get_cli_auth_challenge`：CLI 轮询挑战状态

use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use repositories::board_api_key_repository::{
    BoardApiKeyRepository, PgBoardApiKeyRepository, hash_api_key,
};
use repositories::cli_auth_challenge_repository::{
    CliAuthChallengeRepository, PgCliAuthChallengeRepository,
};
use repositories::models::auth_keys::CliAuthChallenge;

use crate::auth::{AuthError, AuthResult};

/// CLI 挑战默认有效期（分钟）。
const CLI_CHALLENGE_TTL_MINUTES: i64 = 15;

/// 创建 CLI 认证挑战。
///
/// 返回一个包含 `challenge_code` 的挑战对象，CLI 端展示该码供用户在 Board 端批准。
pub async fn create_cli_auth_challenge(
    pool: &PgPool,
    user_id: Uuid,
    company_id: Option<Uuid>,
    device_name: Option<String>,
    requested_access: serde_json::Value,
) -> AuthResult<CliAuthChallenge> {
    let challenge = CliAuthChallenge {
        id: Uuid::new_v4(),
        user_id,
        company_id,
        challenge_code: CliAuthChallenge::generate_challenge_code(),
        device_name,
        requested_access,
        status: "pending".to_string(),
        approved_at: None,
        approved_by_user_id: None,
        api_key_id: None,
        expires_at: Utc::now() + Duration::minutes(CLI_CHALLENGE_TTL_MINUTES),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let saved = repo.create(challenge).await.map_err(|e| AuthError::Internal {
        message: format!("Failed to create CLI auth challenge: {}", e),
    })?;

    Ok(saved)
}

/// 查询挑战状态（供 CLI 轮询）。
pub async fn get_cli_auth_challenge(
    pool: &PgPool,
    id: Uuid,
) -> AuthResult<Option<CliAuthChallenge>> {
    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let challenge = repo.find_by_id(id).await.map_err(|e| AuthError::Internal {
        message: format!("Failed to load CLI auth challenge: {}", e),
    })?;

    let mut challenge = match challenge {
        Some(c) => c,
        None => return Ok(None),
    };

    if challenge.is_pending() && challenge.is_expired() {
        // 已过期但未标记为 rejected，惰性拒绝
        challenge.reject();
        let _ = repo.cancel(challenge.id).await;
        return Ok(None);
    }

    Ok(Some(challenge))
}

/// 批准 CLI 认证挑战，创建 Board API Key 并返回明文 token（仅一次）。
pub async fn approve_cli_auth_challenge(
    pool: &PgPool,
    challenge_id: Uuid,
    approved_by_user_id: Uuid,
) -> AuthResult<(CliAuthChallenge, String)> {
    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let challenge = repo
        .find_by_id(challenge_id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to load CLI auth challenge: {}", e),
        })?
        .ok_or_else(|| AuthError::BadRequest {
            message: "CLI auth challenge not found".to_string(),
        })?;

    if !challenge.is_pending() {
        return Err(AuthError::BadRequest {
            message: format!("Challenge is already {}", challenge.status),
        });
    }
    if challenge.is_expired() {
        let _ = repo.cancel(challenge.id).await;
        return Err(AuthError::BadRequest {
            message: "Challenge has expired".to_string(),
        });
    }

    // 生成 Board API Key（明文仅返回一次）
    let (api_key, plaintext) = create_board_api_key_for_challenge(pool, challenge.user_id).await?;

    repo.approve(challenge.id, approved_by_user_id, api_key.id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to approve CLI auth challenge: {}", e),
        })?;

    let updated = repo
        .find_by_id(challenge.id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to reload CLI auth challenge: {}", e),
        })?
        .expect("challenge exists after approve");

    Ok((updated, plaintext))
}

/// 取消 CLI 认证挑战。
pub async fn cancel_cli_auth_challenge(pool: &PgPool, challenge_id: Uuid) -> AuthResult<()> {
    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let exists = repo
        .find_by_id(challenge_id)
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to load CLI auth challenge: {}", e),
        })?
        .is_some();

    if !exists {
        return Err(AuthError::BadRequest {
            message: "CLI auth challenge not found".to_string(),
        });
    }

    repo.cancel(challenge_id).await.map_err(|e| AuthError::Internal {
        message: format!("Failed to cancel CLI auth challenge: {}", e),
    })?;

    Ok(())
}

/// 为用户创建 Board API Key，返回 (记录, 明文 token)。
async fn create_board_api_key_for_challenge(
    pool: &PgPool,
    user_id: Uuid,
) -> AuthResult<(repositories::models::auth_keys::BoardApiKey, String)> {
    let key_repo = PgBoardApiKeyRepository::new(pool.clone());

    let token = generate_board_api_key_token();
    let key_hash = hash_api_key(&token);
    let key_prefix = token.get(0..12).unwrap_or("bak_").to_string();
    let expires_at = Utc::now() + Duration::days(365);

    let key = key_repo
        .create(
            user_id,
            "CLI Auth Challenge Key".to_string(),
            key_hash,
            key_prefix,
            Some(expires_at),
        )
        .await
        .map_err(|e| AuthError::Internal {
            message: format!("Failed to create Board API Key: {}", e),
        })?;

    Ok((key, token))
}

/// 生成 Board API Key 明文 token：`bak_` + 32 字节随机十六进制。
fn generate_board_api_key_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut raw = [0u8; 32];
    rng.fill(&mut raw);
    format!("bak_{}", hex::encode(raw))
}
