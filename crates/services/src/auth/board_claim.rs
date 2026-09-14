//! Board 认领流程（Paperclip `server/src/board-claim.ts` 的移植）。
//!
//! 场景：实例曾经以 `local_trusted` 运行，`instance_user_roles` 里只有一个
//! 本地 board 管理员。切换到 `authenticated` 后，真实用户需要凭一次性挑战
//! 接管这个实例。
//!
//! 与 Paperclip 一致，挑战是**进程级单例**：只在启动时按部署模式创建一次，
//! 由 `inspect` / `claim` 读取。重启进程即生成新挑战（旧 URL 失效）。
//!
//! 提供：
//! - [`initialize_board_claim_challenge`]：启动时按部署模式决定是否创建挑战
//! - [`inspect_board_claim_challenge`]：查看挑战状态（校验 token + code）
//! - [`claim_board_ownership`]：认领所有权（提升为 instance_admin + 所有公司 owner）
//! - [`claim_first_instance_admin`]：首次管理员认领（`POST /bootstrap/claim`）
//!
//! 与 Paperclip 的差异（Parrot 特有）：Paperclip 用文本常量 `"local-board"`
//! 标识本地 board 主体，而 Parrot 的 `auth_users.id` 是 UUID，本地主体由
//! `LOCAL_TRUSTED_USER_ID`（回退到最早创建的 `auth_users` 行）标识。

use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::middleware::AuthMode;
use crate::auth::{AuthError, AuthResult, MembershipRole};

/// 挑战有效期：24 小时（Paperclip `CLAIM_TTL_MS = 1000 * 60 * 60 * 24`）。
const CLAIM_TTL_HOURS: i64 = 24;

/// 挑战状态（序列化为小写，与 Paperclip `ChallengeStatus` 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Available,
    Claimed,
    Expired,
    Invalid,
}

/// 进程级活跃挑战（Paperclip `activeChallenge`）。
#[derive(Debug, Clone)]
pub struct ClaimChallenge {
    pub token: String,
    pub code: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claimed_by_user_id: Option<Uuid>,
}

impl ClaimChallenge {
    /// 新建挑战：token 24 字节 hex、code 12 字节 hex（与 Paperclip 同）。
    fn new() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut token_bytes = [0u8; 24];
        let mut code_bytes = [0u8; 12];
        rng.fill(&mut token_bytes);
        rng.fill(&mut code_bytes);
        let now = Utc::now();
        Self {
            token: hex::encode(token_bytes),
            code: hex::encode(code_bytes),
            created_at: now,
            expires_at: now + Duration::hours(CLAIM_TTL_HOURS),
            claimed_at: None,
            claimed_by_user_id: None,
        }
    }
}

/// 进程级单例存储。Paperclip 用模块级 `let activeChallenge`；Rust 侧用
/// `LazyLock<Mutex<...>>` 达到同样效果——挑战必须跨请求存活。
static CLAIM_STORE: LazyLock<Mutex<Option<ClaimChallenge>>> = LazyLock::new(|| Mutex::new(None));

fn store() -> &'static Mutex<Option<ClaimChallenge>> {
    &CLAIM_STORE
}

fn read_challenge() -> Option<ClaimChallenge> {
    store().lock().expect("claim store poisoned").clone()
}

fn write_challenge(challenge: Option<ClaimChallenge>) {
    *store().lock().expect("claim store poisoned") = challenge;
}

/// 按 Paperclip `getChallengeStatus` 判定状态。
///
/// 注意判定顺序：token/code 不匹配优先返回 `invalid`；已认领优先于过期。
pub fn get_challenge_status(token: &str, code: Option<&str>) -> ClaimStatus {
    let challenge = match read_challenge() {
        Some(c) => c,
        None => return ClaimStatus::Invalid,
    };
    if challenge.token != token {
        return ClaimStatus::Invalid;
    }
    if challenge.code != code.unwrap_or("") {
        return ClaimStatus::Invalid;
    }
    if challenge.claimed_at.is_some() {
        return ClaimStatus::Claimed;
    }
    if challenge.expires_at <= Utc::now() {
        return ClaimStatus::Expired;
    }
    ClaimStatus::Available
}

