use async_trait::async_trait;
use models::{
    ExecutionEnvironment, EnvironmentStatus, CreateEnvironmentInput,
    UpdateEnvironmentInput, EnvironmentDeleteBlastRadius,
    EnvironmentStaticReferences, EnvironmentActiveRuntimeUse,
    EnvironmentDeleteBlockedReason,
};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

#[async_trait]
pub trait EnvironmentRepository: Send + Sync {
    /// Create a new environment
    async fn create(&self, input: CreateEnvironmentInput) -> Result<ExecutionEnvironment, RepositoryError>;

    /// Get an environment by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ExecutionEnvironment>, RepositoryError>;

    /// Get an environment by name
    async fn get_by_name(&self, name: &str) -> Result<Option<ExecutionEnvironment>, RepositoryError>;

    /// List environments by status
    async fn list_by_status(&self, status: EnvironmentStatus) -> Result<Vec<ExecutionEnvironment>, RepositoryError>;

    /// List all environments
    async fn list_all(&self) -> Result<Vec<ExecutionEnvironment>, RepositoryError>;

    /// Update an environment
    async fn update(&self, id: Uuid, input: UpdateEnvironmentInput) -> Result<ExecutionEnvironment, RepositoryError>;

    /// Delete an environment (soft delete - set status to archived)
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Get delete blast radius (what would be affected if we delete this environment)
    async fn get_delete_blast_radius(&self, id: Uuid) -> Result<EnvironmentDeleteBlastRadius, RepositoryError>;
}

/// PostgreSQL implementation of EnvironmentRepository
pub struct PgEnvironmentRepository {
    pool: PgPool,
}

