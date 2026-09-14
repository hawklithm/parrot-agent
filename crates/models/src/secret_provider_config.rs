use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Secret provider identifier (`SECRET_PROVIDERS`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProvider {
    LocalEncrypted,
    AwsSecretsManager,
    GcpSecretManager,
    Vault,
}

impl SecretProvider {
    /// Canonical wire/DB string. Matches the `provider` column values.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LocalEncrypted => "local_encrypted",
            Self::AwsSecretsManager => "aws_secrets_manager",
            Self::GcpSecretManager => "gcp_secret_manager",
            Self::Vault => "vault",
        }
    }

    /// Registry order, mirroring Paperclip's `provider-registry.ts`.
    pub const ALL: [SecretProvider; 4] = [
        SecretProvider::LocalEncrypted,
        SecretProvider::AwsSecretsManager,
        SecretProvider::GcpSecretManager,
        SecretProvider::Vault,
    ];
}

impl std::fmt::Display for SecretProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provider configuration status (`SECRET_PROVIDER_CONFIG_STATUSES`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProviderConfigStatus {
    Ready,
    Warning,
    ComingSoon,
    Disabled,
}

impl SecretProviderConfigStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Warning => "warning",
            Self::ComingSoon => "coming_soon",
            Self::Disabled => "disabled",
        }
    }

    /// A vault in one of these states can never be the company default.
    pub fn blocks_default(&self) -> bool {
        matches!(self, Self::ComingSoon | Self::Disabled)
    }
}

/// Per-config health status (`SECRET_PROVIDER_CONFIG_HEALTH_STATUSES`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretProviderConfigHealthStatus {
    Ready,
    Warning,
    Error,
    ComingSoon,
    Disabled,
}

/// Health status reported by a provider module (not a stored config).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SecretProviderHealthStatus {
    Ok,
    Warn,
    Error,
}

/// Deployment-level health check for one provider module.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderHealthCheck {
    pub provider: SecretProvider,
    pub status: SecretProviderHealthStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup_guidance: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// `GET /companies/:companyId/secret-providers/health` response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderHealthResponse {
    pub providers: Vec<SecretProviderHealthCheck>,
}

/// Static capability descriptor for one provider module.
///
/// `GET /companies/:companyId/secret-providers` returns these verbatim, so the
/// field set mirrors Paperclip's `SecretProviderDescriptor`
/// (`packages/shared/src/types/secrets.ts`). `configured` is not stored state:
/// it reports whether the *deployment* is ready to serve the provider, which is
/// why only `aws_secrets_manager` computes it from the environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderDescriptor {
    pub id: SecretProvider,
    pub label: String,
    pub requires_external_ref: bool,
    pub supports_managed_values: bool,
    pub supports_external_references: bool,
    pub supports_external_value_writes: bool,
    pub configured: bool,
}

/// Health check details block on a stored configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigHealthDetails {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<String>,
}

/// Stored provider configuration as returned by the API.
///
/// `config` is echoed back exactly as persisted; it is not re-typed into a
/// provider-specific struct so that unknown-but-permitted keys survive a
/// read/write round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySecretProviderConfig {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider: SecretProvider,
    pub display_name: String,
    pub status: SecretProviderConfigStatus,
    pub is_default: bool,
    pub config: serde_json::Value,
    pub health_status: Option<SecretProviderConfigHealthStatus>,
    pub health_checked_at: Option<DateTime<Utc>>,
    pub health_message: Option<String>,
    pub health_details: Option<SecretProviderConfigHealthDetails>,
    pub disabled_at: Option<DateTime<Utc>>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// `POST /companies/:companyId/secret-provider-configs` request body.
///
/// Field names mirror Paperclip's `createSecretProviderConfigSchema`
/// verbatim — notably `isDefault`, not `setAsDefault`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSecretProviderConfigRequest {
    pub provider: SecretProvider,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SecretProviderConfigStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    /// Paperclip's schema applies `.default({})`, so an absent `config` must
    /// behave exactly like an explicit `{}`. `serde_json::Value`'s own default
    /// is `Null`, which no provider schema accepts.
    #[serde(default = "empty_object")]
    pub config: serde_json::Value,
}

/// Serde default for a provider `config` field: Paperclip's `.default({})`.
pub fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// `PATCH /secret-provider-configs/:id` request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSecretProviderConfigRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SecretProviderConfigStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// `POST /secret-provider-configs/:id/health` response body.
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
    pub namespace: Option<String>,
    pub secret_name_prefix: Option<String>,
    pub environment_tag: Option<String>,
    pub owner_tag: Option<String>,
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
    pub config: serde_json::Value,
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
    /// Same `.default({})` contract as the create body.
    #[serde(default = "empty_object")]
    pub config: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<usize>,
}

/// Discovery preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretProviderConfigDiscoveryPreviewResult {
    pub provider: SecretProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    pub sampled_secret_count: usize,
    pub skipped_foreign_paperclip_sample_count: usize,
    pub candidates: Vec<SecretProviderConfigDiscoveryCandidate>,
    pub warnings: Vec<String>,
}
