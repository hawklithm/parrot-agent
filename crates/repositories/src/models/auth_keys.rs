use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Board API密钥表 - board_api_keys
///
/// 存储Board用户的API密钥，用于CLI和第三方工具认证
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BoardApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BoardApiKey {
    /// 创建新的Board API密钥
    pub fn new(
        user_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            name,
            key_hash,
            key_prefix,
            last_used_at: None,
            expires_at,
            is_revoked: false,
            revoked_at: None,
            revoked_by_user_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 检查密钥是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// 检查密钥是否有效（未撤销且未过期）
    pub fn is_valid(&self) -> bool {
        !self.is_revoked && !self.is_expired()
    }

    /// 撤销密钥
    pub fn revoke(&mut self, revoked_by_user_id: Uuid) {
        self.is_revoked = true;
        self.revoked_at = Some(Utc::now());
        self.revoked_by_user_id = Some(revoked_by_user_id);
        self.updated_at = Utc::now();
    }

    /// 记录密钥使用
    pub fn record_usage(&mut self) {
        self.last_used_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// Agent API密钥表 - agent_api_keys
///
/// 存储Agent的API密钥，支持细粒度权限控制
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AgentApiKey {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub scope: sqlx::types::JsonValue,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentApiKey {
    /// 创建新的Agent API密钥
    pub fn new(
        agent_id: Uuid,
        company_id: Uuid,
        name: String,
        key_hash: String,
        key_prefix: String,
        scope: sqlx::types::JsonValue,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            agent_id,
            company_id,
            name,
            key_hash,
            key_prefix,
            scope,
            last_used_at: None,
            expires_at,
            is_revoked: false,
            revoked_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 检查密钥是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// 检查密钥是否有效（未撤销且未过期）
    pub fn is_valid(&self) -> bool {
        !self.is_revoked && !self.is_expired()
    }

    /// 撤销密钥
    pub fn revoke(&mut self) {
        self.is_revoked = true;
        self.revoked_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 记录密钥使用
    pub fn record_usage(&mut self) {
        self.last_used_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// CLI认证挑战表 - cli_auth_challenges
///
/// 对齐 Paperclip 设备授权流程：服务端只保存挑战 secret 与待生效 Board token 的哈希，
/// 两者的明文仅在创建时返回一次。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CliAuthChallenge {
    pub id: Uuid,
    pub secret_hash: String,
    pub command: String,
    pub client_name: Option<String>,
    pub requested_access: String,
    pub requested_company_id: Option<Uuid>,
    pub pending_key_hash: String,
    pub pending_key_name: String,
    pub approved_by_user_id: Option<Uuid>,
    pub board_api_key_id: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CliAuthChallenge {
    /// 派生挑战状态（对齐 Paperclip `challengeStatusForRow`）。
    ///
    /// 优先级：`cancelled` > `expired` > `approved` > `pending`。
    /// 其中 `approved` 要求 `approved_at` 与 `board_api_key_id` 同时存在。
    pub fn status(&self) -> &'static str {
        if self.cancelled_at.is_some() {
            "cancelled"
        } else if self.is_expired() {
            "expired"
        } else if self.approved_at.is_some() && self.board_api_key_id.is_some() {
            "approved"
        } else {
            "pending"
        }
    }

    /// 检查挑战是否已过期（对齐 Paperclip `expiresAt <= now`）。
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_api_key_validity() {
        let user_id = Uuid::new_v4();
        let key = BoardApiKey::new(
            user_id,
            "Test Key".to_string(),
            "hashed_key".to_string(),
            "pk_test".to_string(),
            None,
        );

        assert!(key.is_valid());
        assert!(!key.is_expired());
        assert!(!key.is_revoked);
    }

    #[test]
    fn test_board_api_key_revoke() {
        let user_id = Uuid::new_v4();
        let mut key = BoardApiKey::new(
            user_id,
            "Test Key".to_string(),
            "hashed_key".to_string(),
            "pk_test".to_string(),
            None,
        );

        let revoker_id = Uuid::new_v4();
        key.revoke(revoker_id);

        assert!(key.is_revoked);
        assert!(!key.is_valid());
        assert_eq!(key.revoked_by_user_id, Some(revoker_id));
    }

    #[test]
    fn test_agent_api_key_validity() {
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = serde_json::json!({"type": "global"});

        let key = AgentApiKey::new(
            agent_id,
            company_id,
            "Agent Key".to_string(),
            "hashed_key".to_string(),
            "ak_test".to_string(),
            scope,
            None,
        );

        assert!(key.is_valid());
        assert!(!key.is_expired());
    }

    fn cli_challenge_with(expires_at: DateTime<Utc>) -> CliAuthChallenge {
        let now = Utc::now();
        CliAuthChallenge {
            id: Uuid::new_v4(),
            secret_hash: "secret_hash".to_string(),
            command: "paperclipai company import".to_string(),
            client_name: None,
            requested_access: "board".to_string(),
            requested_company_id: None,
            pending_key_hash: "pending_key_hash".to_string(),
            pending_key_name: "paperclipai cli (board)".to_string(),
            approved_by_user_id: None,
            board_api_key_id: None,
            approved_at: None,
            cancelled_at: None,
            expires_at,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_cli_auth_challenge_status_derivation() {
        let future = Utc::now() + chrono::Duration::minutes(10);
        let past = Utc::now() - chrono::Duration::minutes(1);

        assert_eq!(cli_challenge_with(future).status(), "pending");
        assert_eq!(cli_challenge_with(past).status(), "expired");

        let mut approved = cli_challenge_with(future);
        approved.approved_at = Some(Utc::now());
        approved.board_api_key_id = Some(Uuid::new_v4());
        assert_eq!(approved.status(), "approved");

        // approved_at 存在但缺少 board_api_key_id → 仍为 pending（对齐 Paperclip）
        let mut half_approved = cli_challenge_with(future);
        half_approved.approved_at = Some(Utc::now());
        assert_eq!(half_approved.status(), "pending");

        let mut cancelled = cli_challenge_with(future);
        cancelled.cancelled_at = Some(Utc::now());
        assert_eq!(cancelled.status(), "cancelled");

        // 优先级：cancelled 覆盖 approved，也覆盖 expired
        let mut cancelled_after_approval = cli_challenge_with(future);
        cancelled_after_approval.approved_at = Some(Utc::now());
        cancelled_after_approval.board_api_key_id = Some(Uuid::new_v4());
        cancelled_after_approval.cancelled_at = Some(Utc::now());
        assert_eq!(cancelled_after_approval.status(), "cancelled");

        let mut cancelled_when_expired = cli_challenge_with(past);
        cancelled_when_expired.cancelled_at = Some(Utc::now());
        assert_eq!(cancelled_when_expired.status(), "cancelled");
    }
}
