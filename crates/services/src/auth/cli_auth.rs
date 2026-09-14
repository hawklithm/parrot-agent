//! CLI 认证挑战流程（对齐 Paperclip `board-auth.ts` 的设备授权协议）。
//!
//! 流程：
//! - `create_cli_auth_challenge`：生成挑战 secret 与待生效 Board token，明文各返回一次；
//! - `describe_cli_auth_challenge`：CLI 携带 token 轮询挑战状态；
//! - `approve_cli_auth_challenge`：Board 用户批准，用创建时保存的哈希落库 Board API Key；
//! - `cancel_cli_auth_challenge`：取消挑战。

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use repositories::board_api_key_repository::{hash_api_key, verify_api_key};
use repositories::cli_auth_challenge_repository::PgCliAuthChallengeRepository;
use repositories::models::auth_keys::{BoardApiKey, CliAuthChallenge};

use crate::auth::{AuthError, AuthResult};

/// CLI 挑战有效期：10 分钟（Paperclip `CLI_AUTH_CHALLENGE_TTL_MS`）。
pub const CLI_AUTH_CHALLENGE_TTL_SECONDS: i64 = 600;

/// 批准后签发的 Board API Key 有效期：30 天（Paperclip `BOARD_API_KEY_TTL_MS`）。
pub const BOARD_API_KEY_TTL_DAYS: i64 = 30;

/// 「需要实例管理员」的访问级别取值。
const ACCESS_INSTANCE_ADMIN_REQUIRED: &str = "instance_admin_required";

/// 未提供客户端名称时的 Key 名称字面量（对齐 Paperclip `paperclipai cli`）。
pub const DEFAULT_KEY_LABEL: &str = "paperclipai cli";

/// `key_prefix` 展示列取哈希的前 12 个字符。
const KEY_PREFIX_LEN: usize = 12;

/// 挑战不存在或 token 校验失败时统一对外的 404 文案（对齐 Paperclip）。
const CHALLENGE_NOT_FOUND: &str = "CLI auth challenge not found";

/// 创建挑战的结果；两个 token 的明文仅在此处返回一次。
pub struct CliChallengeCreated {
    pub id: Uuid,
    /// 挑战 secret（`pcp_cli_auth_` 前缀）。
    pub token: String,
    /// 批准后生效的 Board API token（`pcp_board_` 前缀）。
    pub board_api_token: String,
    pub expires_at: DateTime<Utc>,
}

/// 批准人信息。
pub struct CliApprovedByUser {
    pub id: Uuid,
    pub name: Option<String>,
    pub email: Option<String>,
}

/// 挑战详情视图。
pub struct CliChallengeView {
    pub id: Uuid,
    /// `"pending" | "approved" | "cancelled" | "expired"`。
    pub status: String,
    pub command: String,
    pub client_name: Option<String>,
    /// `"board" | "instance_admin_required"`。
    pub requested_access: String,
    pub requested_company_id: Option<Uuid>,
    pub requested_company_name: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub approved_by_user: Option<CliApprovedByUser>,
}