/// 对外暴露的挑战检视结果（Paperclip `inspectBoardClaimChallenge`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardClaimInspection {
    pub status: ClaimStatus,
    /// 认领必须由已登录的浏览器会话完成，因此恒为 true。
    pub requires_sign_in: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub claimed_by_user_id: Option<Uuid>,
}

/// 检视挑战状态（不消费挑战）。
pub fn inspect_board_claim_challenge(token: &str, code: Option<&str>) -> BoardClaimInspection {
    let status = get_challenge_status(token, code);
    let challenge = read_challenge();
    BoardClaimInspection {
        status,
        requires_sign_in: true,
        expires_at: challenge.as_ref().map(|c| c.expires_at),
        claimed_by_user_id: challenge.and_then(|c| c.claimed_by_user_id),
    }
}

/// 启动时初始化挑战（Paperclip `initializeBoardClaimChallenge`）。
///
/// 仅当部署模式为 `authenticated`、且 `instance_user_roles` 中恰好只有
/// 一个 `instance_admin` 且它就是本地 board 主体时才创建挑战；否则清空。
/// 已有挑战未过期且未被认领时保持不变。
pub async fn initialize_board_claim_challenge(
    pool: &PgPool,
    deployment_mode: AuthMode,
) -> AuthResult<()> {
    if deployment_mode != AuthMode::Authenticated {
        write_challenge(None);
        return Ok(());
    }

    let local_board_user_id = resolve_local_board_user_id(pool).await?;
    let admin_user_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM instance_user_roles WHERE role = 'instance_admin'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to list instance admins: {e}")))?;

    let only_local_board_admin =
        admin_user_ids.len() == 1 && Some(admin_user_ids[0]) == local_board_user_id;
    if !only_local_board_admin {
        write_challenge(None);
        return Ok(());
    }

    let needs_new_challenge = match read_challenge() {
        None => true,
        Some(c) => c.expires_at <= Utc::now() || c.claimed_at.is_some(),
    };
    if needs_new_challenge {
        write_challenge(Some(ClaimChallenge::new()));
    }
    Ok(())
}

/// 返回当前可用于告警横幅的认领 URL（Penclip `getBoardClaimWarningUrl`）。
///
/// 无挑战、已认领或已过期时返回 `None`。
pub fn get_board_claim_warning_url(host: &str, port: u16) -> Option<String> {
    let challenge = read_challenge()?;
    if challenge.claimed_at.is_some() || challenge.expires_at <= Utc::now() {
        return None;
    }
    let visible_host = if host == "0.0.0.0" { "localhost" } else { host };
    Some(format!(
        "http://{visible_host}:{port}/board-claim/{}?code={}",
        challenge.token, challenge.code
    ))
}

/// 认领结果（Paperclip `claimBoardOwnership` 的返回值）。
#[derive(Debug, Clone)]
pub enum BoardClaimOutcome {
    Claimed { claimed_by_user_id: Uuid },
    /// 非 `available` 状态：invalid / claimed / expired 原样透出。
    NotAvailable(ClaimStatus),
}

