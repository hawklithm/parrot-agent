use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::user_secret::{UserSecret, UserSecretCoverage, UserSecretDefinition, SecretBinding};

#[async_trait]
pub trait UserSecretRepository: Send + Sync {
    async fn create_definition(&self, definition: UserSecretDefinition) -> RepositoryResult<UserSecretDefinition>;
    async fn list_definitions(&self, company_id: Uuid) -> RepositoryResult<Vec<UserSecretDefinition>>;
    async fn get_definition(&self, definition_id: Uuid) -> RepositoryResult<Option<UserSecretDefinition>>;
    async fn update_definition(&self, definition: UserSecretDefinition) -> RepositoryResult<UserSecretDefinition>;
    async fn delete_definition(&self, definition_id: Uuid) -> RepositoryResult<()>;
    async fn get_coverage_stats(&self, company_id: Uuid, definition_id: Uuid) -> RepositoryResult<UserSecretCoverage>;

    async fn create_secret(&self, secret: UserSecret) -> RepositoryResult<UserSecret>;
    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> RepositoryResult<Vec<UserSecret>>;
    async fn get_secret(&self, secret_id: Uuid) -> RepositoryResult<Option<UserSecret>>;
    async fn update_secret(&self, secret: UserSecret) -> RepositoryResult<UserSecret>;
    async fn delete_secret(&self, secret_id: Uuid) -> RepositoryResult<()>;
    async fn get_secret_bindings(&self, secret_id: Uuid) -> RepositoryResult<Vec<SecretBinding>>;
}

pub struct PostgresUserSecretRepository {
    pool: PgPool,
}

