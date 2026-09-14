use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Row shape of `company_secret_provider_configs`.
///
/// Mirrors the Paperclip `companySecretProviderConfigs` table column-for-column;
/// the API model (`CompanySecretProviderConfig` in `secret_provider_config.rs`)
/// is a distinct wire type and must not be confused with this DB row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SecretProviderConfig {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider: String,
    pub display_name: String,
    pub status: String,
    pub is_default: bool,
    pub config: serde_json::Value,
    pub health_status: Option<String>,
    pub health_checked_at: Option<DateTime<Utc>>,
    pub health_message: Option<String>,
    pub health_details: Option<serde_json::Value>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a provider configuration row.
///
/// `status` and `is_default` arrive already resolved by the service layer
/// (default status per provider, and `is_default` forced false for
/// `coming_soon`/`disabled`), so this struct is a direct persistence payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecretProviderConfigInput {
    pub company_id: Uuid,
    pub provider: String,
    pub display_name: String,
    pub status: String,
    pub is_default: bool,
    pub config: serde_json::Value,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
}

/// Partial update for a provider configuration row.
///
/// Every field is optional; `None` means "leave unchanged". `status` and
/// `is_default` are re-derived together by the service so that a config moving
/// into `coming_soon`/`disabled` always drops its default flag.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateSecretProviderConfigInput {
    pub display_name: Option<String>,
    pub status: Option<String>,
    pub is_default: Option<bool>,
    pub config: Option<serde_json::Value>,
}
