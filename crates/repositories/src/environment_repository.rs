use async_trait::async_trait;
use models::{
    ExecutionEnvironment, EnvironmentStatus, CreateEnvironmentInput,
    UpdateEnvironmentInput, EnvironmentDeleteBlastRadius,
    EnvironmentStaticReferences, EnvironmentActiveRuntimeUse,
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
        // Count active leases
        let active_leases: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM environment_leases
            WHERE environment_id = $1 AND status = 'active'
            "#
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        // Check for agents using this environment
        // TODO: This requires agents table to have environment_id field
        let _affected_agents: i64 = 0;

        // Check for issues using this environment
        // TODO: This requires issues table to have environment_id or execution_workspace_id field
        let _affected_issues: i64 = 0;

        let mut blocked_reasons = Vec::new();
        let can_delete = if active_leases.0 > 0 {
            blocked_reasons.push(format!("{} active lease(s) must be released first", active_leases.0));
            false
        } else {
            true
        };

        Ok(EnvironmentDeleteBlastRadius {
            environment_id: id,
            can_delete,
            delete_blocked_reasons: Vec::new(),
            blocked_reasons,
            affected_agents: Vec::new(),
            affected_issues: Vec::new(),
            active_leases: Vec::new(),
            static_references: EnvironmentStaticReferences {
                is_managed_local: false,
                is_instance_default: false,
                agent_default_count: 0,
                execution_workspace_selection_count: 0,
                issue_selection_count: 0,
                project_selection_count: 0,
                secret_binding_count: 0,
            },
            active_runtime_use: EnvironmentActiveRuntimeUse {
                active_lease_count: active_leases.0 as i32,
                active_custom_image_setup_session_count: 0,
                has_active_runtime_use: active_leases.0 > 0,
            },
        })
    }
}
