use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::secret_provider::{SecretProviderConfig, ProviderHealthCheck};

#[async_trait]
pub trait SecretProviderConfigRepository: Send + Sync {
    async fn create_config(&self, config: SecretProviderConfig) -> RepositoryResult<SecretProviderConfig>;
    async fn list_configs(&self, company_id: Uuid) -> RepositoryResult<Vec<SecretProviderConfig>>;
    async fn get_config(&self, config_id: Uuid) -> RepositoryResult<Option<SecretProviderConfig>>;
    async fn update_config(&self, config: SecretProviderConfig) -> RepositoryResult<SecretProviderConfig>;
    async fn delete_config(&self, config_id: Uuid) -> RepositoryResult<()>;
    async fn set_default(&self, company_id: Uuid, config_id: Uuid) -> RepositoryResult<()>;
    async fn get_default_config(&self, company_id: Uuid) -> RepositoryResult<Option<SecretProviderConfig>>;
    async fn record_health_check(&self, health_check: ProviderHealthCheck) -> RepositoryResult<()>;
}

pub struct PostgresSecretProviderConfigRepository {
    pool: PgPool,
}

impl PostgresSecretProviderConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecretProviderConfigRepository for PostgresSecretProviderConfigRepository {
    async fn create_config(&self, config: SecretProviderConfig) -> RepositoryResult<SecretProviderConfig> {
        sqlx::query(
            r#"INSERT INTO secret_provider_configs
               (id, company_id, provider_type, config, is_default, enabled, created_at, updated_at, created_by_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(config.id)
        .bind(config.company_id)
        .bind(&config.provider_type)
        .bind(&config.config)
        .bind(config.is_default)
        .bind(config.enabled)
        .bind(config.created_at)
        .bind(config.updated_at)
        .bind(config.created_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(config)
    }

    async fn list_configs(&self, company_id: Uuid) -> RepositoryResult<Vec<SecretProviderConfig>> {
        let configs = sqlx::query_as::<_, SecretProviderConfig>(
            r#"SELECT id, company_id, provider_type, config, is_default, enabled,
                      created_at, updated_at, created_by_user_id
               FROM secret_provider_configs
               WHERE company_id = $1
               ORDER BY is_default DESC, created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(configs)
    }

    async fn get_config(&self, config_id: Uuid) -> RepositoryResult<Option<SecretProviderConfig>> {
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"SELECT id, company_id, provider_type, config, is_default, enabled,
                      created_at, updated_at, created_by_user_id
               FROM secret_provider_configs
               WHERE id = $1"#
        )
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(config)
    }

    async fn update_config(&self, config: SecretProviderConfig) -> RepositoryResult<SecretProviderConfig> {
        sqlx::query(
            r#"UPDATE secret_provider_configs
               SET provider_type = $2, config = $3, is_default = $4, enabled = $5, updated_at = $6
               WHERE id = $1"#
        )
        .bind(config.id)
        .bind(&config.provider_type)
        .bind(&config.config)
        .bind(config.is_default)
        .bind(config.enabled)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(config)
    }

    async fn delete_config(&self, config_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM secret_provider_configs WHERE id = $1")
            .bind(config_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_default(&self, company_id: Uuid, config_id: Uuid) -> RepositoryResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("UPDATE secret_provider_configs SET is_default = false WHERE company_id = $1")
            .bind(company_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("UPDATE secret_provider_configs SET is_default = true WHERE id = $1")
            .bind(config_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_default_config(&self, company_id: Uuid) -> RepositoryResult<Option<SecretProviderConfig>> {
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"SELECT id, company_id, provider_type, config, is_default, enabled,
                      created_at, updated_at, created_by_user_id
               FROM secret_provider_configs
               WHERE company_id = $1 AND is_default = true"#
        )
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(config)
    }

    async fn record_health_check(&self, health_check: ProviderHealthCheck) -> RepositoryResult<()> {
        sqlx::query(
            r#"INSERT INTO provider_health_checks
               (id, provider_config_id, status, latency_ms, error_message, checked_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#
        )
        .bind(Uuid::new_v4())
        .bind(health_check.provider_config_id)
        .bind(&format!("{:?}", health_check.status))
        .bind(health_check.latency_ms.map(|v| v as i64))
        .bind(&health_check.error_message)
        .bind(health_check.checked_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
