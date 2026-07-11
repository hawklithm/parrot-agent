use async_trait::async_trait;
use models::{
    CompanySecret, SecretStatus, CreateSecretInput, UpdateSecretInput,
    SecretBinding, SecretBindingTargetType,
    SecretProviderConfig, SecretProviderType, CreateSecretProviderConfigInput, UpdateSecretProviderConfigInput,
    UserSecretDefinition, CreateUserSecretDefinitionInput, UpdateUserSecretDefinitionInput,
    UserSecret, CreateUserSecretInput, UpdateUserSecretInput,
};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

#[async_trait]
pub trait SecretRepository: Send + Sync {
    /// Create a new company secret
    async fn create(&self, input: CreateSecretInput) -> Result<CompanySecret, RepositoryError>;

    /// Get a secret by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CompanySecret>, RepositoryError>;

    /// Get a secret by company_id and key
    async fn get_by_key(&self, company_id: Uuid, key: &str) -> Result<Option<CompanySecret>, RepositoryError>;

    /// List all secrets for a company
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<CompanySecret>, RepositoryError>;

    /// Update a secret
    async fn update(&self, id: Uuid, input: UpdateSecretInput) -> Result<CompanySecret, RepositoryError>;

    /// Delete a secret (soft delete)
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// List bindings for a secret
    async fn list_bindings(&self, secret_id: Uuid) -> Result<Vec<SecretBinding>, RepositoryError>;

    /// Create a binding between secret and target
    async fn create_binding(&self, secret_id: Uuid, target_type: SecretBindingTargetType, target_id: Uuid) -> Result<SecretBinding, RepositoryError>;

