use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 用户密钥定义 - 定义公司级别的用户密钥模板
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSecretDefinition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub description: Option<String>,
    pub required: bool,
    pub scope: sqlx::types::Json<UserSecretScope>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Uuid,
}

/// 用户密钥作用域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSecretScope {
    pub project_ids: Option<Vec<Uuid>>,
    pub agent_ids: Option<Vec<Uuid>>,
    pub applies_to_all: bool,
}

/// 用户密钥实例 - 具体用户的密钥值
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserSecret {
    pub id: Uuid,
    pub user_id: Uuid,
    pub definition_id: Uuid,
    pub encrypted_value: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_rotated_at: Option<DateTime<Utc>>,
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
            key,
            description,
            required,
            scope: sqlx::types::Json(scope),
            created_at: now,
            updated_at: now,
            created_by_user_id,
        }
    }
}

impl UserSecret {
    pub fn new(user_id: Uuid, definition_id: Uuid, encrypted_value: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            user_id,
            definition_id,
            encrypted_value,
            created_at: now,
            updated_at: now,
            last_rotated_at: None,
        }
    }

    pub fn rotate(&mut self, new_encrypted_value: String) {
        self.encrypted_value = new_encrypted_value;
        self.last_rotated_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