/// 认领 Board 所有权。
///
/// 严格按 Paperclip 的四步执行：
/// 1. 若目标用户尚无 `instance_admin` 则插入；
/// 2. 删除本地 board 主体的 `instance_admin`；
/// 3. 对每个公司：无成员关系则插入 `{status: active, membership_role: owner}`；
///    已存在且状态不是 `active` 才更新为 active + owner；
/// 4. 事务提交后，为每个公司补齐 owner 的默认权限授予。
///
/// 注意：Paperclip **不**归档本地 board 的成员关系，仅移除其实例管理员角色。
pub async fn claim_board_ownership(
    pool: &PgPool,
    token: &str,
    code: Option<&str>,
    user_id: Uuid,
) -> AuthResult<BoardClaimOutcome> {
    if get_challenge_status(token, code) != ClaimStatus::Available {
        return Ok(BoardClaimOutcome::NotAvailable(get_challenge_status(
            token, code,
        )));
    }

    let local_board_user_id = resolve_local_board_user_id(pool)
        .await?
        .ok_or_else(|| AuthError::internal("No local board principal found to claim"))?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to begin transaction: {e}")))?;

    // 1. 目标用户获得 instance_admin（已存在则跳过）。
    let existing_target_admin = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to check target instance admin: {e}")))?;

    if existing_target_admin.is_none() {
        let role = repositories::models::auth::InstanceUserRole::new(
            user_id,
            "instance_admin".to_string(),
        );
        sqlx::query(
            "INSERT INTO instance_user_roles (id, user_id, role, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $4) \
             ON CONFLICT (user_id, role) DO NOTHING",
        )
        .bind(role.id)
        .bind(role.user_id)
        .bind(&role.role)
        .bind(role.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to grant instance admin: {e}")))?;
    }

    // 2. 移除本地 board 主体的 instance_admin。
    sqlx::query("DELETE FROM instance_user_roles WHERE user_id = $1 AND role = 'instance_admin'")
        .bind(local_board_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to remove local board admin: {e}")))?;

    // 3. 目标用户在每一个公司成为 owner。
    let company_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM companies")
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to list companies: {e}")))?;

    for company_id in &company_ids {
        let existing_status: Option<String> = sqlx::query_scalar(
            "SELECT status::text FROM company_memberships \
             WHERE company_id = $1 AND principal_type = 'user'::principal_type AND principal_id = $2 \
             LIMIT 1",
        )
        .bind(company_id)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to load membership: {e}")))?;

        match existing_status.as_deref() {
            None => {
                sqlx::query(
                    "INSERT INTO company_memberships \
                     (company_id, principal_type, principal_id, status, membership_role, created_at, updated_at) \
                     VALUES ($1, 'user'::principal_type, $2, \
                             'active'::company_membership_status, 'owner'::membership_role, NOW(), NOW())",
                )
                .bind(company_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AuthError::internal(format!("Failed to create owner membership: {e}")))?;
            }
            Some(status) if status != "active" => {
                sqlx::query(
                    "UPDATE company_memberships \
                     SET status = 'active'::company_membership_status, \
                         membership_role = 'owner'::membership_role, \
                         updated_at = NOW() \
                     WHERE company_id = $1 AND principal_type = 'user'::principal_type AND principal_id = $2",
                )
                .bind(company_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AuthError::internal(format!("Failed to promote owner membership: {e}")))?;
            }
            Some(_) => {}
        }
    }

    tx.commit()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to commit claim transaction: {e}")))?;

    // 4. 提交后补齐 owner 默认权限授予（Paperclip `ensureHumanRoleDefaultGrants`）。
    for company_id in &company_ids {
        crate::auth::middleware::ensure_human_role_default_grants(
            pool,
            *company_id,
            user_id,
            MembershipRole::Owner,
        )
        .await;
    }

    // 记录认领者，之后 token 判定为 claimed。
    {
        let mut guard = store().lock().expect("claim store poisoned");
        if let Some(challenge) = guard.as_mut() {
            if challenge.token == token {
                challenge.claimed_at = Some(Utc::now());
                challenge.claimed_by_user_id = Some(user_id);
            }
        }
    }

    Ok(BoardClaimOutcome::Claimed {
        claimed_by_user_id: user_id,
    })
}

/// 首次管理员认领结果（Paperclip `FirstAdminClaimResult`）。
#[derive(Debug, Clone)]
pub enum FirstAdminClaimOutcome {
    Claimed { user_id: Uuid },
    AlreadyClaimed { existing_user_id: Option<Uuid> },
}

/// 首次管理员认领：实例尚无任何 `instance_admin` 时把该用户提升为管理员。
///
/// 按 Paperclip `claimFirstInstanceAdmin` 使用表级锁
/// （`LOCK TABLE instance_user_roles IN SHARE ROW EXCLUSIVE MODE`）串行化并发认领。
pub async fn claim_first_instance_admin(
    pool: &PgPool,
    user_id: Uuid,
) -> AuthResult<FirstAdminClaimOutcome> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to begin transaction: {e}")))?;

    sqlx::query("LOCK TABLE instance_user_roles IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .map_err(|e| AuthError::internal(format!("Failed to lock instance_user_roles: {e}")))?;

    let existing_admin: Option<Uuid> = sqlx::query_scalar(
        "SELECT user_id FROM instance_user_roles WHERE role = 'instance_admin' LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to check instance admins: {e}")))?;

    if let Some(existing_user_id) = existing_admin {
        return Ok(FirstAdminClaimOutcome::AlreadyClaimed {
            existing_user_id: Some(existing_user_id),
        });
    }

    let role = repositories::models::auth::InstanceUserRole::new(
        user_id,
        "instance_admin".to_string(),
    );
    sqlx::query(
        "INSERT INTO instance_user_roles (id, user_id, role, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $4)",
    )
    .bind(role.id)
    .bind(role.user_id)
    .bind(&role.role)
    .bind(role.created_at)
    .execute(&mut *tx)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to grant first instance admin: {e}")))?;

    tx.commit()
        .await
        .map_err(|e| AuthError::internal(format!("Failed to commit first admin claim: {e}")))?;

    Ok(FirstAdminClaimOutcome::Claimed { user_id })
}

