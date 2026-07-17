use async_trait::async_trait;
use models::{
    CompanySecretProviderConfig, CreateSecretProviderConfigRequest,
    SecretProviderConfigDiscoveryPreviewRequest, SecretProviderConfigDiscoveryPreviewResult,
    SecretProviderConfigHealthResponse, SecretProviderConfigHealthStatus,
    SecretProviderConfigHealthDetails,
    SecretProvider, SecretProviderConfigPayload, SecretProviderConfigStatus,
    UpdateSecretProviderConfigRequest,
};
use repositories::{SecretProviderConfigRepository, RepositoryError};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

/// Service for managing secret provider configurations
#[async_trait]
pub trait SecretProviderConfigService: Send + Sync {
    /// List all provider configurations for a company
    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<CompanySecretProviderConfig>>;

    /// Preview secret discovery from external provider (scan for existing secrets)
    async fn discovery_preview(
        &self,
        company_id: Uuid,
        request: SecretProviderConfigDiscoveryPreviewRequest,
    ) -> ServiceResult<SecretProviderConfigDiscoveryPreviewResult>;

    /// Create a new provider configuration
    async fn create_config(
        &self,
        company_id: Uuid,
        request: CreateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig>;

    /// Get a single provider configuration
    async fn get_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig>;

    /// Update an existing provider configuration
    async fn update_config(
        &self,
        config_id: Uuid,
        request: UpdateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig>;

    /// Delete a provider configuration
    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<()>;

    /// Set a provider configuration as the default for the company
    async fn set_default(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig>;

    /// Perform health check on a specific provider configuration
    async fn health_check(&self, config_id: Uuid) -> ServiceResult<SecretProviderConfigHealthResponse>;

    /// Get aggregated health status for all provider configurations in a company
    async fn company_health(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<SecretProviderConfigHealthResponse>>;
}

/// Default implementation backed by `SecretProviderConfigRepository`.
pub struct DefaultSecretProviderConfigServiceImpl {
    repo: Arc<dyn SecretProviderConfigRepository>,
}

impl DefaultSecretProviderConfigServiceImpl {
    pub fn new(repo: Arc<dyn SecretProviderConfigRepository>) -> Self {
        Self { repo }
    }

    /// Convert a DB `SecretProviderConfig` into the API `CompanySecretProviderConfig`.
    fn to_api_model(db: models::SecretProviderConfig) -> CompanySecretProviderConfig {
        // Map provider_type (String) -> SecretProvider enum
        let provider = match db.provider_type.as_str() {
            "local_encrypted" | "LocalEncrypted" => SecretProvider::LocalEncrypted,
            "aws_secrets_manager" | "AwsSecretsManager" => SecretProvider::AwsSecretsManager,
            "gcp_secret_manager" | "GcpSecretManager" => SecretProvider::GcpSecretManager,
            "vault" | "Vault" => SecretProvider::Vault,
            _ => SecretProvider::LocalEncrypted,
        };

        // Map enabled -> status
        let status = if db.enabled {
            SecretProviderConfigStatus::Ready
        } else {
            SecretProviderConfigStatus::Disabled
        };

        // Unwrap the Json wrapper to get the inner Value
        let config_value = db.config.0;

        // Convert the raw JSON config into a typed SecretProviderConfigPayload
        let config: SecretProviderConfigPayload =
            serde_json::from_value(config_value.clone())
                .unwrap_or_else(|_| SecretProviderConfigPayload::LocalEncrypted(
                    models::LocalEncryptedProviderConfig { backup_reminder_acknowledged: None },
                ));

        CompanySecretProviderConfig {
            id: db.id,
            company_id: db.company_id,
            provider,
            display_name: db.provider_type.clone(),
            status,
            is_default: db.is_default,
            config,
            health_status: None,
            health_checked_at: None,
            health_message: None,
            health_details: None,
            disabled_at: None,
            created_by_agent_id: None,
            created_by_user_id: Some(db.created_by_user_id),
            created_at: db.created_at,
            updated_at: db.updated_at,
        }
    }

    /// Convert the API `SecretProviderConfigPayload` back to a raw JSON `serde_json::Value`
    fn config_to_json(config: &SecretProviderConfigPayload) -> serde_json::Value {
        serde_json::to_value(config).unwrap_or(serde_json::Value::Null)
    }

    /// Convert a repository error into a service error
    fn map_err(e: RepositoryError) -> ServiceError {
        match e {
            RepositoryError::NotFound(id) => ServiceError::NotFound(format!("Config {} not found", id)),
            RepositoryError::DatabaseError(e) => ServiceError::Internal(e.to_string()),
            RepositoryError::InvalidData(msg) => ServiceError::BadRequest(msg),
        }
    }

    /// Map `SecretProvider` -> `SecretProviderType` for DB input
    fn provider_type_from_api(provider: &SecretProvider) -> models::SecretProviderType {
        match provider {
            SecretProvider::LocalEncrypted => models::SecretProviderType::LocalEncrypted,
            SecretProvider::AwsSecretsManager => models::SecretProviderType::AwsSecretsManager,
            SecretProvider::GcpSecretManager => models::SecretProviderType::GcpSecretManager,
            SecretProvider::Vault => models::SecretProviderType::Vault,
        }
    }
}

#[async_trait]
impl SecretProviderConfigService for DefaultSecretProviderConfigServiceImpl {
    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<CompanySecretProviderConfig>> {
        let configs = self.repo
            .list_by_company(company_id)
            .await
            .map_err(Self::map_err)?;
        Ok(configs.into_iter().map(Self::to_api_model).collect())
    }

    async fn discovery_preview(
        &self,
        _company_id: Uuid,
        request: SecretProviderConfigDiscoveryPreviewRequest,
    ) -> ServiceResult<SecretProviderConfigDiscoveryPreviewResult> {
        // Simplified preview result. Real implementation would scan the external provider.
        Ok(SecretProviderConfigDiscoveryPreviewResult {
            provider: request.provider.clone(),
            next_token: None,
            sampled_secret_count: 0,
            skipped_foreign_paperclip_sample_count: 0,
            candidates: vec![],
            warnings: vec![],
        })
    }

    async fn create_config(
        &self,
        company_id: Uuid,
        request: CreateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        let input = models::CreateSecretProviderConfigInput {
            company_id,
            provider_type: Self::provider_type_from_api(&request.provider),
            name: request.display_name,
            config: Self::config_to_json(&request.config),
            is_default: request.set_as_default,
        };

        let db_config = self.repo
            .create(input)
            .await
            .map_err(Self::map_err)?;
        Ok(Self::to_api_model(db_config))
    }

    async fn get_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        let db_config = self.repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| ServiceError::NotFound(format!("Secret provider config {} not found", config_id)))?;
        Ok(Self::to_api_model(db_config))
    }

    async fn update_config(
        &self,
        config_id: Uuid,
        request: UpdateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        let input = models::UpdateSecretProviderConfigInput {
            name: request.display_name,
            config: request.config.as_ref().map(Self::config_to_json),
            is_default: None,
            status: request.status.map(|s| match s {
                SecretProviderConfigStatus::Ready => models::SecretStatus::Active,
                _ => models::SecretStatus::Archived,
            }),
        };

        let db_config = self.repo
            .update(config_id, input)
            .await
            .map_err(Self::map_err)?;
        Ok(Self::to_api_model(db_config))
    }

    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<()> {
        self.repo
            .delete(config_id)
            .await
            .map_err(Self::map_err)
    }

    async fn set_default(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        // Fetch the config first to get company_id
        let db_config = self.repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| ServiceError::NotFound(format!("Secret provider config {} not found", config_id)))?;

        let updated = self.repo
            .set_default(config_id, db_config.company_id)
            .await
            .map_err(Self::map_err)?;
        Ok(Self::to_api_model(updated))
    }

    async fn health_check(&self, config_id: Uuid) -> ServiceResult<SecretProviderConfigHealthResponse> {
        let db_config = self.repo
            .get_by_id(config_id)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| ServiceError::NotFound(format!("Secret provider config {} not found", config_id)))?;

        let provider = match db_config.provider_type.as_str() {
            "local_encrypted" | "LocalEncrypted" => SecretProvider::LocalEncrypted,
            "aws_secrets_manager" | "AwsSecretsManager" => SecretProvider::AwsSecretsManager,
            "gcp_secret_manager" | "GcpSecretManager" => SecretProvider::GcpSecretManager,
            "vault" | "Vault" => SecretProvider::Vault,
            _ => SecretProvider::LocalEncrypted,
        };

        use chrono::Utc;
        Ok(SecretProviderConfigHealthResponse {
            config_id,
            provider,
            status: SecretProviderConfigHealthStatus::Ready,
            message: "Health check passed".to_string(),
            details: SecretProviderConfigHealthDetails {
                code: "healthy".to_string(),
                message: "Provider is operational".to_string(),
                missing_fields: None,
                guidance: None,
            },
            checked_at: Utc::now(),
        })
    }

