use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 用户密钥定义 - 定义公司级别的用户密钥模板（paperclip-aligned: user_secret_definitions）
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSecretDefinition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub provider: String,
    pub managed_mode: String,
    pub provider_config_id: Option<Uuid>,
    pub provider_metadata: Option<serde_json::Value>,
    pub usage_guidance: Option<String>,
    pub required: bool,
    pub scope: sqlx::types::Json<UserSecretScope>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Option<Uuid>,
    pub updated_by_user_id: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// 用户密钥作用域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSecretScope {
    pub project_ids: Option<Vec<Uuid>>,
    pub agent_ids: Option<Vec<Uuid>>,
    pub applies_to_all: bool,
}

/// 用户密钥实例 - 具体用户为某定义提交的值（paperclip-aligned: user_secret_declarations）
///
/// 每个 (user_id, definition_id) 对应一行声明；加密后的值存于 `value_material`
/// （parrot 扩展列，paperclip 本身不存值，值由用户在 UI 提供并走 provider）。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSecret {
    pub id: Uuid,
    pub company_id: Uuid,
    pub user_secret_definition_id: Uuid,
    pub user_id: Uuid,
    pub env_key: String,
    pub value_material: Option<String>,
    pub value_sha256: Option<String>,
    pub version_selector: String,
    pub required: bool,
    pub allow_missing_override: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 用户密钥覆盖率统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSecretCoverage {
    pub definition_id: Uuid,
    pub definition_key: String,
    pub total_users: i64,
    pub users_with_secret: i64,
    pub coverage_percentage: f64,
    pub required: bool,
}

/// 密钥绑定信息 - 哪些资源使用了这个密钥
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretBinding {
    pub resource_type: String,
    pub resource_id: Uuid,
    pub resource_name: Option<String>,
    pub bound_at: DateTime<Utc>,
}

impl UserSecretDefinition {
    pub fn new(
        company_id: Uuid,
        key: String,
        description: Option<String>,
        required: bool,
        scope: UserSecretScope,
        created_by_user_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            key: key.clone(),
            name: Some(key),
            description,
            status: "active".to_string(),
            provider: "local_encrypted".to_string(),
            managed_mode: "paperclip_managed".to_string(),
            provider_config_id: None,
            provider_metadata: None,
            usage_guidance: None,
            required,
            scope: sqlx::types::Json(scope),
            created_at: now,
            updated_at: now,
            created_by_user_id: Some(created_by_user_id),
            updated_by_user_id: Some(created_by_user_id),
            deleted_at: None,
        }
    }
}

impl UserSecret {
    pub fn new(
        company_id: Uuid,
        user_id: Uuid,
        definition_id: Uuid,
        env_key: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            user_secret_definition_id: definition_id,
            user_id,
            env_key,
            value_material: None,
            value_sha256: None,
            version_selector: "latest".to_string(),
            required: true,
            allow_missing_override: false,
            created_at: now,
            updated_at: now,
        }
    }
}
