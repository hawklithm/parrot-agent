use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Type;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum SecretScope {
    Company,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum SecretManagedMode {
    PaperclipManaged,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum SecretStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CompanySecret {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub key: String,
    pub provider: Option<String>,
    pub provider_config_id: Option<Uuid>,
    pub managed_mode: SecretManagedMode,
    pub scope: SecretScope,
    pub description: Option<String>,
    pub status: SecretStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecretInput {
    pub company_id: Uuid,
    pub name: String,
    pub key: String,
    pub provider: Option<String>,
    pub provider_config_id: Option<Uuid>,
    pub managed_mode: SecretManagedMode,
    pub scope: SecretScope,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSecretInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<SecretStatus>,
    pub provider: Option<String>,
    pub provider_config_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum SecretProviderType {
    LocalEncrypted,
    AwsSecretsManager,
    GcpSecretManager,
    Vault,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SecretProviderConfig {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider_type: SecretProviderType,
    pub name: String,
    pub config: JsonValue,
    pub is_default: bool,
    pub status: SecretStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecretProviderConfigInput {
    pub company_id: Uuid,
    pub provider_type: SecretProviderType,
    pub name: String,
    pub config: JsonValue,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSecretProviderConfigInput {
    pub name: Option<String>,
    pub config: Option<JsonValue>,
    pub is_default: Option<bool>,
    pub status: Option<SecretStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserSecretDefinition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub required: bool,
    pub status: String,
    pub provider: String,
    pub managed_mode: String,
    pub provider_config_id: Option<Uuid>,
    pub provider_metadata: Option<String>,
    pub usage_guidance: Option<String>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub updated_by_user_id: Option<Uuid>,
    pub updated_by_agent_id: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserSecretDefinitionInput {
    pub company_id: Uuid,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserSecretDefinitionInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserSecret {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub user_id: Uuid,
    pub value_ref: String,
    pub status: SecretStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserSecretInput {
    pub definition_id: Uuid,
    pub user_id: Uuid,
    pub value_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserSecretInput {
    pub value_ref: Option<String>,
    pub status: Option<SecretStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SecretBinding {
    pub id: Uuid,
    pub secret_id: Uuid,
    pub target_type: SecretBindingTargetType,
    pub target_id: Uuid,
    pub env_key: Option<String>,
    pub config_path: Option<String>,
    pub required: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum SecretBindingTargetType {
    Agent,
    Environment,
    Project,
    Routine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealthStatus {
    pub provider_id: Uuid,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}
