use async_trait::async_trait;
use models::{
    SecretProviderConfig, CreateSecretProviderConfigInput, UpdateSecretProviderConfigInput,
    UserSecretDefinition, CreateUserSecretDefinitionInput, UpdateUserSecretDefinitionInput,
};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

/// Columns of `company_secret_provider_configs`, in the order every query in
/// this module selects them. `SELECT *` is avoided so a future column addition
/// cannot silently shift the `FromRow` mapping.
const PROVIDER_CONFIG_COLUMNS: &str = "id, company_id, provider, display_name, status, is_default, \
     config, health_status, health_checked_at, health_message, health_details, disabled_at, \
     created_by_agent_id, created_by_user_id, created_at, updated_at";

#[async_trait]
pub trait SecretProviderConfigRepository: Send + Sync {
    async fn create(&self, input: CreateSecretProviderConfigInput) -> Result<SecretProviderConfig, RepositoryError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError>;
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<SecretProviderConfig>, RepositoryError>;
    async fn update(&self, id: Uuid, input: UpdateSecretProviderConfigInput) -> Result<Option<SecretProviderConfig>, RepositoryError>;
    /// Hard-delete and return the removed row (Paperclip `removeProviderConfig`).
    async fn delete(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError>;
    /// Clear the default flag for every config sharing `(company_id, provider)`,
    /// then set it on `id`. Returns `None` when the row is missing or is in a
    /// status that forbids being default.
    async fn set_default(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError>;
    /// Persist the outcome of a health check on a stored configuration.
    async fn record_health(
        &self,
        id: Uuid,
        status: &str,
        message: &str,
        details: &serde_json::Value,
        checked_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError>;
}

pub struct PgSecretProviderConfigRepository {
    pool: PgPool,
}

impl PgSecretProviderConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecretProviderConfigRepository for PgSecretProviderConfigRepository {
    async fn create(&self, input: CreateSecretProviderConfigInput) -> Result<SecretProviderConfig, RepositoryError> {
        let sql = format!(
            "INSERT INTO company_secret_provider_configs (
                 company_id, provider, display_name, status, is_default, config,
                 disabled_at, created_by_agent_id, created_by_user_id
             )
             VALUES ($1, $2, $3, $4, $5, $6,
                     CASE WHEN $4 = 'disabled' THEN NOW() ELSE NULL END, $7, $8)
             RETURNING {PROVIDER_CONFIG_COLUMNS}"
        );
        let config = sqlx::query_as::<_, SecretProviderConfig>(&sql)
            .bind(input.company_id)
            .bind(&input.provider)
            .bind(&input.display_name)
            .bind(&input.status)
            .bind(input.is_default)
            .bind(&input.config)
            .bind(input.created_by_agent_id)
            .bind(&input.created_by_user_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(config)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let sql = format!(
            "SELECT {PROVIDER_CONFIG_COLUMNS} FROM company_secret_provider_configs WHERE id = $1"
        );
        let config = sqlx::query_as::<_, SecretProviderConfig>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(config)
    }

    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<SecretProviderConfig>, RepositoryError> {
        // No status filter: Paperclip returns every config for the company,
        // newest first (`secrets.ts:2856-2861`).
        let sql = format!(
            "SELECT {PROVIDER_CONFIG_COLUMNS} FROM company_secret_provider_configs
             WHERE company_id = $1
             ORDER BY created_at DESC"
        );
        let configs = sqlx::query_as::<_, SecretProviderConfig>(&sql)
            .bind(company_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(configs)
    }

    async fn update(&self, id: Uuid, input: UpdateSecretProviderConfigInput) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let sql = format!(
            "UPDATE company_secret_provider_configs
             SET display_name = COALESCE($2, display_name),
                 status = COALESCE($3, status),
                 is_default = COALESCE($4, is_default),
                 config = COALESCE($5, config),
                 disabled_at = CASE
                     WHEN COALESCE($3, status) = 'disabled' THEN COALESCE(disabled_at, NOW())
                     ELSE NULL
                 END,
                 updated_at = NOW()
             WHERE id = $1
             RETURNING {PROVIDER_CONFIG_COLUMNS}"
        );
        let config = sqlx::query_as::<_, SecretProviderConfig>(&sql)
            .bind(id)
            .bind(&input.display_name)
            .bind(&input.status)
            .bind(input.is_default)
            .bind(&input.config)
            .fetch_optional(&self.pool)
            .await?;

        Ok(config)
    }

    async fn delete(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let sql = format!(
            "DELETE FROM company_secret_provider_configs WHERE id = $1
             RETURNING {PROVIDER_CONFIG_COLUMNS}"
        );
        let config = sqlx::query_as::<_, SecretProviderConfig>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(config)
    }

    async fn set_default(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let mut tx = self.pool.begin().await?;

        // Re-read inside the transaction so a concurrent status change cannot
        // slip a coming_soon/disabled vault into the default slot.
        let current_sql = format!(
            "SELECT {PROVIDER_CONFIG_COLUMNS} FROM company_secret_provider_configs WHERE id = $1"
        );
        let current = sqlx::query_as::<_, SecretProviderConfig>(&current_sql)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

        let Some(current) = current else {
            tx.commit().await?;
            return Ok(None);
        };
        if current.status == "coming_soon" || current.status == "disabled" {
            tx.commit().await?;
            return Ok(None);
        }

        // The partial unique index only covers `(company_id, provider)`, so only
        // peers of the same provider can collide.
        sqlx::query(
            "UPDATE company_secret_provider_configs
             SET is_default = false, updated_at = NOW()
             WHERE company_id = $1 AND provider = $2 AND is_default = true",
        )
        .bind(current.company_id)
        .bind(&current.provider)
        .execute(&mut *tx)
        .await?;

        let updated_sql = format!(
            "UPDATE company_secret_provider_configs
             SET is_default = true, updated_at = NOW()
             WHERE id = $1 AND status NOT IN ('coming_soon', 'disabled')
             RETURNING {PROVIDER_CONFIG_COLUMNS}"
        );
        let updated = sqlx::query_as::<_, SecretProviderConfig>(&updated_sql)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(updated)
    }

    async fn record_health(
        &self,
        id: Uuid,
        status: &str,
        message: &str,
        details: &serde_json::Value,
        checked_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE company_secret_provider_configs
             SET health_status = $2, health_checked_at = $3, health_message = $4,
                 health_details = $5, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(checked_at)
        .bind(message)
        .bind(details)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait]
pub trait UserSecretDefinitionRepository: Send + Sync {
    async fn create(&self, input: CreateUserSecretDefinitionInput) -> Result<UserSecretDefinition, RepositoryError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<UserSecretDefinition>, RepositoryError>;
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<UserSecretDefinition>, RepositoryError>;
    async fn update(&self, id: Uuid, input: UpdateUserSecretDefinitionInput) -> Result<UserSecretDefinition, RepositoryError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
}

pub struct PgUserSecretDefinitionRepository {
    pool: PgPool,
}

impl PgUserSecretDefinitionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserSecretDefinitionRepository for PgUserSecretDefinitionRepository {
    async fn create(&self, input: CreateUserSecretDefinitionInput) -> Result<UserSecretDefinition, RepositoryError> {
        let definition = sqlx::query_as::<_, UserSecretDefinition>(
            r#"
            INSERT INTO user_secret_definitions (
                company_id, name, key, description, required
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, company_id, name, key, description, required, created_at, updated_at
            "#
        )
        .bind(&input.company_id)
        .bind(&input.name)
        .bind(&input.key)
        .bind(&input.description)
        .bind(&input.required)
        .fetch_one(&self.pool)
        .await?;

        Ok(definition)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<UserSecretDefinition>, RepositoryError> {
        let definition = sqlx::query_as::<_, UserSecretDefinition>(
            r#"
            SELECT id, company_id, name, key, description, required, created_at, updated_at
            FROM user_secret_definitions
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(definition)
    }

    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<UserSecretDefinition>, RepositoryError> {
        let definitions = sqlx::query_as::<_, UserSecretDefinition>(
            r#"
            SELECT id, company_id, name, key, description, required, created_at, updated_at
            FROM user_secret_definitions
            WHERE company_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(definitions)
    }

    async fn update(&self, id: Uuid, input: UpdateUserSecretDefinitionInput) -> Result<UserSecretDefinition, RepositoryError> {
        let mut query = String::from("UPDATE user_secret_definitions SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if input.description.is_some() {
            bind_count += 1;
            query.push_str(&format!(", description = ${}", bind_count));
        }
        if input.required.is_some() {
            bind_count += 1;
            query.push_str(&format!(", required = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, company_id, name, key, description, required, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, UserSecretDefinition>(&query).bind(id);

        if let Some(name) = input.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(description) = input.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(required) = input.required {
            query_builder = query_builder.bind(required);
        }

        let definition = query_builder.fetch_one(&self.pool).await?;

        Ok(definition)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM user_secret_definitions
            WHERE id = $1
            "#
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
