use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Secret provider type enum (aws_secrets_manager, gcp_secret_manager, vault, local_encrypted)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProvider {
    LocalEncrypted,
    AwsSecretsManager,
    GcpSecretManager,
    Vault,
}

/// Provider configuration status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProviderConfigStatus {
    Ready,
    Warning,
    ComingSoon,
    Disabled,
}

/// Provider configuration health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProviderConfigHealthStatus {
    Ready,
    Warning,
    Error,
    ComingSoon,
    Disabled,
}

/// Local encrypted provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEncryptedProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_reminder_acknowledged: Option<bool>,
}

/// AWS Secrets Manager provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwsSecretsManagerProviderConfig {
    pub region: String,
    pub endpoint: String,
    pub deployment_id: String,
    pub prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kms_key_id: Option<String>,
    pub environment_tag: String,
    pub provider_owner_tag: String,
    #[serde(default = "default_recovery_window")]
    pub delete_recovery_window_days: i32,
}

fn default_recovery_window() -> i32 {
    30
}

/// GCP Secret Manager provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GcpSecretManagerProviderConfig {
    pub project_id: String,
    pub service_account_email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_path_prefix: Option<String>,
}

/// Vault provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultProviderConfig {
    pub address: String,
    pub mount_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_path_prefix: Option<String>,
}

/// Union type for provider-specific configurations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SecretProviderConfigPayload {
    LocalEncrypted(LocalEncryptedProviderConfig),
    AwsSecretsManager(AwsSecretsManagerProviderConfig),
    GcpSecretManager(GcpSecretManagerProviderConfig),
    Vault(VaultProviderConfig),
}

/// Health check details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigHealthDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<Vec<String>>,
}

/// Company secret provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySecretProviderConfig {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider: SecretProvider,
    pub display_name: String,
    pub status: SecretProviderConfigStatus,
    pub is_default: bool,
    pub config: SecretProviderConfigPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_status: Option<SecretProviderConfigHealthStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_checked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_details: Option<SecretProviderConfigHealthDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSecretProviderConfigRequest {
    pub provider: SecretProvider,
    pub display_name: String,
    pub config: SecretProviderConfigPayload,
    #[serde(default)]
    pub set_as_default: bool,
}

/// Request to update an existing provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSecretProviderConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<SecretProviderConfigPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SecretProviderConfigStatus>,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigHealthResponse {
    pub config_id: Uuid,
    pub provider: SecretProvider,
    pub status: SecretProviderConfigHealthStatus,
    pub message: String,
    pub details: SecretProviderConfigHealthDetails,
    pub checked_at: DateTime<Utc>,
}

/// Discovery signal (patterns found during secret scanning)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoverySignal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_name_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kms_key_id: Option<String>,
    pub has_kms_key: bool,
    pub sample_count: usize,
    pub paperclip_managed_sample_count: usize,
    pub skipped_foreign_paperclip_sample_count: usize,
}

/// Discovery sample (individual secret found during scan)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoverySample {
    pub name: String,
    pub has_kms_key: bool,
    pub tag_keys: Vec<String>,
}

/// Discovery candidate (suggested provider configuration)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoveryCandidate {
    pub provider: SecretProvider,
    pub display_name: String,
    pub config: SecretProviderConfigPayload,
    pub sample_count: usize,
    pub samples: Vec<SecretProviderConfigDiscoverySample>,
    pub signals: SecretProviderConfigDiscoverySignal,
    pub warnings: Vec<String>,
}

/// Discovery preview request (scan external provider for secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoveryPreviewRequest {
    pub provider: SecretProvider,
    pub config: SecretProviderConfigPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    100
}

/// Discovery preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoveryPreviewResult {
    pub provider: SecretProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    pub sampled_secret_count: usize,
    pub skipped_foreign_paperclip_sample_count: usize,
    pub candidates: Vec<SecretProviderConfigDiscoveryCandidate>,
    pub warnings: Vec<String>,
}
