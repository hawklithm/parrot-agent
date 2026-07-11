use async_trait::async_trait;
use models::{
    ExecutionWorkspace, WorkspaceStatus, WorkspaceMode, WorkspaceStrategyType,
    CreateExecutionWorkspaceInput, UpdateExecutionWorkspaceInput,
};
use uuid::Uuid;
use sqlx::PgPool;
use crate::RepositoryError;

#[async_trait]
pub trait ExecutionWorkspaceRepository: Send + Sync {
    /// Create a new execution workspace
    async fn create(&self, input: CreateExecutionWorkspaceInput) -> Result<ExecutionWorkspace, RepositoryError>;

    /// Get workspace by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ExecutionWorkspace>, RepositoryError>;

    /// List workspaces by company
    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<ExecutionWorkspace>, RepositoryError>;

    /// List workspaces by project
    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ExecutionWorkspace>, RepositoryError>;

    /// List workspaces by issue
    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<ExecutionWorkspace>, RepositoryError>;

    /// List workspaces by status
    async fn list_by_status(&self, company_id: Uuid, status: WorkspaceStatus) -> Result<Vec<ExecutionWorkspace>, RepositoryError>;

    /// Update workspace
    async fn update(&self, id: Uuid, input: UpdateExecutionWorkspaceInput) -> Result<ExecutionWorkspace, RepositoryError>;

    /// Delete workspace (soft delete by setting status to Archived)
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Count workspaces by company
    async fn count_by_company(&self, company_id: Uuid) -> Result<i64, RepositoryError>;
}

/// PostgreSQL implementation of ExecutionWorkspaceRepository
pub struct PgExecutionWorkspaceRepository {
    pool: PgPool,
}

impl PgExecutionWorkspaceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ExecutionWorkspaceRepository for PgExecutionWorkspaceRepository {
    async fn create(&self, input: CreateExecutionWorkspaceInput) -> Result<ExecutionWorkspace, RepositoryError> {
        let workspace = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            INSERT INTO execution_workspaces (
                company_id, project_id, project_workspace_id, source_issue_id,
                name, mode, strategy_type, cwd, provider_ref, base_ref, branch_name, repo_url, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING id, company_id, project_id, project_workspace_id, source_issue_id,
                      name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                  metadata, created_at, updated_at
            "#
        )
        .bind(&input.company_id)
        .bind(&input.project_id)
        .bind(&input.project_workspace_id)
        .bind(&input.source_issue_id)
        .bind(&input.name)
        .bind(&input.mode)
        .bind(&input.strategy_type)
        .bind(&input.cwd)
        .bind(&input.provider_ref)
        .bind(&input.base_ref)
        .bind(&input.branch_name)
        .bind(&input.repo_url)
        .bind(&input.metadata)
        .fetch_one(&self.pool)
        .await?;

        Ok(workspace)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<ExecutionWorkspace>, RepositoryError> {
        let workspace = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                   name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                   metadata, created_at, updated_at
            FROM execution_workspaces
            WHERE id = $1
            "#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(workspace)
    }

    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<ExecutionWorkspace>, RepositoryError> {
        let workspaces = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                   name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                   metadata, created_at, updated_at
            FROM execution_workspaces
            WHERE company_id = $1 AND status != $2
            ORDER BY created_at DESC
            LIMIT $3 OFFSET $4
            "#
        )
        .bind(company_id)
        .bind(WorkspaceStatus::Archived)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(workspaces)
    }

    async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<ExecutionWorkspace>, RepositoryError> {
        let workspaces = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                   name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                   metadata, created_at, updated_at
            FROM execution_workspaces
            WHERE project_id = $1 AND status != $2
            ORDER BY created_at DESC
            "#
        )
        .bind(project_id)
        .bind(WorkspaceStatus::Archived)
        .fetch_all(&self.pool)
        .await?;

        Ok(workspaces)
    }

    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<ExecutionWorkspace>, RepositoryError> {
        let workspaces = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                   name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                   metadata, created_at, updated_at
            FROM execution_workspaces
            WHERE source_issue_id = $1 AND status != $2
            ORDER BY created_at DESC
            "#
        )
        .bind(issue_id)
        .bind(WorkspaceStatus::Archived)
        .fetch_all(&self.pool)
        .await?;

        Ok(workspaces)
    }

    async fn list_by_status(&self, company_id: Uuid, status: WorkspaceStatus) -> Result<Vec<ExecutionWorkspace>, RepositoryError> {
        let workspaces = sqlx::query_as::<_, ExecutionWorkspace>(
            r#"
            SELECT id, company_id, project_id, project_workspace_id, source_issue_id,
                   name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url,
                   metadata, created_at, updated_at
            FROM execution_workspaces
            WHERE company_id = $1 AND status = $2
            ORDER BY created_at DESC
            "#
        )
        .bind(company_id)
        .bind(status)
        .fetch_all(&self.pool)
        .await?;

        Ok(workspaces)
    }

    async fn update(&self, id: Uuid, input: UpdateExecutionWorkspaceInput) -> Result<ExecutionWorkspace, RepositoryError> {
        let mut query = String::from("UPDATE execution_workspaces SET updated_at = NOW()");
        let mut bind_count = 1;

        if input.name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", name = ${}", bind_count));
        }
        if input.status.is_some() {
            bind_count += 1;
            query.push_str(&format!(", status = ${}", bind_count));
        }
        if input.cwd.is_some() {
            bind_count += 1;
            query.push_str(&format!(", cwd = ${}", bind_count));
        }
        if input.provider_ref.is_some() {
            bind_count += 1;
            query.push_str(&format!(", provider_ref = ${}", bind_count));
        }
        if input.base_ref.is_some() {
            bind_count += 1;
            query.push_str(&format!(", base_ref = ${}", bind_count));
        }
        if input.branch_name.is_some() {
            bind_count += 1;
            query.push_str(&format!(", branch_name = ${}", bind_count));
        }
        if input.metadata.is_some() {
            bind_count += 1;
            query.push_str(&format!(", metadata = ${}", bind_count));
        }

        query.push_str(" WHERE id = $1 RETURNING id, company_id, project_id, project_workspace_id, source_issue_id, name, mode, strategy_type, status, cwd, provider_ref, base_ref, branch_name, repo_url, metadata, created_at, updated_at");

        let mut query_builder = sqlx::query_as::<_, ExecutionWorkspace>(&query).bind(id);

        if let Some(name) = input.name {
            query_builder = query_builder.bind(name);
        }
        if let Some(status) = input.status {
            query_builder = query_builder.bind(status);
        }
        if let Some(cwd) = input.cwd {
            query_builder = query_builder.bind(cwd);
        }
        if let Some(provider_ref) = input.provider_ref {
            query_builder = query_builder.bind(provider_ref);
        }
        if let Some(base_ref) = input.base_ref {
            query_builder = query_builder.bind(base_ref);
        }
        if let Some(branch_name) = input.branch_name {
            query_builder = query_builder.bind(branch_name);
        }
        if let Some(metadata) = input.metadata {
            query_builder = query_builder.bind(metadata);
        }

        let workspace = query_builder.fetch_one(&self.pool).await?;

        Ok(workspace)
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            UPDATE execution_workspaces
            SET status = $1, updated_at = NOW()
            WHERE id = $2
            "#
        )
        .bind(WorkspaceStatus::Archived)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn count_by_company(&self, company_id: Uuid) -> Result<i64, RepositoryError> {
        let count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM execution_workspaces
            WHERE company_id = $1 AND status != $2
            "#
        )
        .bind(company_id)
        .bind(WorkspaceStatus::Archived)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }
}