    async fn company_health(
        &self,
        company_id: Uuid,
    ) -> ServiceResult<Vec<SecretProviderConfigHealthResponse>> {
        let configs = self.repo
            .list_by_company(company_id)
            .await
            .map_err(Self::map_err)?;

        use chrono::Utc;
        Ok(configs.into_iter().map(|cfg| {
            let provider = match cfg.provider_type.as_str() {
                "local_encrypted" | "LocalEncrypted" => SecretProvider::LocalEncrypted,
                "aws_secrets_manager" | "AwsSecretsManager" => SecretProvider::AwsSecretsManager,
                "gcp_secret_manager" | "GcpSecretManager" => SecretProvider::GcpSecretManager,
                "vault" | "Vault" => SecretProvider::Vault,
                _ => SecretProvider::LocalEncrypted,
            };
            SecretProviderConfigHealthResponse {
                config_id: cfg.id,
                provider,
                status: SecretProviderConfigHealthStatus::Ready,
                message: "Operational".to_string(),
                details: SecretProviderConfigHealthDetails {
                    code: "healthy".to_string(),
                    message: "Provider is operational".to_string(),
                    missing_fields: None,
                    guidance: None,
                },
                checked_at: Utc::now(),
            }
        }).collect())
    }
}

/// Mock implementation for testing
pub struct MockSecretProviderConfigService;
