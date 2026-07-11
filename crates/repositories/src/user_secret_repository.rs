use async_trait::async_trait;
use chrono::{DateTime, Utc};
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
               (id, company_id, key, description, required, scope, created_at, updated_at, created_by_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(definition.id)
        .bind(definition.company_id)
        .bind(&definition.key)
        .bind(&definition.description)
        .bind(definition.required)
        .bind(&definition.scope)
        .bind(definition.created_at)
        .bind(definition.updated_at)
        .bind(definition.created_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(definition)
    }

    async fn list_definitions(&self, company_id: Uuid) -> RepositoryResult<Vec<UserSecretDefinition>> {
        let definitions = sqlx::query_as::<_, UserSecretDefinition>(
            r#"SELECT id, company_id, key, description, required, scope, created_at, updated_at, created_by_user_id
               FROM user_secret_definitions
               WHERE company_id = $1
               ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(definitions)
    }

    async fn get_definition(&self, definition_id: Uuid) -> RepositoryResult<Option<UserSecretDefinition>> {
        let definition = sqlx::query_as::<_, UserSecretDefinition>(
            r#"SELECT id, company_id, key, description, required, scope, created_at, updated_at, created_by_user_id
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
               SET key = $2, description = $3, required = $4, scope = $5, updated_at = $6
               WHERE id = $1"#
        )
        .bind(definition.id)
        .bind(&definition.key)
        .bind(&definition.description)
        .bind(definition.required)
        .bind(&definition.scope)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(definition)
    }

    async fn delete_definition(&self, definition_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM user_secret_definitions WHERE id = $1")
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
                 (SELECT COUNT(DISTINCT us.user_id)
                  FROM user_secrets us
                  WHERE us.definition_id = $2) as users_with_secret"#
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
            r#"INSERT INTO user_secrets
               (id, user_id, definition_id, encrypted_value, created_at, updated_at, last_rotated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#
        )
        .bind(secret.id)
        .bind(secret.user_id)
        .bind(secret.definition_id)
        .bind(&secret.encrypted_value)
        .bind(secret.created_at)
        .bind(secret.updated_at)
        .bind(secret.last_rotated_at)
        .execute(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> RepositoryResult<Vec<UserSecret>> {
        let secrets = sqlx::query_as::<_, UserSecret>(
            r#"SELECT us.id, us.user_id, us.definition_id, us.encrypted_value,
                      us.created_at, us.updated_at, us.last_rotated_at
               FROM user_secrets us
               JOIN user_secret_definitions usd ON us.definition_id = usd.id
               WHERE us.user_id = $1 AND usd.company_id = $2
               ORDER BY us.created_at DESC"#
        )
        .bind(user_id)
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(secrets)
    }

    async fn get_secret(&self, secret_id: Uuid) -> RepositoryResult<Option<UserSecret>> {
        let secret = sqlx::query_as::<_, UserSecret>(
            r#"SELECT id, user_id, definition_id, encrypted_value, created_at, updated_at, last_rotated_at
               FROM user_secrets
               WHERE id = $1"#
        )
        .bind(secret_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn update_secret(&self, secret: UserSecret) -> RepositoryResult<UserSecret> {
        sqlx::query(
            r#"UPDATE user_secrets
               SET encrypted_value = $2, updated_at = $3, last_rotated_at = $4
               WHERE id = $1"#
        )
        .bind(secret.id)
        .bind(&secret.encrypted_value)
        .bind(Utc::now())
        .bind(secret.last_rotated_at)
        .execute(&self.pool)
        .await?;
        Ok(secret)
    }

    async fn delete_secret(&self, secret_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM user_secrets WHERE id = $1")
            .bind(secret_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_secret_bindings(&self, secret_id: Uuid) -> RepositoryResult<Vec<SecretBinding>> {
        Ok(vec![])
    }
}
