use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::secret_provider::{
    SecretProviderConfig, ProviderHealthCheck, ProviderHealthStatus,
    SecretDiscoveryPreview, SecretDiscoveryCandidate,
    RemoteImportPreview, RemoteImportResult, ConflictResolution,
};
use repositories::secret_provider_config_repository::SecretProviderConfigRepository;

#[async_trait]
pub trait SecretProviderConfigService: Send + Sync {
    async fn create_config(
        &self,
        company_id: Uuid,
        provider_type: String,
        config: serde_json::Value,
        created_by_user_id: Uuid,
    ) -> ServiceResult<SecretProviderConfig>;

    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<SecretProviderConfig>>;
    async fn get_config(&self, config_id: Uuid) -> ServiceResult<Option<SecretProviderConfig>>;

    async fn update_config(
        &self,
        config_id: Uuid,
        provider_type: Option<String>,
        config: Option<serde_json::Value>,
        enabled: Option<bool>,
    ) -> ServiceResult<SecretProviderConfig>;

    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<()>;
    async fn set_default_provider(&self, company_id: Uuid, config_id: Uuid) -> ServiceResult<()>;

    async fn discover_secrets_preview(
        &self,
        config_id: Uuid,
        max_results: Option<usize>,
        next_token: Option<String>,
    ) -> ServiceResult<SecretDiscoveryPreview>;

    async fn test_provider_health(&self, config_id: Uuid) -> ServiceResult<ProviderHealthCheck>;

    /// Preview secrets from remote provider before importing
    async fn remote_import_preview(
        &self,
        company_id: Uuid,
        config_id: Uuid,
        filters: Option<serde_json::Value>,
    ) -> ServiceResult<RemoteImportPreview>;

    /// Execute remote import of secrets from external provider
    async fn remote_import_execute(
        &self,
        company_id: Uuid,
        config_id: Uuid,
        secret_keys: Vec<String>,
        conflict_resolution: ConflictResolution,
        created_by_user_id: Uuid,
    ) -> ServiceResult<RemoteImportResult>;
}

pub struct SecretProviderConfigServiceImpl {
    repository: Arc<dyn SecretProviderConfigRepository>,
}

impl SecretProviderConfigServiceImpl {
    pub fn new(repository: Arc<dyn SecretProviderConfigRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl SecretProviderConfigService for SecretProviderConfigServiceImpl {
    async fn create_config(
        &self,
        company_id: Uuid,
        provider_type: String,
        config: serde_json::Value,
        created_by_user_id: Uuid,
    ) -> ServiceResult<SecretProviderConfig> {
        let provider_config = SecretProviderConfig::new(
            company_id,
            provider_type,
            config,
            created_by_user_id,
        );

        self.repository
            .create_config(provider_config)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<SecretProviderConfig>> {
        self.repository
            .list_configs(company_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_config(&self, config_id: Uuid) -> ServiceResult<Option<SecretProviderConfig>> {
        self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn update_config(
        &self,
        config_id: Uuid,
        provider_type: Option<String>,
        config: Option<serde_json::Value>,
        enabled: Option<bool>,
    ) -> ServiceResult<SecretProviderConfig> {
        let mut provider_config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Config {} not found", config_id)))?;

        if let Some(pt) = provider_type {
            provider_config.provider_type = pt;
        }
        if let Some(c) = config {
            provider_config.config = sqlx::types::Json(c);
        }
        if let Some(e) = enabled {
            provider_config.enabled = e;
        }

        self.repository
            .update_config(provider_config)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<()> {
        self.repository
            .delete_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn set_default_provider(&self, company_id: Uuid, config_id: Uuid) -> ServiceResult<()> {
        self.repository
            .set_default(company_id, config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn discover_secrets_preview(
        &self,
        config_id: Uuid,
        _max_results: Option<usize>,
        _next_token: Option<String>,
    ) -> ServiceResult<SecretDiscoveryPreview> {
        let config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Config {} not found", config_id)))?;

        // TODO: Implement actual provider-specific discovery logic
        // For now, return empty preview
        Ok(SecretDiscoveryPreview {
            provider: config.provider_type.clone(),
            next_token: None,
            sampled_secret_count: 0,
            skipped_foreign_paperclip_sample_count: 0,
            candidates: vec![],
            warnings: vec![],
        })
    }

    async fn test_provider_health(&self, config_id: Uuid) -> ServiceResult<ProviderHealthCheck> {
        let start = Instant::now();

        let config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Config {} not found", config_id)))?;

        // TODO: Implement actual provider-specific health check
        let latency = start.elapsed().as_millis() as u64;
        let status = if config.enabled {
            ProviderHealthStatus::Healthy
        } else {
            ProviderHealthStatus::Unhealthy
        };

        let health_check = ProviderHealthCheck {
            provider_config_id: config_id,
            status: status.clone(),
            latency_ms: Some(latency),
            error_message: None,
            checked_at: chrono::Utc::now(),
        };

        self.repository
            .record_health_check(health_check.clone())
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        Ok(health_check)
    }

    async fn remote_import_preview(
        &self,
        company_id: Uuid,
        config_id: Uuid,
        _filters: Option<serde_json::Value>,
    ) -> ServiceResult<RemoteImportPreview> {
        let config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Config {} not found", config_id)))?;

        if config.company_id != company_id {
            return Err(ServiceError::Unauthorized(
                "Config does not belong to this company".to_string()
            ));
        }

        // TODO: Implement actual provider-specific discovery with filters
        // For now, return mock preview data
        let candidates = vec![
            SecretDiscoveryCandidate {
                external_ref: "aws://secrets-manager/prod/db-password".to_string(),
                suggested_name: "db_password".to_string(),
                tags: vec![("env".to_string(), "prod".to_string())],
                created_at: Some(chrono::Utc::now()),
                last_modified_at: Some(chrono::Utc::now()),
                conflict: None,
            }
        ];

        Ok(RemoteImportPreview {
            provider: config.provider_type.clone(),
            total_secrets: candidates.len(),
            new_secrets: candidates.len(),
            conflicting_secrets: 0,
            candidates,
            warnings: vec![],
        })
    }

    async fn remote_import_execute(
        &self,
        company_id: Uuid,
        config_id: Uuid,
        secret_keys: Vec<String>,
        _conflict_resolution: ConflictResolution,
        _created_by_user_id: Uuid,
    ) -> ServiceResult<RemoteImportResult> {
        let config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Config {} not found", config_id)))?;

        if config.company_id != company_id {
            return Err(ServiceError::Unauthorized(
                "Config does not belong to this company".to_string()
            ));
        }

        // TODO: Implement actual provider-specific import logic
        // For now, simulate successful import
        let imported_keys = secret_keys.clone();

        Ok(RemoteImportResult {
            imported_count: imported_keys.len(),
            skipped_count: 0,
            failed_count: 0,
            imported_keys,
            skipped_keys: vec![],
            errors: vec![],
        })
    }
}