impl PostgresUserSecretRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserSecretRepository for PostgresUserSecretRepository {
    async fn create_definition(&self, definition: UserSecretDefinition) -> RepositoryResult<UserSecretDefinition> {
        sqlx::query(
            r#"INSERT INTO user_secret_definitions
               (id, company_id, key, name, description, status, provider, managed_mode,
                provider_config_id, provider_metadata, usage_guidance, required,
                created_at, updated_at, created_by_user_id, updated_by_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#
        )
        .bind(definition.id)
        .bind(definition.company_id)
        .bind(&definition.key)
        .bind(&definition.name)
        .bind(&definition.description)
        .bind(&definition.status)
        .bind(&definition.provider)
        .bind(&definition.managed_mode)
        .bind(definition.provider_config_id)
        .bind(&definition.provider_metadata)
        .bind(&definition.usage_guidance)
        .bind(definition.required)
        .bind(definition.created_at)
        .bind(definition.updated_at)
        .bind(definition.created_by_user_id)
        .bind(definition.updated_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(definition)
    }

    async fn list_definitions(&self, company_id: Uuid) -> RepositoryResult<Vec<UserSecretDefinition>> {
        let definitions = sqlx::query_as::<_, UserSecretDefinition>(
            r#"SELECT id, company_id, key, name, description, status, provider, managed_mode,
                      provider_config_id, provider_metadata, usage_guidance, required,
                      created_at, updated_at, created_by_user_id, updated_by_user_id, deleted_at
               FROM user_secret_definitions
               WHERE company_id = $1 AND deleted_at IS NULL
               ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(definitions)
    }

    async fn get_definition(&self, definition_id: Uuid) -> RepositoryResult<Option<UserSecretDefinition>> {
        let definition = sqlx::query_as::<_, UserSecretDefinition>(
            r#"SELECT id, company_id, key, name, description, status, provider, managed_mode,
                      provider_config_id, provider_metadata, usage_guidance, required,
                      created_at, updated_at, created_by_user_id, updated_by_user_id, deleted_at
               FROM user_secret_definitions
               WHERE id = $1"#
        )
        .bind(definition_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(definition)
    }

    async fn update_definition(&self, definition: UserSecretDefinition) -> RepositoryResult<UserSecretDefinition> {
        sqlx::query(
            r#"UPDATE user_secret_definitions
               SET key = $2, name = $3, description = $4, status = $5, provider = $6,
                   managed_mode = $7, provider_config_id = $8, provider_metadata = $9,
                   usage_guidance = $10, required = $11, updated_at = $12, updated_by_user_id = $13
               WHERE id = $1"#
        )
        .bind(definition.id)
        .bind(&definition.key)
        .bind(&definition.name)
        .bind(&definition.description)
        .bind(&definition.status)
        .bind(&definition.provider)
        .bind(&definition.managed_mode)
        .bind(definition.provider_config_id)
        .bind(&definition.provider_metadata)
        .bind(&definition.usage_guidance)
        .bind(definition.required)
        .bind(Utc::now())
        .bind(definition.updated_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(definition)
    }

    async fn delete_definition(&self, definition_id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"UPDATE user_secret_definitions SET deleted_at = now(), updated_at = now() WHERE id = $1"#
        )
        .bind(definition_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_coverage_stats(&self, company_id: Uuid, definition_id: Uuid) -> RepositoryResult<UserSecretCoverage> {
        let definition = self.get_definition(definition_id).await?
            .ok_or_else(|| crate::RepositoryError::InvalidData("Definition not found".to_string()))?;

        let (total_users, users_with_secret): (i64, i64) = sqlx::query_as(
            r#"SELECT
                 (SELECT COUNT(DISTINCT cm.principal_id)
                  FROM company_memberships cm
                  WHERE cm.company_id = $1 AND cm.principal_type = 'user' AND cm.status = 'active') as total_users,
                 (SELECT COUNT(DISTINCT usd.target_id)
                  FROM user_secret_declarations usd
                  WHERE usd.user_secret_definition_id = $2
                    AND usd.target_type = 'user'
                    AND usd.value_material IS NOT NULL) as users_with_secret"#
        )
        .bind(company_id)
        .bind(definition_id)
        .fetch_one(&self.pool)
        .await?;

        let coverage_percentage = if total_users > 0 {
            (users_with_secret as f64 / total_users as f64) * 100.0
        } else {
            0.0
        };

        Ok(UserSecretCoverage {
            definition_id,
            definition_key: definition.key,
            total_users,
            users_with_secret,
            coverage_percentage,
            required: definition.required,
        })
    }

    async fn create_secret(&self, secret: UserSecret) -> RepositoryResult<UserSecret> {
        sqlx::query(
            r#"INSERT INTO user_secret_declarations
               (id, company_id, user_secret_definition_id, target_type, target_id, config_path,
                env_key, version_selector, required, allow_missing_override, value_material, value_sha256,
                created_at, updated_at)
               VALUES ($1, $2, $3, 'user', $4, 'env', $5, $6, $7, $8, $9, $10, $11, $12)"#
        )
        .bind(secret.id)
        .bind(secret.company_id)
        .bind(secret.user_secret_definition_id)
        .bind(secret.user_id.to_string())
        .bind(&secret.env_key)
        .bind(&secret.version_selector)
        .bind(secret.required)
        .bind(secret.allow_missing_override)
        .bind(&secret.value_material)
        .bind(&secret.value_sha256)
        .bind(secret.created_at)
        .bind(secret.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> RepositoryResult<Vec<UserSecret>> {
        let secrets = sqlx::query_as::<_, UserSecret>(
            r#"SELECT id, company_id, user_secret_definition_id, target_id::uuid AS user_id, env_key,
                      value_material, value_sha256, version_selector, required, allow_missing_override,
                      created_at, updated_at
               FROM user_secret_declarations
               WHERE target_type = 'user' AND target_id = $1 AND company_id = $2
               ORDER BY created_at DESC"#
        )
        .bind(user_id.to_string())
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(secrets)
    }

    async fn get_secret(&self, secret_id: Uuid) -> RepositoryResult<Option<UserSecret>> {
        let secret = sqlx::query_as::<_, UserSecret>(
            r#"SELECT id, company_id, user_secret_definition_id, target_id::uuid AS user_id, env_key,
                      value_material, value_sha256, version_selector, required, allow_missing_override,
                      created_at, updated_at
               FROM user_secret_declarations
               WHERE id = $1"#
        )
        .bind(secret_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn update_secret(&self, secret: UserSecret) -> RepositoryResult<UserSecret> {
        sqlx::query(
            r#"UPDATE user_secret_declarations
               SET value_material = $2, value_sha256 = $3, env_key = $4,
                   version_selector = $5, required = $6, allow_missing_override = $7, updated_at = $8
               WHERE id = $1"#
        )
        .bind(secret.id)
        .bind(&secret.value_material)
        .bind(&secret.value_sha256)
        .bind(&secret.env_key)
        .bind(&secret.version_selector)
        .bind(secret.required)
        .bind(secret.allow_missing_override)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn delete_secret(&self, secret_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM user_secret_declarations WHERE id = $1")
            .bind(secret_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_secret_bindings(&self, _secret_id: Uuid) -> RepositoryResult<Vec<SecretBinding>> {
        Ok(vec![])
    }
}
