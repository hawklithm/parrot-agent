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
/// 存储CLI认证流程的挑战码，支持设备授权流程
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CliAuthChallenge {
    pub id: Uuid,
    pub user_id: Uuid,
    pub company_id: Option<Uuid>,
    pub challenge_code: String,
    pub device_name: Option<String>,
    pub requested_access: sqlx::types::JsonValue,
    pub status: String,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by_user_id: Option<Uuid>,
    pub api_key_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CliAuthChallenge {
    /// 创建新的CLI认证挑战
    pub fn new(
        user_id: Uuid,
        company_id: Option<Uuid>,
        challenge_code: String,
        device_name: Option<String>,
        requested_access: sqlx::types::JsonValue,
        ttl_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            company_id,
            challenge_code,
            device_name,
            requested_access,
            status: "pending".to_string(),
            approved_at: None,
            approved_by_user_id: None,
            api_key_id: None,
            expires_at: now + chrono::Duration::seconds(ttl_seconds),
            created_at: now,
            updated_at: now,
        }
    }

    /// 生成挑战码（8位随机字符）
    pub fn generate_challenge_code() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// 检查挑战是否已过期
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// 检查挑战状态
    pub fn is_pending(&self) -> bool {
        self.status == "pending"
    }

    pub fn is_approved(&self) -> bool {
        self.status == "approved"
    }

    pub fn is_rejected(&self) -> bool {
        self.status == "rejected"
    }

    /// 批准挑战
    pub fn approve(&mut self, approved_by_user_id: Uuid, api_key_id: Uuid) {
        self.status = "approved".to_string();
        self.approved_at = Some(Utc::now());
        self.approved_by_user_id = Some(approved_by_user_id);
        self.api_key_id = Some(api_key_id);
        self.updated_at = Utc::now();
    }

    /// 拒绝挑战
    pub fn reject(&mut self) {
        self.status = "rejected".to_string();
        self.updated_at = Utc::now();
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

    #[test]
    fn test_cli_auth_challenge_generation() {
        let code =Challenge::generate_challenge_code();
        assert_eq!(code.len(), 8);
        assert!(code.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn test_cli_auth_challenge_approval() {
        let user_id = Uuid::new_v4();
        let mut challenge = CliAuthChallenge::new(
            user_id,
            None,
            "ABCD1234".to_string(),
            Some("My Device".to_string()),
            serde_json::json!({}),
            300,
        );

        assert!(challenge.is_pending());

        let approver_id = Uuid::new_v4();
        let api_key_id = Uuid::new_v4();
        challenge.approve(approver_id, api_key_id);

        assert!(challenge.is_approved());
        assert_eq!(challenge.api_key_id, Some(api_key_id));
    }
}
