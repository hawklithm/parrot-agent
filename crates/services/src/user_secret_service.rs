use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::user_secret::{UserSecret, UserSecretCoverage, UserSecretDefinition, UserSecretScope, SecretBinding};
use repositories::user_secret_repository::UserSecretRepository;

#[async_trait]
pub trait UserSecretService: Send + Sync {
    async fn create_definition(
        &self,
        company_id: Uuid,
        key: String,
        description: Option<String>,
        required: bool,
        scope: UserSecretScope,
        created_by_user_id: Uuid,
    ) -> ServiceResult<UserSecretDefinition>;

    async fn list_definitions(&self, company_id: Uuid) -> ServiceResult<Vec<UserSecretDefinition>>;
    async fn get_definition(&self, definition_id: Uuid) -> ServiceResult<Option<UserSecretDefinition>>;

    async fn update_definition(
        &self,
        definition_id: Uuid,
        key: Option<String>,
        description: Option<String>,
        required: Option<bool>,
        scope: Option<UserSecretScope>,
    ) -> ServiceResult<UserSecretDefinition>;

    async fn delete_definition(&self, definition_id: Uuid) -> ServiceResult<()>;
    async fn get_coverage_stats(&self, company_id: Uuid, definition_id: Uuid) -> ServiceResult<UserSecretCoverage>;

    async fn set_user_secret(
        &self,
        user_id: Uuid,
        definition_id: Uuid,
        encrypted_value: String,
    ) -> ServiceResult<UserSecret>;

    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<UserSecret>>;
    async fn get_user_secret(&self, user_id: Uuid, definition_id: Uuid) -> ServiceResult<Option<UserSecret>>;
    async fn rotate_user_secret(&self, secret_id: Uuid, new_encrypted_value: String) -> ServiceResult<UserSecret>;
    async fn delete_user_secret(&self, secret_id: Uuid) -> ServiceResult<()>;
    async fn get_secret_bindings(&self, secret_id: Uuid) -> ServiceResult<Vec<SecretBinding>>;
}

pub struct UserSecretServiceImpl {
    repository: Arc<dyn UserSecretRepository>,
}

impl UserSecretServiceImpl {
    pub fn new(repository: Arc<dyn UserSecretRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl UserSecretService for UserSecretServiceImpl {
    async fn create_definition(
        &self,
        company_id: Uuid,
        key: String,
        description: Option<String>,
        required: bool,
        scope: UserSecretScope,
        created_by_user_id: Uuid,
    ) -> ServiceResult<UserSecretDefinition> {
        let definition = UserSecretDefinition::new(
            company_id,
            key,
            description,
            required,
            scope,
            created_by_user_id,
        );

        self.repository
            .create_definition(definition)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn list_definitions(&self, company_id: Uuid) -> ServiceResult<Vec<UserSecretDefinition>> {
        self.repository
            .list_definitions(company_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_definition(&self, definition_id: Uuid) -> ServiceResult<Option<UserSecretDefinition>> {
        self.repository
            .get_definition(definition_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn update_definition(
        &self,
        definition_id: Uuid,
        key: Option<String>,
        description: Option<String>,
        required: Option<bool>,
        scope: Option<UserSecretScope>,
    ) -> ServiceResult<UserSecretDefinition> {
        let mut definition = self.repository
            .get_definition(definition_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Definition {} not found", definition_id)))?;

        if let Some(k) = key {
            definition.key = k;
        }
        if let Some(d) = description {
            definition.description = Some(d);
        }
        if let Some(r) = required {
            definition.required = r;
        }
        if let Some(s) = scope {
            definition.scope = sqlx::types::Json(s);
        }

        self.repository
            .update_definition(definition)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn delete_definition(&self, definition_id: Uuid) -> ServiceResult<()> {
        self.repository
            .delete_definition(definition_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_coverage_stats(&self, company_id: Uuid, definition_id: Uuid) -> ServiceResult<UserSecretCoverage> {
        self.repository
            .get_coverage_stats(company_id, definition_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn set_user_secret(
        &self,
        user_id: Uuid,
        definition_id: Uuid,
        encrypted_value: String,
    ) -> ServiceResult<UserSecret> {
        // First check if secret already exists
        let secrets = self.repository
            .list_user_secrets(user_id, Uuid::nil())
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let existing = secrets.iter().find(|s| s.definition_id == definition_id);

        if let Some(secret) = existing {
            let mut updated = secret.clone();
            updated.rotate(encrypted_value);
            self.repository
                .update_secret(updated)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))
        } else {
            let secret = UserSecret::new(user_id, definition_id, encrypted_value);
            self.repository
                .create_secret(secret)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))
        }
    }

    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<UserSecret>> {
        self.repository
            .list_user_secrets(user_id, company_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_user_secret(&self, user_id: Uuid, definition_id: Uuid) -> ServiceResult<Option<UserSecret>> {
        let secrets = self.repository
            .list_user_secrets(user_id, Uuid::nil())
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        Ok(secrets.into_iter().find(|s| s.definition_id == definition_id))
    }

    async fn rotate_user_secret(&self, secret_id: Uuid, new_encrypted_value: String) -> ServiceResult<UserSecret> {
        let mut secret = self.repository
            .get_secret(secret_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Secret {} not found", secret_id)))?;

        secret.rotate(new_encrypted_value);

        self.repository
            .update_secret(secret)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn delete_user_secret(&self, secret_id: Uuid) -> ServiceResult<()> {
        self.repository
            .delete_secret(secret_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }

    async fn get_secret_bindings(&self, secret_id: Uuid) -> ServiceResult<Vec<SecretBinding>> {
        self.repository
            .get_secret_bindings(secret_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))
    }
}
