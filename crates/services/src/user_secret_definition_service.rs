use crate::errors::ServiceResult;
use async_trait::async_trait;
use models::{
    CreateUserSecretDefinitionRequest, MyUserSecretEntry, SecretBinding,
    UpdateUserSecretDefinitionRequest, UpsertUserSecretRequest, UserSecretCoverageSummary,
    UserSecretDefinition, UserSecretValue,
};
use std::sync::Arc;
use uuid::Uuid;
use sqlx::{PgPool, Row};

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

pub struct UserSecretDefinitionServiceImpl {
    pool: Option<PgPool>,
}

impl UserSecretDefinitionServiceImpl {
    pub fn new() -> Self {
        Self { pool: None }
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    fn from_row(row: &sqlx::postgres::PgRow) -> UserSecretDefinition {
        UserSecretDefinition {
            id: row.get("id"), company_id: row.get("company_id"),
            key: row.get("key"), name: row.get("name"), description: row.get("description"), required: row.get("required"),
            status: row.get("status"), provider: row.get("provider"), managed_mode: row.get("managed_mode"),
            provider_config_id: row.get("provider_config_id"), provider_metadata: row.get("provider_metadata"),
            usage_guidance: row.get("usage_guidance"), created_by_agent_id: row.get("created_by_agent_id"),
            created_by_user_id: row.get("created_by_user_id"), updated_by_agent_id: row.get("updated_by_agent_id"),
            updated_by_user_id: row.get("updated_by_user_id"), deleted_at: row.get("deleted_at"),
            created_at: row.get("created_at"), updated_at: row.get("updated_at"),
        }
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
        if let Some(pool) = &self.pool {
            let rows = sqlx::query("SELECT * FROM user_secret_definitions WHERE company_id = $1 AND deleted_at IS NULL ORDER BY created_at DESC")
                .bind(company_id).fetch_all(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?;
            return Ok(rows.iter().map(Self::from_row).collect());
        }
        Ok(vec![
            self.mock_definition(Uuid::new_v4(), company_id, "github_token"),
            self.mock_definition(Uuid::new_v4(), company_id, "openai_api_key"),
        ])
    }

    async fn create_definition(&self, company_id: Uuid, req: CreateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query("INSERT INTO user_secret_definitions (company_id,key,name,description,provider,managed_mode,usage_guidance) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING *")
                .bind(company_id).bind(&req.key).bind(&req.name).bind(&req.description).bind(&req.provider).bind(&req.managed_mode).bind(&req.usage_guidance)
                .fetch_one(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?;
            return Ok(Self::from_row(&row));
        }
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
        if let Some(pool) = &self.pool {
            let row = sqlx::query("SELECT * FROM user_secret_definitions WHERE id = $1 AND deleted_at IS NULL")
                .bind(definition_id).fetch_optional(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?
                .ok_or_else(|| crate::errors::ServiceError::NotFound(format!("Definition {} not found", definition_id)))?;
            return Ok(Self::from_row(&row));
        }
        Ok(self.mock_definition(definition_id, Uuid::new_v4(), "example_key"))
    }

    async fn update_definition(&self, definition_id: Uuid, req: UpdateUserSecretDefinitionRequest) -> ServiceResult<UserSecretDefinition> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query("UPDATE user_secret_definitions SET name = COALESCE($2,name), description = COALESCE($3,description), status = COALESCE($4,status), usage_guidance = COALESCE($5,usage_guidance), updated_at = now() WHERE id = $1 AND deleted_at IS NULL RETURNING *")
                .bind(definition_id).bind(req.name).bind(req.description).bind(req.status).bind(req.usage_guidance)
                .fetch_optional(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?
                .ok_or_else(|| crate::errors::ServiceError::NotFound(format!("Definition {} not found", definition_id)))?;
            return Ok(Self::from_row(&row));
        }
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
        if let Some(pool) = &self.pool {
            sqlx::query("UPDATE user_secret_definitions SET deleted_at = now(), status = 'archived', updated_at = now() WHERE id = $1")
                .bind(_definition_id).execute(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?;
            return Ok(());
        }
        Ok(())
    }

    async fn get_coverage(&self, definition_id: Uuid) -> ServiceResult<UserSecretCoverageSummary> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query("SELECT d.id, COUNT(s.id) FILTER (WHERE s.value_material IS NOT NULL) AS configured_count, COUNT(s.id) FILTER (WHERE s.value_material IS NULL) AS missing_count FROM user_secret_definitions d LEFT JOIN user_secret_declarations s ON s.user_secret_definition_id = d.id WHERE d.id = $1 AND d.deleted_at IS NULL GROUP BY d.id")
                .bind(definition_id).fetch_optional(pool).await.map_err(|e| crate::errors::ServiceError::Repository(e.to_string()))?
                .ok_or_else(|| crate::errors::ServiceError::NotFound(format!("Definition {} not found", definition_id)))?;
            return Ok(UserSecretCoverageSummary {
                definition_id: row.get("id"),
                configured_count: row.get::<i64, _>("configured_count") as i32,
                missing_count: row.get::<i64, _>("missing_count") as i32,
                inactive_count: 0,
            });
        }
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
