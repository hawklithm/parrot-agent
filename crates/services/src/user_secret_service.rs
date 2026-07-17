use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use crate::secret_provider::{LocalEncryptedProvider, load_secret_encryption_key, sha256_hex};
use models::user_secret::{UserSecret, UserSecretCoverage, UserSecretDefinition, UserSecretScope, SecretBinding};
use repositories::user_secret_repository::UserSecretRepository;
use repositories::secret_repository::UserSecretDefinitionRepository;

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

    /// Set (create or update) the encrypted value for a user's secret declaration.
    /// `value` is the **plaintext**; it is encrypted with the local AES-256-GCM
    /// provider before storage (paperclip-aligned material model).
    async fn set_user_secret(
        &self,
        user_id: Uuid,
        definition_id: Uuid,
        value: String,
    ) -> ServiceResult<UserSecret>;

    async fn list_user_secrets(&self, user_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<UserSecret>>;
    async fn get_user_secret(&self, user_id: Uuid, definition_id: Uuid) -> ServiceResult<Option<UserSecret>>;

    /// Rotate the secret value. `new_value` is the **plaintext** new value.
    async fn rotate_user_secret(&self, secret_id: Uuid, new_value: String) -> ServiceResult<UserSecret>;
    async fn delete_user_secret(&self, secret_id: Uuid) -> ServiceResult<()>;
    async fn get_secret_bindings(&self, secret_id: Uuid) -> ServiceResult<Vec<SecretBinding>>;
}

pub struct UserSecretServiceImpl {
    repository: Arc<dyn UserSecretRepository>,
    definition_repository: Arc<dyn UserSecretDefinitionRepository>,
}

impl UserSecretServiceImpl {
    pub fn new(
        repository: Arc<dyn UserSecretRepository>,
        definition_repository: Arc<dyn UserSecretDefinitionRepository>,
    ) -> Self {
        Self { repository, definition_repository }
    }

    fn encrypt_value(&self, plaintext: &str) -> ServiceResult<(String, String)> {
        let key = load_secret_encryption_key();
        let provider = LocalEncryptedProvider::new(key)
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        let ciphertext = provider
            .encrypt(plaintext)
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        let sha = sha256_hex(plaintext);
        Ok((ciphertext, sha))
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
            definition.key = k.clone();
            definition.name = Some(k);
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
        value: String,
    ) -> ServiceResult<UserSecret> {
        let definition = self
            .definition_repository
            .get_by_id(definition_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Definition {} not found", definition_id)))?;

        let (material, sha) = self.encrypt_value(&value)?;
        let env_key = definition.key.clone();
        let company_id = definition.company_id;

        // Find existing declaration for this (user, definition).
        let existing = self
            .repository
            .list_user_secrets(user_id, company_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .into_iter()
            .find(|s| s.user_secret_definition_id == definition_id);

        let secret = if let Some(mut s) = existing {
            s.value_material = Some(material);
            s.value_sha256 = Some(sha);
            s.env_key = env_key;
            self.repository
                .update_secret(s)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?
        } else {
            let mut s = UserSecret::new(company_id, user_id, definition_id, env_key);
            s.value_material = Some(material);
            s.value_sha256 = Some(sha);
            self.repository
                .create_secret(s)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?
        };

        Ok(secret)
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

        Ok(secrets.into_iter().find(|s| s.user_secret_definition_id == definition_id))
    }

    async fn rotate_user_secret(&self, secret_id: Uuid, new_value: String) -> ServiceResult<UserSecret> {
        let mut secret = self.repository
            .get_secret(secret_id)
            .await
            .map_err(|e| ServiceError::Repository(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Secret {} not found", secret_id)))?;

        let rotated = self.encrypt_value(&new_value)?;
        secret.value_material = Some(rotated.0);
        secret.value_sha256 = Some(rotated.1);
        secret.updated_at = chrono::Utc::now();

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
