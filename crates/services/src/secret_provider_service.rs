use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::secret_provider::{
    SecretProviderConfig, ProviderHealthCheck, ProviderHealthStatus,
    SecretDiscoveryPreview, SecretDiscoveryCandidate,
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
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn list_configs(&self, company_id: Uuid) -> ServiceResult<Vec<SecretProviderConfig>> {
        self.repository
            .list_configs(company_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_config(&self, config_id: Uuid) -> ServiceResult<Option<SecretProviderConfig>> {
        self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
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
            .map_err(|e| ServiceError::Database(e.to_string()))?
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
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn delete_config(&self, config_id: Uuid) -> ServiceResult<()> {
        self.repository
            .delete_config(config_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn set_default_provider(&self, company_id: Uuid, config_id: Uuid) -> ServiceResult<()> {
        self.repository
            .set_default(company_id, config_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn discover_secrets_preview(
        &self,
        config_id: Uuid,
        max_results: Option<usize>,
        next_token: Option<String>,
    ) -> ServiceResult<SecretDiscoveryPreview> {
        let config = self.repository
            .get_config(config_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))?
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
            .map_err(|e| ServiceError::Database(e.to_string()))?
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
            .map_err(|e| ServiceError::Database(e.to_string()))?;

        Ok(health_check)
    }
}