    /// Delete a binding
    async fn delete_binding(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// List bindings for a target
    async fn list_bindings_by_target(&self, target_type: SecretBindingTargetType, target_id: Uuid) -> Result<Vec<SecretBinding>, RepositoryError>;
}

/// PostgreSQL implementation of SecretRepository
pub struct PgSecretRepository {
    pool: PgPool,
}

impl PgSecretRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecretRepository for PgSecretRepository {
    async fn create(&self, input: CreateSecretInput) -> Result<CompanySecret, RepositoryError> {
        let secret = sqlx::query_as::<_, CompanySecret>(
            r#"
            INSERT INTO company_secrets (
                company_id, name, key, provider, provider_config_id,
                managed_mode, scope, description
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, company_id, name, key, provider, provider_config_id,
                      managed_mode, scope, description, status, created_at, updated_at
            "#
        )
        .bind(&input.company_id)
        .bind(&input.name)
        .bind(&input.key)
        .bind(&input.provider)
        .bind(&input.provider_config_id)
        .bind(&input.managed_mode)
        .bind(&input.scope)
        .bind(&input.description)
        .fetch_one(&self.pool)
        .await?;

        Ok(secret)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<CompanySecret>, RepositoryError> {
        let secret = sqlx::query_as::<_, CompanySecret>(
            r#"
            SELECT id, company_id, name, key, provider, provider_config_id,
                   managed_mode, scope, description, status, created_at, updated_at
            FROM company_secrets
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(secret)
    }

    async fn get_by_key(&self, company_id: Uuid, key: &str) -> Result<Option<CompanySecret>, RepositoryError> {
        let secret = sqlx::query_as::<_, CompanySecret>(
            r#"
            SELECT id, company_id, name, key, provider, provider_config_id,
                   managed_mode, scope, description, status, created_at, updated_at
            FROM company_secrets
            WHERE company_id = $1 AND key = $2 AND status = $3
            "#
        )
        .bind(company_id)
        .bind(key)
        .bind(SecretStatus::Active)
        .fetch_optional(&self.pool)
        .await?;

        Ok(secret)
    }

    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<CompanySecret>, RepositoryError> {
        let secrets = sqlx::query_as::<_, CompanySecret>(
            r#"
            SELECT id, company_id, name, key, provider, provider_config_id,
                   managed_mode, scope, description, status, created_at, updated_at
            FROM company_secrets
            WHERE company_id = $1 AND status = $2
            ORDER BY created_at DESC
            "#
        )
        .bind(company_id)
        .bind(SecretStatus::Active)
        .fetch_all(&self.pool)
        .await?;

        Ok(secrets)
    }

    async fn update(&self, id: Uuid, input: UpdateSecretInput) -> Result<CompanySecret, RepositoryError> {
        let mut query = String::from("UPDATE company_secrets SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if input.description.is_some() {
            bind_count += 1;
            query.push_str(&format!(", description = ${}", bind_count));
        }
        if input.status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", status = ${}", bind_count));
        }
        if input.provider.is_some() {
            bind_count += 1;
            query.push_str(&format!(", provider = ${}", bind_count));
        }
        if input.provider_config_id.is_some() {
            bind_count += 1;
            query.push_str(&format!(", provider_config_id = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, company_id, name, key, provider, provider_config_id, managed_mode, scope, description, status, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, CompanySecret>(&query).bind(id);

        if let Some(name) = input.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(description) = input.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(status) = input.status {
            query_builder = query_builder.bind(status);
        }
        if let Some(provider) = input.provider {
            query_builder = query_builder.bind(provider);
        }
        if let Some(provider_config_id) = input.provider_config_id {
            query_builder = query_builder.bind(provider_config_id);
        }

        let secret = query_builder.fetch_one(&self.pool).await?;

        Ok(secret)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE company_secrets
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind(SecretStatus::Archived)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_bindings(&self, secret_id: Uuid) -> Result<Vec<SecretBinding>, RepositoryError> {
        let bindings = sqlx::query_as::<_, SecretBinding>(
            r#"
            SELECT id, secret_id, target_type, target_id, created_at
            FROM secret_bindings
            WHERE secret_id = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(secret_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(bindings)
    }

    async fn create_binding(&self, secret_id: Uuid, target_type: SecretBindingTargetType, target_id: Uuid) -> Result<SecretBinding, RepositoryError> {
        let binding = sqlx::query_as::<_, SecretBinding>(
            r#"
            INSERT INTO secret_bindings (secret_id, target_type, target_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (secret_id, target_type, target_id) DO UPDATE
            SET created_at = NOW()
            RETURNING id, secret_id, target_type, target_id, created_at
            "#
        )
        .bind(secret_id)
        .bind(target_type)
        .bind(target_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(binding)
    }

    async fn delete_binding(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            DELETE FROM secret_bindings
            WHERE id = $1
            "#
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_bindings_by_target(&self, target_type: SecretBindingTargetType, target_id: Uuid) -> Result<Vec<SecretBinding>, RepositoryError> {
        let bindings = sqlx::query_as::<_, SecretBinding>(
            r#"
            SELECT id, secret_id, target_type, target_id, created_at
            FROM secret_bindings
            WHERE target_type = $1 AND target_id = $2
            ORDER BY created_at DESC
            "#
        )
        .bind(target_type)
        .bind(target_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(bindings)
    }
}

#[async_trait]
pub trait SecretProviderConfigRepository: Send + Sync {
    async fn create(&self, input: CreateSecretProviderConfigInput) -> Result<SecretProviderConfig, RepositoryError>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError>;
    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<SecretProviderConfig>, RepositoryError>;
    async fn update(&self, id: Uuid, input: UpdateSecretProviderConfigInput) -> Result<SecretProviderConfig, RepositoryError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
    async fn set_default(&self, id: Uuid, company_id: Uuid) -> Result<SecretProviderConfig, RepositoryError>;
    async fn get_default(&self, company_id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError>;
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
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"
            INSERT INTO secret_provider_configs (
                company_id, provider_type, name, config, is_default
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, company_id, provider_type, name, config, is_default, status, created_at, updated_at
            "#
        )
        .bind(&input.company_id)
        .bind(&input.provider_type)
        .bind(&input.name)
        .bind(&input.config)
        .bind(&input.is_default)
        .fetch_one(&self.pool)
        .await?;

        Ok(config)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"
            SELECT id, company_id, provider_type, name, config, is_default, status, created_at, updated_at
            FROM secret_provider_configs
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(config)
    }

    async fn list_by_company(&self, company_id: Uuid) -> Result<Vec<SecretProviderConfig>, RepositoryError> {
        let configs = sqlx::query_as::<_, SecretProviderConfig>(
            r#"
            SELECT id, company_id, provider_type, name, config, is_default, status, created_at, updated_at
            FROM secret_provider_configs
            WHERE company_id = $1 AND status = $2
            ORDER BY is_default DESC, created_at DESC
            "#
        )
        .bind(company_id)
        .bind(SecretStatus::Active)
        .fetch_all(&self.pool)
        .await?;

        Ok(configs)
    }

    async fn update(&self, id: Uuid, input: UpdateSecretProviderConfigInput) -> Result<SecretProviderConfig, RepositoryError> {
        let mut query = String::from("UPDATE secret_provider_configs SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if input.config.is_some() {
            bind_count += 1;
            query.push_str(&format!(", config = ${}", bind_count));
        }
        if input.is_default.is_some() {
            bind_count += 1;
            query.push_str(&format!(", is_default = ${}", bind_count));
        }
        if input.status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", status = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, company_id, provider_type, name, config, is_default, status, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, SecretProviderConfig>(&query).bind(id);

        if let Some(name) = input.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(config) = input.config {
            query_builder = query_builder.bind(config);
        }
        if let Some(is_default) = input.is_default {
            query_builder = query_builder.bind(is_default);
        }
        if let Some(status) = input.status {
            query_builder = query_builder.bind(status);
        }

        let config = query_builder.fetch_one(&self.pool).await?;

        Ok(config)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE secret_provider_configs
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind(SecretStatus::Archived)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn set_default(&self, id: Uuid, company_id: Uuid) -> Result<SecretProviderConfig, RepositoryError> {
        let mut tx = self.pool.begin().await?;

        // Unset all other defaults
        sqlx::query(
            r#"
            UPDATE secret_provider_configs
            SET is_default = false, updated_at = NOW()
            WHERE company_id = $1 AND is_default = true
            "#
        )
        .bind(company_id)
        .execute(&mut *tx)
        .await?;

        // Set this one as default
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"
            UPDATE secret_provider_configs
            SET is_default = true, updated_at = NOW()
            WHERE id = $1 AND company_id = $2
            RETURNING id, company_id, provider_type, name, config, is_default, status, created_at, updated_at
            "#
        )
        .bind(id)
        .bind(company_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(config)
    }

    async fn get_default(&self, company_id: Uuid) -> Result<Option<SecretProviderConfig>, RepositoryError> {
        let config = sqlx::query_as::<_, SecretProviderConfig>(
            r#"
            SELECT id, company_id, provider_type, name, config, is_default, status, created_at, updated_at
            FROM secret_provider_configs
            WHERE company_id = $1 AND is_default = true AND status = $2
            "#
        )
        .bind(company_id)
        .bind(SecretStatus::Active)
        .fetch_optional(&self.pool)
        .await?;

        Ok(config)
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
