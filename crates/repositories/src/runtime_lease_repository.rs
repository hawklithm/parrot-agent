use async_trait::async_trait;
use models::{
    RuntimeLease, EnvironmentLeaseStatus, EnvironmentLeasePolicy,
    CreateRuntimeLeaseInput, UpdateRuntimeLeaseInput,
};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

#[async_trait]
pub trait RuntimeLeaseRepository: Send + Sync {
    /// Create a new runtime lease
    async fn create(&self, input: CreateRuntimeLeaseInput) -> Result<RuntimeLease, RepositoryError>;

    /// Get a runtime lease by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<RuntimeLease>, RepositoryError>;

    /// List all leases for an environment
    async fn list_by_environment(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError>;

    /// List active leases for an environment
    async fn list_active_by_environment(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError>;

    /// Find a reusable lease for an environment
    async fn find_reusable_lease(&self, environment_id: Uuid) -> Result<Option<RuntimeLease>, RepositoryError>;

    /// Update a runtime lease
    async fn update(&self, id: Uuid, input: UpdateRuntimeLeaseInput) -> Result<RuntimeLease, RepositoryError>;

    /// Release a lease
    async fn release(&self, id: Uuid) -> Result<RuntimeLease, RepositoryError>;

    /// Mark expired leases
    async fn mark_expired(&self) -> Result<i64, RepositoryError>;

    /// Cleanup released/expired leases
    async fn cleanup(&self, lease_id: Uuid) -> Result<(), RepositoryError>;

    /// List leases by agent
    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError>;

    /// List leases by run
    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError>;

    /// List leases by issue
    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError>;
}

/// PostgreSQL implementation of RuntimeLeaseRepository
pub struct PgRuntimeLeaseRepository {
    pool: PgPool,
}

impl PgRuntimeLeaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RuntimeLeaseRepository for PgRuntimeLeaseRepository {
    async fn create(&self, input: CreateRuntimeLeaseInput) -> Result<RuntimeLease, RepositoryError> {
        let lease = sqlx::query_as::<_, RuntimeLease>(
            r#"
            INSERT INTO environment_leases (
                environment_id, agent_id, run_id, issue_id,
                policy, workspace_id, lease_metadata, expires_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, environment_id, agent_id, run_id, issue_id, status, policy,
                      workspace_id, lease_metadata, cleanup_status, cleanup_error,
                      acquired_at, released_at, expires_at, created_at, updated_at
            "#
        )
        .bind(&input.environment_id)
        .bind(&input.agent_id)
        .bind(&input.run_id)
        .bind(&input.issue_id)
        .bind(&input.policy)
        .bind(&input.workspace_id)
        .bind(&input.lease_metadata)
        .bind(&input.expires_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(lease)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<RuntimeLease>, RepositoryError> {
        let lease = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(lease)
    }

    async fn list_by_environment(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError> {
        let leases = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE environment_id = $1
            ORDER BY acquired_at DESC
            "#
        )
        .bind(environment_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(leases)
    }

    async fn list_active_by_environment(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError> {
        let leases = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE environment_id = $1 AND status = $2
            ORDER BY acquired_at DESC
            "#
        )
        .bind(environment_id)
        .bind(EnvironmentLeaseStatus::Active)
        .fetch_all(&self.pool)
        .await?;

        Ok(leases)
    }

    async fn find_reusable_lease(&self, environment_id: Uuid) -> Result<Option<RuntimeLease>, RepositoryError> {
        let lease = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE environment_id = $1
              AND status = $2
              AND policy = $3
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY acquired_at ASC
            LIMIT 1
            "#
        )
        .bind(environment_id)
        .bind(EnvironmentLeaseStatus::Active)
        .bind(EnvironmentLeasePolicy::Reusable)
        .fetch_optional(&self.pool)
        .await?;

        Ok(lease)
    }

    async fn update(&self, id: Uuid, input: UpdateRuntimeLeaseInput) -> Result<RuntimeLease, RepositoryError> {
        // Build dynamic UPDATE query
        let mut query = String::from("UPDATE environment_leases SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", status = ${}", bind_count));
        }
        if input.cleanup_status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", cleanup_status = ${}", bind_count));
        }
        if input.cleanup_error.is_some() {
            bind_count += 1;
            query.push_str(&format!(", cleanup_error = ${}", bind_count));
        }
        if input.released_at.is_some() {
            bind_count += 1;
            query.push_str(&format!(", released_at = ${}", bind_count));
        }
        if input.lease_metadata.is_some() {
            bind_count += 1;
            query.push_str(&format!(", lease_metadata = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, environment_id, agent_id, run_id, issue_id, status, policy, workspace_id, lease_metadata, cleanup_status, cleanup_error, acquired_at, released_at, expires_at, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, RuntimeLease>(&query).bind(id);

        if let Some(status) = input.status {
            query_builder = query_builder.bind(status);
        }
        if let Some(cleanup_status) = input.cleanup_status {
            query_builder = query_builder.bind(cleanup_status);
        }
        if let Some(cleanup_error) = input.cleanup_error {
            query_builder = query_builder.bind(cleanup_error);
        }
        if let Some(released_at) = input.released_at {
            query_builder = query_builder.bind(released_at);
        }
        if let Some(lease_metadata) = input.lease_metadata {
            query_builder = query_builder.bind(lease_metadata);
        }

        let lease = query_builder.fetch_one(&self.pool).await?;

        Ok(lease)
    }

    async fn release(&self, id: Uuid) -> Result<RuntimeLease, RepositoryError> {
        let lease = sqlx::query_as::<_, RuntimeLease>(
            r#"
            UPDATE environment_leases
            SET status = $1, released_at = NOW(), updated_at = NOW()
            WHERE id = $2
            RETURNING id, environment_id, agent_id, run_id, issue_id, status, policy,
                      workspace_id, lease_metadata, cleanup_status, cleanup_error,
                      acquired_at, released_at, expires_at, created_at, updated_at
            "#
        )
        .bind(EnvironmentLeaseStatus::Released)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(lease)
    }

    async fn mark_expired(&self) -> Result<i64, RepositoryError> {
        let result = sqlx::query(
            r#"
            UPDATE environment_leases
            SET status = $1, updated_at = NOW()
            WHERE status = $2 AND expires_at IS NOT NULL AND expires_at <= NOW()
            "#
        )
        .bind(EnvironmentLeaseStatus::Expired)
        .bind(EnvironmentLeaseStatus::Active)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as i64)
    }

    async fn cleanup(&self, lease_id: Uuid) -> Result<(), RepositoryError> {
        // Mark cleanup as in progress
        sqlx::query(
            r#"
            UPDATE environment_leases
            SET cleanup_status = _at = NOW()
            WHERE id = $2
            "#
        )
        .bind("in_progress")
        .bind(lease_id)
        .execute(&self.pool)
        .await?;

        // TODO: Actual cleanup logic (e.g., terminate workspace, cleanup resources)
        // For now, just mark as completed

        sqlx::query(
            r#"
            UPDATE environment_leases
            SET cleanup_status = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind("completed")
        .bind(lease_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError> {
        let leases = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE agent_id = $1
            ORDER BY acquired_at DESC
            "#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(leases)
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError> {
        let leases = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE run_id = $1
            ORDER BY acquired_at DESC
            "#
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(leases)
    }

    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<RuntimeLease>, RepositoryError> {
        let leases = sqlx::query_as::<_, RuntimeLease>(
            r#"
            SELECT id, environment_id, agent_id, run_id, issue_id, status, policy,
                   workspace_id, lease_metadata, cleanup_status, cleanup_error,
                   acquired_at, released_at, expires_at, created_at, updated_at
            FROM environment_leases
            WHERE issue_id = $1
            ORDER BY acquired_at DESC
            "#
        )
        .bind(issue_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(leases)
    }
}