impl PgEnvironmentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EnvironmentRepository for PgEnvironmentRepository {
    async fn create(&self, input: CreateEnvironmentInput) -> Result<ExecutionEnvironment, RepositoryError> {
        let status = input.status.unwrap_or(EnvironmentStatus::Active);
        let config = input.config;
        let env_vars = input.env_vars.unwrap_or_else(|| serde_json::json!({}));

        let environment = sqlx::query_as::<_, ExecutionEnvironment>(
            r#"
            INSERT INTO environments (name, description, driver, status, config, env_vars, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at
            "#
        )
        .bind(&input.name)
        .bind(&input.description)
        .bind(&input.driver)
        .bind(&status)
        .bind(&config)
        .bind(&env_vars)
        .bind(&input.metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(environment)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ExecutionEnvironment>, RepositoryError> {
        let environment = sqlx::query_as::<_, ExecutionEnvironment>(
            r#"
            SELECT id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at
            FROM environments
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(environment)
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ExecutionEnvironment>, RepositoryError> {
        let environment = sqlx::query_as::<_, ExecutionEnvironment>(
            r#"
            SELECT id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at
            FROM environments
            WHERE name = $1
            "#
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(environment)
    }

    async fn list_by_status(&self, status: EnvironmentStatus) -> Result<Vec<ExecutionEnvironment>, RepositoryError> {
        let environments = sqlx::query_as::<_, ExecutionEnvironment>(
            r#"
            SELECT id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at
            FROM environments
            WHERE status = $1
            ORDER BY created_at DESC
            "#
        )
        .bind(&status)
        .fetch_all(&self.pool)
        .await?;

        Ok(environments)
    }

    async fn list_all(&self) -> Result<Vec<ExecutionEnvironment>, RepositoryError> {
        let environments = sqlx::query_as::<_, ExecutionEnvironment>(
            r#"
            SELECT id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at
            FROM environments
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(environments)
    }

    async fn update(&self, id: Uuid, input: UpdateEnvironmentInput) -> Result<ExecutionEnvironment, RepositoryError> {
        // Build dynamic UPDATE query based on provided fields
        let mut query = String::from("UPDATE environments SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if input.description.is_some() {
            bind_count += 1;
            query.push_str(&format!(", description = ${}", bind_count));
        }
        if input.driver.is_some() {
            bind_count += 1;
            query.push_str(&format!(", driver = ${}", bind_count));
        }
        if input.status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", status = ${}", bind_count));
        }
        if input.config.is_some() {
            bind_count += 1;
            query.push_str(&format!(", config = ${}", bind_count));
        }
        if input.env_vars.is_some() {
            bind_count += 1;
            query.push_str(&format!(", env_vars = ${}", bind_count));
        }
        if input.metadata.is_some() {
            bind_count += 1;
            query.push_str(&format!(", metadata = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, name, description, driver, status, config, env_vars, metadata, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, ExecutionEnvironment>(&query).bind(id);

        if let Some(name) = input.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(description) = input.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(driver) = input.driver {
            query_builder = query_builder.bind(driver);
        }
        if let Some(status) = input.status {
            query_builder = query_builder.bind(status);
        }
        if let Some(config) = input.config {
            query_builder = query_builder.bind(config);
        }
        if let Some(env_vars) = input.env_vars {
            query_builder = query_builder.bind(env_vars);
        }
        if let Some(metadata) = input.metadata {
            query_builder = query_builder.bind(metadata);
        }

        let environment = query_builder.fetch_one(&self.pool).await?;

        Ok(environment)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Soft delete: set status to archived
        sqlx::query(
            r#"
            UPDATE environments
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind(EnvironmentStatus::Archived)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_delete_blast_radius(&self, id: Uuid) -> Result<EnvironmentDeleteBlastRadius, RepositoryError> {
        let environment: Option<(String,)> = sqlx::query_as(
            "SELECT driver FROM environments WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some((driver,)) = environment else {
            return Err(RepositoryError::NotFound(id));
        };

        let affected_agents: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM agents WHERE metadata->>'environmentId' = $1 ORDER BY id",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let affected_issues: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM issues WHERE execution_workspace_settings->>'environmentId' = $1 ORDER BY id",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await?;
        let active_leases: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM environment_leases WHERE environment_id = $1 AND status = 'active' ORDER BY last_used_at DESC, created_at DESC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        let agent_default_count = affected_agents.len() as i32;
        let workspace_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_workspaces WHERE metadata->'config'->>'environmentId' = $1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let issue_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE execution_workspace_settings->>'environmentId' = $1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let project_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM projects WHERE env->>'environmentId' = $1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let secret_binding_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM company_secret_bindings WHERE target_type = 'environment' AND target_id = $1",
        )
        .bind(id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let is_instance_default: bool = sqlx::query_scalar(
            "SELECT COALESCE(general->>'defaultEnvironmentId', '') = $1 FROM instance_settings WHERE id = 1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(false);
        let pending_cleanup_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM environment_leases WHERE environment_id = $1 AND status = 'pending_cleanup'",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let reusable_lease_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM environment_leases WHERE environment_id = $1 AND lease_policy = 'reuse_by_environment' AND status IN ('active', 'released', 'retained')",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        let active_setup_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM environment_custom_image_setup_sessions WHERE environment_id = $1 AND status IN ('pending', 'running', 'starting', 'waiting_for_user', 'capturing')",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        let is_managed_local = driver == "local";
        let mut delete_blocked_reasons = Vec::new();
        let mut blocked_reasons = Vec::new();
        if is_managed_local {
            delete_blocked_reasons.push(EnvironmentDeleteBlockedReason::ManagedLocal);
            blocked_reasons.push("managed local environments cannot be deleted".to_string());
        }
        if is_instance_default {
            delete_blocked_reasons.push(EnvironmentDeleteBlockedReason::InstanceDefault);
            blocked_reasons.push("environment is the instance default".to_string());
        }
        if pending_cleanup_count > 0 {
            blocked_reasons.push(format!("{} pending sandbox cleanup lease(s) must be resolved first", pending_cleanup_count));
        }
        if reusable_lease_count > 0 {
            blocked_reasons.push(format!("{} reusable sandbox lease(s) must be released first", reusable_lease_count));
        }

        Ok(EnvironmentDeleteBlastRadius {
            environment_id: id,
            can_delete: delete_blocked_reasons.is_empty() && pending_cleanup_count == 0 && reusable_lease_count == 0,
            delete_blocked_reasons,
            blocked_reasons,
            affected_agents,
            affected_issues,
            active_leases: active_leases.clone(),
            static_references: EnvironmentStaticReferences {
                is_managed_local,
                is_instance_default,
                agent_default_count,
                execution_workspace_selection_count: workspace_count as i32,
                issue_selection_count: issue_count as i32,
                project_selection_count: project_count as i32,
                secret_binding_count: secret_binding_count as i32,
            },
            active_runtime_use: EnvironmentActiveRuntimeUse {
                active_lease_count: active_leases.len() as i32,
                active_custom_image_setup_session_count: active_setup_count as i32,
                has_active_runtime_use: !active_leases.is_empty() || active_setup_count > 0,
            },
        })
    }
}
