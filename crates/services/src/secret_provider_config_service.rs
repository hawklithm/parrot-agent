use async_trait::async_trait;
use models::{
    CompanySecretProviderConfig, CreateSecretProviderConfigRequest,
    SecretProviderConfigDiscoveryPreviewRequest, SecretProviderConfigDiscoveryPreviewResult,
    SecretProviderConfigHealthResponse, UpdateSecretProviderConfigRequest,
};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::ServiceResult;

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

/// Mock implementation for testing
pub struct MockSecretProviderConfigService;

#[async_trait]
impl SecretProviderConfigService for MockSecretProviderConfigService {
    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<CompanySecretProviderConfig>> {
        use chrono::Utc;
        use models::{
            AwsSecretsManagerProviderConfig, LocalEncryptedProviderConfig, SecretProvider,
            SecretProviderConfigHealthDetails, SecretProviderConfigHealthStatus,
            SecretProviderConfigPayload, SecretProviderConfigStatus,
        };

        Ok(vec![
            CompanySecretProviderConfig {
                id: Uuid::new_v4(),
                company_id,
                provider: SecretProvider::LocalEncrypted,
                display_name: "Local Encrypted Storage".to_string(),
                status: SecretProviderConfigStatus::Ready,
                is_default: true,
                config: SecretProviderConfigPayload::LocalEncrypted(LocalEncryptedProviderConfig {
                    backup_reminder_acknowledged: Some(true),
                }),
                health_status: Some(SecretProviderConfigHealthStatus::Ready),
                health_checked_at: Some(Utc::now()),
                health_message: Some("Operational".to_string()),
                health_details: Some(SecretProviderConfigHealthDetails {
                    code: "healthy".to_string(),
                    message: "Local storage is operational".to_string(),
                    missing_fields: None,
                    guidance: None,
                }),
                disabled_at: None,
                created_by_agent_id: None,
                created_by_user_id: Some(Uuid::new_v4()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            CompanySecretProviderConfig {
                id: Uuid::new_v4(),
                company_id,
                provider: SecretProvider::AwsSecretsManager,
                display_name: "AWS Prod (us-east-1)".to_string(),
                status: SecretProviderConfigStatus::Ready,
                is_default: false,
                config: SecretProviderConfigPayload::AwsSecretsManager(AwsSecretsManagerProviderConfig {
                    region: "us-east-1".to_string(),
                    endpoint: "https://secretsmanager.us-east-1.amazonaws.com".to_string(),
                    deployment_id: "prod-us-east-1".to_string(),
                    prefix: "paperclip/".to_string(),
                    kms_key_id: Some("arn:aws:kms:us-east-1:123456789012:key/abcd-1234".to_string()),
                    environment_tag: "production".to_string(),
                    provider_owner_tag: "paperclip".to_string(),
                    delete_recovery_window_days: 30,
                }),
                health_status: Some(SecretProviderConfigHealthStatus::Ready),
                health_checked_at: Some(Utc::now()),
                health_message: Some("Connected successfully".to_string()),
                health_details: Some(SecretProviderConfigHealthDetails {
                    code: "healthy".to_string(),
                    message: "AWS Secrets Manager is reachable".to_string(),
                    missing_fields: None,
                    guidance: None,
                }),
                disabled_at: None,
                created_by_agent_id: None,
                created_by_user_id: Some(Uuid::new_v4()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ])
    }

    async fn discovery_preview(
        &self,
        _company_id: Uuid,
        request: SecretProviderConfigDiscoveryPreviewRequest,
    ) -> ServiceResult<SecretProviderConfigDiscoveryPreviewResult> {
        use models::{
            SecretProviderConfigDiscoveryCandidate, SecretProviderConfigDiscoverySignal,
            SecretProviderConfigDiscoverySample,
        };

        Ok(SecretProviderConfigDiscoveryPreviewResult {
            provider: request.provider.clone(),
            next_token: None,
            sampled_secret_count: 15,
            skipped_foreign_paperclip_sample_count: 2,
            candidates: vec![SecretProviderConfigDiscoveryCandidate {
                provider: request.provider.clone(),
                display_name: "Discovered AWS Configuration".to_string(),
                config: request.config,
                sample_count: 15,
                samples: vec![
                    SecretProviderConfigDiscoverySample {
                        name: "paperclip/prod/database-url".to_string(),
                        has_kms_key: true,
                        tag_keys: vec!["environment".to_string(), "owner".to_string()],
                    },
                    SecretProviderConfigDiscoverySample {
                        name: "paperclip/prod/api-key".to_string(),
                        has_kms_key: true,
                        tag_keys: vec!["environment".to_string(), "owner".to_string()],
                    },
                ],
                signals: SecretProviderConfigDiscoverySignal {
                    namespace: Some("paperclip".to_string()),
                    secret_name_prefix: Some("paperclip/prod/".to_string()),
                    environment_tag: Some("production".to_string()),
                    owner_tag: Some("paperclip".to_string()),
                    kms_key_id: Some("arn:aws:kms:us-east-1:123456789012:key/abcd-1234".to_string()),
                    has_kms_key: true,
                    sample_count: 15,
                    paperclip_managed_sample_count: 13,
                    skipped_foreign_paperclip_sample_count: 2,
                },
                warnings: vec![
                    "2 secrets skipped (owned by different Paperclip instance)".to_string(),
                ],
            }],
            warnings: vec![],
        })
    }

    async fn create_config(
        &self,
        company_id: Uuid,
        request: CreateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        use chrono::Utc;
        use models::SecretProviderConfigStatus;

        Ok(CompanySecretProviderConfig {
            id: Uuid::new_v4(),
            company_id,
            provider: request.provider,
            display_name: request.display_name,
            status: SecretProviderConfigStatus::Ready,
            is_default: request.set_as_default,
            config: request.config,
            health_status: None,
            health_checked_at: None,
            health_message: None,
            health_details: None,
            disabled_at: None,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn get_config(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        use chrono::Utc;
        use models::{
            LocalEncryptedProviderConfig, SecretProvider, SecretProviderConfigPayload,
            SecretProviderConfigStatus,
        };

        Ok(CompanySecretProviderConfig {
            id: config_id,
            company_id: Uuid::new_v4(),
            provider: SecretProvider::LocalEncrypted,
            display_name: "Local Encrypted Storage".to_string(),
            status: SecretProviderConfigStatus::Ready,
            is_default: true,
            config: SecretProviderConfigPayload::LocalEncrypted(LocalEncryptedProviderConfig {
                backup_reminder_acknowledged: Some(true),
            }),
            health_status: None,
            health_checked_at: None,
            health_message: None,
            health_details: None,
            disabled_at: None,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn update_config(
        &self,
        config_id: Uuid,
        request: UpdateSecretProviderConfigRequest,
    ) -> ServiceResult<CompanySecretProviderConfig> {
        use chrono::Utc;
        use models::{
            LocalEncryptedProviderConfig, SecretProvider, SecretProviderConfigPayload,
            SecretProviderConfigStatus,
        };

        Ok(CompanySecretProviderConfig {
            id: config_id,
            company_id: Uuid::new_v4(),
            provider: SecretProvider::LocalEncrypted,
            display_name: request.display_name.unwrap_or("Updated Config".to_string()),
            status: request.status.unwrap_or(SecretProviderConfigStatus::Ready),
            is_default: false,
            config: request
                .config
                .unwrap_or(SecretProviderConfigPayload::LocalEncrypted(
                    LocalEncryptedProviderConfig {
                        backup_reminder_acknowledged: Some(true),
                    },
                )),
            health_status: None,
            health_checked_at: None,
            health_message: None,
            health_details: None,
            disabled_at: None,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn delete_config(&self, _config_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn set_default(&self, config_id: Uuid) -> ServiceResult<CompanySecretProviderConfig> {
        use chrono::Utc;
        use models::{
            LocalEncryptedProviderConfig, SecretProvider, SecretProviderConfigPayload,
            SecretProviderConfigStatus,
        };

        Ok(CompanySecretProviderConfig {
            id: config_id,
            company_id: Uuid::new_v4(),
            provider: SecretProvider::LocalEncrypted,
            display_name: "Local Encrypted Storage".to_string(),
            status: SecretProviderConfigStatus::Ready,
            is_default: true, // Set as default
            config: SecretProviderConfigPayload::LocalEncrypted(LocalEncryptedProviderConfig {
                backup_reminder_acknowledged: Some(true),
            }),
            health_status: None,
            health_checked_at: None,
            health_message: None,
            health_details: None,
            disabled_at: None,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn health_check(&self, config_id: Uuid) -> ServiceResult<SecretProviderConfigHealthResponse> {
        use chrono::Utc;
        use models::{
            SecretProvider, SecretProviderConfigHealthDetails, SecretProviderConfigHealthStatus,
        };

        Ok(SecretProviderConfigHealthResponse {
            config_id,
            provider: SecretProvider::LocalEncrypted,
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
        use chrono::Utc;
        use models::{
            SecretProvider, SecretProviderConfigHealthDetails, SecretProviderConfigHealthStatus,
        };

        Ok(vec![
            SecretProviderConfigHealthResponse {
                config_id: Uuid::new_v4(),
                provider: SecretProvider::LocalEncrypted,
                status: SecretProviderConfigHealthStatus::Ready,
                message: "Operational".to_string(),
                details: SecretProviderConfigHealthDetails {
                    code: "healthy".to_string(),
                    message: "Local storage is operational".to_string(),
                    missing_fields: None,
                    guidance: None,
                },
                checked_at: Utc::now(),
            },
            SecretProviderConfigHealthResponse {
                config_id: Uuid::new_v4(),
                provider: SecretProvider::AwsSecretsManager,
                status: SecretProviderConfigHealthStatus::Ready,
                message: "Connected successfully".to_string(),
                details: SecretProviderConfigHealthDetails {
                    code: "healthy".to_string(),
                    message: "AWS Secrets Manager is reachable".to_string(),
                    missing_fields: None,
                    guidance: None,
                },
                checked_at: Utc::now(),
            },
        ])
    }
}