/// 批准结果。
pub struct CliApprovalOutcome {
    /// `status == "approved"`。
    pub approved: bool,
    pub status: String,
    /// 批准人。
    pub user_id: Uuid,
    /// `challenge.board_api_key_id`。
    pub key_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

/// 取消结果。
pub struct CliCancelOutcome {
    pub status: String,
    pub cancelled: bool,
}

/// 生成 24 字节随机数的十六进制表示（48 个小写十六进制字符）。
fn random_hex_24() -> String {
    let mut raw = [0u8; 24];
    rand::thread_rng().fill(&mut raw);
    hex::encode(raw)
}

/// 挑战 secret：`pcp_cli_auth_` + 48 位十六进制（对齐 Paperclip `createCliAuthSecret`）。
fn create_cli_auth_secret() -> String {
    format!("pcp_cli_auth_{}", random_hex_24())
}

/// 待生效的 Board token：`pcp_board_` + 48 位十六进制（对齐 Paperclip `createBoardApiToken`）。
fn create_board_api_token() -> String {
    format!("pcp_board_{}", random_hex_24())
}

/// 按 ID 加载挑战并做 constant-time secret 校验。
///
/// 存在性失败与哈希不匹配都归为 `NotFound`，避免按「挑战是否存在」区分响应。
async fn load_by_secret(
    repo: &PgCliAuthChallengeRepository,
    id: Uuid,
    token: &str,
) -> AuthResult<CliAuthChallenge> {
    let challenge = repo
        .find_by_id(id)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load CLI auth challenge: {e}")))?
        .ok_or_else(|| AuthError::not_found(CHALLENGE_NOT_FOUND))?;

    // `verify_api_key` 内部先做 SHA-256 再 constant-time 比较（subtle::ConstantTimeEq），
    // 与 Paperclip `tokenHashesMatch` 的语义一致。
    if !verify_api_key(token, &challenge.secret_hash) {
        return Err(AuthError::not_found(CHALLENGE_NOT_FOUND));
    }

    Ok(challenge)
}

/// 创建 CLI 认证挑战。
///
/// 返回挑战 secret 与待生效 Board token 的明文，两者仅此一次返回；服务端只落库其哈希。
pub async fn create_cli_auth_challenge(
    pool: &PgPool,
    command: String,
    client_name: Option<String>,
    requested_access: String,
    requested_company_id: Option<Uuid>,
) -> AuthResult<CliChallengeCreated> {
    let client_name = client_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let label_base = client_name
        .clone()
        .unwrap_or_else(|| DEFAULT_KEY_LABEL.to_string());
    let pending_key_name = if requested_access == ACCESS_INSTANCE_ADMIN_REQUIRED {
        format!("{label_base} (instance admin)")
    } else {
        format!("{label_base} (board)")
    };

    let challenge_secret = create_cli_auth_secret();
    let pending_board_token = create_board_api_token();
    let now = Utc::now();
    let expires_at = now + Duration::seconds(CLI_AUTH_CHALLENGE_TTL_SECONDS);

    let challenge = CliAuthChallenge {
        id: Uuid::new_v4(),
        secret_hash: hash_api_key(&challenge_secret),
        command: command.trim().to_string(),
        client_name,
        requested_access,
        requested_company_id,
        pending_key_hash: hash_api_key(&pending_board_token),
        pending_key_name,
        approved_by_user_id: None,
        board_api_key_id: None,
        approved_at: None,
        cancelled_at: None,
        expires_at,
        created_at: now,
        updated_at: now,
    };

    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    repo.create(&challenge)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to create CLI auth challenge: {e}")))?;

    Ok(CliChallengeCreated {
        id: challenge.id,
        token: challenge_secret,
        board_api_token: pending_board_token,
        expires_at,
    })
}

/// 查询挑战详情；`id` 不存在或 `token` 与 `secret_hash` 不匹配时返回 `NotFound`。
pub async fn describe_cli_auth_challenge(
    pool: &PgPool,
    id: Uuid,
    token: &str,
) -> AuthResult<CliChallengeView> {
    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let challenge = load_by_secret(&repo, id, token).await?;

    let requested_company_name = match challenge.requested_company_id {
        Some(company_id) => {
            sqlx::query_scalar::<_, String>("SELECT name FROM companies WHERE id = $1")
                .bind(company_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| {
                    AuthError::internal(format!("Failed to load requested company: {e}"))
                })?
        }
        None => None,
    };

    let approved_by_user = match challenge.approved_by_user_id {
        Some(user_id) => {
            let row = sqlx::query_as::<_, (Uuid, Option<String>, String)>(
                "SELECT id, name, email FROM auth_users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to load approving user: {e}")))?;

            row.map(|(id, name, email)| CliApprovedByUser {
                id,
                name,
                email: Some(email),
            })
        }
        None => None,
    };

    let status = challenge.status().to_string();

    Ok(CliChallengeView {
        id: challenge.id,
        status,
        command: challenge.command,
        client_name: challenge.client_name,
        requested_access: challenge.requested_access,
        requested_company_id: challenge.requested_company_id,
        requested_company_name,
        approved_at: challenge.approved_at,
        cancelled_at: challenge.cancelled_at,
        expires_at: challenge.expires_at,
        approved_by_user,
    })
}

