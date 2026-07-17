use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{
    CreateUserSecretDefinitionRequest, MyUserSecretEntry, SecretBinding,
    UpdateUserSecretDefinitionRequest, UpsertUserSecretRequest, UserSecretCoverageSummary,
    UserSecretDefinition, UserSecretValue,
};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait UserSecretDefinitionService: Send + Sync {
    async fn list_definitions(&self, company_id: Uuid) -> ServiceResult<Vec<UserSecretDefinition>>;
    async fn create_definition(&self, company_id: Uuid, req: CreateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition>;
    async fn get_definition(&self, definition_id: Uuid) -> ServiceResult<UserSecretDefinition>;
    async fn update_definition(&self, definition_id: Uuid, req: UpdateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition>;
    async fn delete_definition(&self, definition_id: Uuid) -> ServiceResult<()>;
    async fn get_coverage(&self, definition_id: Uuid) -> ServiceResult<UserSecretCoverageSummary>;
    async fn list_my_secrets(&self, company_id: Uuid, user_id: Uuid) -> ServiceResult<Vec<MyUserSecretEntry>>;
    async fn upsert_my_secret(&self, company_id: Uuid, user_id: Uuid, req: UpsertUserSecretRequest) -> ServiceResult<UserSecretValue>;
    async fn delete_my_secret(&self, secret_id: Uuid, user_id: Uuid) -> ServiceResult<()>;
    async fn rotate_my_secret(&self, secret_id: Uuid, user_id: Uuid) -> ServiceResult<UserSecretValue>;
    async fn get_secret_bindings(&self, secret_id: Uuid) -> ServiceResult<Vec<SecretBinding>>;
}

pub struct UserSecretDefinitionServiceImpl {}

impl UserSecretDefinitionServiceImpl {
    pub fn new() -> Self {
        Self {}
    }

    fn mock_definition(&self, id: Uuid, company_id: Uuid, key: &str) -> UserSecretDefinition {
        UserSecretDefinition {
            id,
            company_id,
            key: key.to_string(),
            name: format!("{} Secret", key.to_uppercase()),
            description: Some(format!("User-level {} credential", key)),
            required: false,
            status: "active".to_string(),
            provider: "local_encrypted".to_string(),
            managed_mode: "managed".to_string(),
            provider_config_id: None,
            provider_metadata: None,
            usage_guidance: Some("Store your personal API key here".to_string()),
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            updated_by_agent_id: None,
            updated_by_user_id: None,
            deleted_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl UserSecretDefinitionService for UserSecretDefinitionServiceImpl {
    async fn list_definitions(&self, company_id: Uuid) -> ServiceResult<Vec<UserSecretDefinition>> {
        Ok(vec![
            self.mock_definition(Uuid::new_v4(), company_id, "github_token"),
            self.mock_definition(Uuid::new_v4(), company_id, "openai_api_key"),
        ])
    }

    async fn create_definition(&self, company_id: Uuid, req: CreateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition> {
        Ok(UserSecretDefinition {
            id: Uuid::new_v4(),
            company_id,
            key: req.key,
            name: req.name,
            description: req.description,
            required: false,
            status: "active".to_string(),
            provider: req.provider,
            managed_mode: req.managed_mode,
            provider_config_id: None,
            provider_metadata: None,
            usage_guidance: req.usage_guidance,
            created_by_agent_id: None,
            created_by_user_id: Some(Uuid::new_v4()),
            updated_by_agent_id: None,
            updated_by_user_id: None,
            deleted_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn get_definition(&self, definition_id: Uuid) -> ServiceResult<UserSecretDefinition> {
        Ok(self.mock_definition(definition_id, Uuid::new_v4(), "example_key"))
    }

    async fn update_definition(&self, definition_id: Uuid, req: UpdateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition> {
        let mut def = self.mock_definition(definition_id, Uuid::new_v4(), "updated_key");
        if let Some(name) = req.name {
            def.name = name;
        }
        if let Some(description) = req.description {
            def.description = Some(description);
        }
        if let Some(status) = req.status {
            def.status = status;
        }
        if let Some(usage_guidance) = req.usage_guidance {
            def.usage_guidance = Some(usage_guidance);
        }
        def.updated_at = chrono::Utc::now();
        Ok(def)
    }

    async fn delete_definition(&self, _definition_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn get_coverage(&self, definition_id: Uuid) -> ServiceResult<UserSecretCoverageSummary> {
        Ok(UserSecretCoverageSummary {
            definition_id,
            configured_count: 8,
            missing_count: 2,
            inactive_count: 0,
        })
    }

    async fn list_my_secrets(&self, company_id: Uuid, _user_id: Uuid) -> ServiceResult<Vec<MyUserSecretEntry>> {
        let defs = self.list_definitions(company_id).await?;
        Ok(defs.into_iter().map(|definition| {
            use models::user_secret_definition::UserSecretDefinition as TargetDef;
            MyUserSecretEntry {
            definition: TargetDef {
                id: definition.id,
                company_id: definition.company_id,
                key: definition.key,
                name: definition.name,
                description: definition.description,
                status: definition.status,
                provider: definition.provider,
                managed_mode: definition.managed_mode,
                provider_config_id: definition.provider_config_id,
                provider_metadata: definition.provider_metadata.and_then(|m| serde_json::from_str(&m).ok()),
                usage_guidance: definition.usage_guidance,
                created_by_agent_id: definition.created_by_agent_id,
                created_by_user_id: definition.created_by_user_id,
                updated_by_agent_id: definition.updated_by_agent_id,
                updated_by_user_id: definition.updated_by_user_id,
                deleted_at: definition.deleted_at,
                created_at: definition.created_at,
                updated_at: definition.updated_at,
            },
            secret: None,
        }
        }).collect())
    }

    async fn upsert_my_secret(&self, company_id: Uuid, user_id: Uuid, req: UpsertUserSecretRequest) -> ServiceResult<UserSecretValue> {
        Ok(UserSecretValue {
            id: Uuid::new_v4(),
            company_id,
            user_id,
            user_secret_definition_id: req.definition_id,
            key: "example_key".to_string(),
            name: "Example Secret".to_string(),
            provider: "local_encrypted".to_string(),
            status: "active".to_string(),
            managed_mode: "managed".to_string(),
            external_ref: None,
            provider_config_id: None,
            provider_metadata: None,
            latest_version: 1,
            last_resolved_at: Some(chrono::Utc::now()),
            last_rotated_at: None,
            deleted_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn delete_my_secret(&self, _secret_id: Uuid, _user_id: Uuid) -> ServiceResult<()> {
        Ok(())
    }

    async fn rotate_my_secret(&self, secret_id: Uuid, user_id: Uuid) -> ServiceResult<UserSecretValue> {
        let mut secret = self.upsert_my_secret(Uuid::new_v4(), user_id, UpsertUserSecretRequest {
            definition_id: Uuid::new_v4(),
            value: "rotated_value".to_string(),
        }).await?;
        secret.id = secret_id;
        secret.latest_version += 1;
        secret.last_rotated_at = Some(chrono::Utc::now());
        Ok(secret)
    }

    async fn get_secret_bindings(&self, _secret_id: Uuid) -> ServiceResult<Vec<SecretBinding>> {
        Ok(vec![
            SecretBinding {
                id: Uuid::new_v4(),
                secret_id: Uuid::new_v4(),
                target_type: models::SecretBindingTargetType::Agent,
                target_id: Uuid::new_v4(),
                config_path: Some("env.GITHUB_TOKEN".to_string()),
                env_key: Some("GITHUB_TOKEN".to_string()),
                required: true,
                created_at: chrono::Utc::now(),
            },
        ])
    }
}

pub fn create_user_secret_definition_service() -> Arc<dyn UserSecretDefinitionService> {
    Arc::new(UserSecretDefinitionServiceImpl::new())
}
