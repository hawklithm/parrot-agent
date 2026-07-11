use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// 密钥提供商配置
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SecretProviderConfig {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider_type: String,
    pub config: sqlx::types::Json<serde_json::Value>,
    pub is_default: bool,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Uuid,
}

/// 提供商健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// 提供商健康检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealthCheck {
    pub provider_config_id: Uuid,
    pub status: ProviderHealthStatus,
    pub latency_ms: Option<u64>,
    pub error_message: Option<String>,
    pub checked_at: DateTime<Utc>,
}

/// 密钥发现候选
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDiscoveryCandidate {
    pub external_ref: String,
    pub suggested_name: String,
    pub tags: Vec<(String, String)>,
    pub created_at: Option<DateTime<Utc>>,
    pub last_modified_at: Option<DateTime<Utc>>,
    pub conflict: Option<String>,
}

/// 密钥发现预览结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretDiscoveryPreview {
    pub provider: String,
    pub next_token: Option<String>,
    pub sampled_secret_count: usize,
    pub skipped_foreign_paperclip_sample_count: usize,
    pub candidates: Vec<SecretDiscoveryCandidate>,
    pub warnings: Vec<String>,
}

/// Conflict resolution strategy for remote import
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Skip secrets that already exist
    Skip,
    /// Overwrite existing secrets
    Overwrite,
    /// Create new version of existing secrets
    Version,
}

/// Remote import preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImportPreview {
    pub provider: String,
    pub total_secrets: usize,
    pub new_secrets: usize,
    pub conflicting_secrets: usize,
    pub candidates: Vec<SecretDiscoveryCandidate>,
    pub warnings: Vec<String>,
}

/// Remote import execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
 iled_count: usize,
    pub imported_keys: Vec<String>,
    pub skipped_keys: Vec<String>,
    pub errors: Vec<ImportError>,
}

/// Import error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub secret_key: String,
    pub error_message: String,
}

impl SecretProviderConfig {
    pub fn new(
        company_id: Uuid,
        provider_type: String,
        config: serde_json::Value,
        created_by_user_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),     company_id,
            provider_type,
            config: sqlx::types::Json(config),
            is_default: false,
            enabled: true,
            created_at: now,
            updated_at: now,
            created_by_user_id,
        }
    }
}