/// 批准 CLI 认证挑战。
///
/// 存在性或 token 校验失败返回 `NotFound`；非实例管理员批准
/// `instance_admin_required` 请求返回 `Forbidden`；已过期/已取消的挑战
/// 直接返回当前状态（不签发 Key），不作为错误。
pub async fn approve_cli_auth_challenge(
    pool: &PgPool,
    id: Uuid,
    token: &str,
    approver_user_id: Uuid,
    approver_is_instance_admin: bool,
) -> AuthResult<CliApprovalOutcome> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to begin CLI auth approval: {e}")))?;

    // 先取行锁：并发批准同一挑战时只能有一个事务签发 Key。
    let challenge = PgCliAuthChallengeRepository::find_by_id_for_update(&mut tx, id)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to lock CLI auth challenge: {e}")))?
        .ok_or_else(|| AuthError::not_found(CHALLENGE_NOT_FOUND))?;

    if !verify_api_key(token, &challenge.secret_hash) {
        return Err(AuthError::not_found(CHALLENGE_NOT_FOUND));
    }

    let status = challenge.status();

    // 已过期/已取消：原样返回状态，不签发 Key。
    if status == "expired" || status == "cancelled" {
        return Ok(CliApprovalOutcome {
            approved: false,
            status: status.to_string(),
            user_id: approver_user_id,
            key_id: challenge.board_api_key_id,
            expires_at: challenge.expires_at,
        });
    }

    if challenge.requested_access == ACCESS_INSTANCE_ADMIN_REQUIRED && !approver_is_instance_admin {
        return Err(AuthError::forbidden("Instance admin required"));
    }

    let key_id = match challenge.board_api_key_id {
        Some(existing_key_id) => existing_key_id,
        None => {
            // Board token 的明文只在创建时返回过一次，这里只能复用创建时保存的哈希。
            let key_prefix = challenge
                .pending_key_hash
                .get(..KEY_PREFIX_LEN)
                .unwrap_or_else(|| challenge.pending_key_hash.as_str())
                .to_string();

            // `key_prefix` 是 Parrot 独有的展示列（Paperclip 无对应列），
            // 因此只能取待生效 Key 哈希的前 12 个字符作为展示前缀。
            let key = BoardApiKey::new(
                approver_user_id,
                challenge.pending_key_name,
                challenge.pending_key_hash,
                key_prefix,
                Some(Utc::now() + Duration::days(BOARD_API_KEY_TTL_DAYS)),
            );

            // `board_api_keys.company_id` 在迁移 83 后可空且 Paperclip 无此列，故不绑定（落 NULL）。
            sqlx::query(
                r#"INSERT INTO board_api_keys (
                       id, user_id, name, key_hash, key_prefix, last_used_at, expires_at,
                       is_revoked, revoked_at, revoked_by_user_id, created_at, updated_at
                   )
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
            )
            .bind(key.id)
            .bind(key.user_id)
            .bind(&key.name)
            .bind(&key.key_hash)
            .bind(&key.key_prefix)
            .bind(key.last_used_at)
            .bind(key.expires_at)
            .bind(key.is_revoked)
            .bind(key.revoked_at)
            .bind(key.revoked_by_user_id)
            .bind(key.created_at)
            .bind(key.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AuthError::internal(format!("Failed to create Board API Key: {e}")))?;

            key.id
        }
    };

    // 对齐 Paperclip：保留既有 approved_at（幂等重复批准不改写首次批准时间）。
    let approved_at = challenge.approved_at.unwrap_or_else(Utc::now);
    let updated = PgCliAuthChallengeRepository::mark_approved(
        &mut tx,
        challenge.id,
        approver_user_id,
        key_id,
        approved_at,
    )
    .await
    .map_err(|e| AuthError::internal(format!("Failed to approve CLI auth challenge: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to commit CLI auth approval: {e}")))?;

    let status = updated.status();

    Ok(CliApprovalOutcome {
        approved: status == "approved",
        status: status.to_string(),
        user_id: approver_user_id,
        key_id: updated.board_api_key_id,
        expires_at: updated.expires_at,
    })
}

/// 取消 CLI 认证挑战。
///
/// 存在性或 token 校验失败返回 `NotFound`；已批准/已过期/已取消的挑战
/// 原样返回当前状态，不作为错误。
pub async fn cancel_cli_auth_challenge(
    pool: &PgPool,
    id: Uuid,
    token: &str,
) -> AuthResult<CliCancelOutcome> {
    let repo = PgCliAuthChallengeRepository::new(pool.clone());
    let challenge = load_by_secret(&repo, id, token).await?;

    let status = challenge.status();
    if status != "pending" {
        return Ok(CliCancelOutcome {
            status: status.to_string(),
            cancelled: false,
        });
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to begin CLI auth cancellation: {e}")))?;

    let updated =
        PgCliAuthChallengeRepository::mark_cancelled(&mut tx, challenge.id, Utc::now())
            .await
            .map_err(|e| {
                AuthError::internal(format!("Failed to cancel CLI auth challenge: {e}"))
            })?;

    tx.commit().await.map_err(|e| {
        AuthError::internal(format!("Failed to commit CLI auth cancellation: {e}"))
    })?;

    Ok(CliCancelOutcome {
        status: updated.status().to_string(),
        cancelled: updated.cancelled_at.is_some(),
    })
}
