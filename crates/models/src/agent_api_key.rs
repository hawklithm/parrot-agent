use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent API Key 结构体
/// 用于 Agent 自我认证（GET /agents/me）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentApiKey {
    /// API Key ID
    pub id: Uuid,
    /// 关联的 Agent ID
    pub agent_id: Uuid,
    /// 关联的 Company ID
    pub company_id: Uuid,
    /// 密钥名称/描述
    pub name: String,
    /// 密钥哈希值（bcrypt）
    pub key_hash: String,
    /// 最后使用时间
    pub last_used_at: Option<DateTime<Utc>>,
    /// 撤销时间（NULL表示未撤销）
    pub revoked_at: Option<DateTime<Utc>>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl AgentApiKey {
    /// 检查密钥是否有效（未撤销）
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}