/// 解析本地 board 主体 ID。
///
/// Parrot 用 UUID 标识用户，因此不存在 Paperclip 的 `"local-board"` 文本常量。
/// 对应关系：本地主体 = `LOCAL_TRUSTED_USER_ID`（由 `main` 在启动时写入），
/// 回退到最早创建的 `auth_users` 行——与
/// `crates/server/src/main.rs:ensure_local_trusted_principal` 的选取规则一致。
async fn resolve_local_board_user_id(pool: &PgPool) -> AuthResult<Option<Uuid>> {
    if let Some(configured) = std::env::var("LOCAL_TRUSTED_USER_ID")
        .ok()
        .and_then(|value| Uuid::parse_str(&value).ok())
    {
        return Ok(Some(configured));
    }

    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM auth_users ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::internal(format!("Failed to resolve local board principal: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_generates_expected_token_shapes() {
        let challenge = ClaimChallenge::new();
        assert_eq!(challenge.token.len(), 48, "24 bytes hex");
        assert_eq!(challenge.code.len(), 24, "12 bytes hex");
        assert!(challenge.claimed_at.is_none());
        assert!(
            challenge.expires_at > challenge.created_at,
            "expiry must be in the future"
        );
    }

    #[test]
    fn status_is_invalid_without_an_active_challenge() {
        // 单例在测试进程中可能已被其他用例写过，先清空。
        write_challenge(None);
        assert_eq!(get_challenge_status("token", Some("code")), ClaimStatus::Invalid);
    }

    #[test]
    fn status_precedence_matches_paperclip() {
        let mut challenge = ClaimChallenge::new();
        let token = challenge.token.clone();
        let code = challenge.code.clone();

        write_challenge(Some(challenge.clone()));
        assert_eq!(
            get_challenge_status(&token, Some(&code)),
            ClaimStatus::Available
        );

        // token 不匹配优先 invalid
        assert_eq!(
            get_challenge_status("wrong", Some(&code)),
            ClaimStatus::Invalid
        );
        // code 不匹配优先 invalid
        assert_eq!(get_challenge_status(&token, Some("wrong")), ClaimStatus::Invalid);
        // 缺 code 视为空串 → invalid
        assert_eq!(get_challenge_status(&token, None), ClaimStatus::Invalid);

        // 已认领优先于过期
        challenge.claimed_at = Some(Utc::now());
        challenge.expires_at = Utc::now() - Duration::hours(1);
        write_challenge(Some(challenge));

        assert_eq!(get_challenge_status(&token, Some(&code)), ClaimStatus::Claimed);
        write_challenge(None);
    }

    #[test]
    fn expired_challenge_is_reported_as_expired() {
        let mut challenge = ClaimChallenge::new();
        let token = challenge.token.clone();
        let code = challenge.code.clone();
        challenge.expires_at = Utc::now() - Duration::seconds(1);
        write_challenge(Some(challenge));

        assert_eq!(
            get_challenge_status(&token, Some(&code)),
            ClaimStatus::Expired
        );
        write_challenge(None);
    }
}
