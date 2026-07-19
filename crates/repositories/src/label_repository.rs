use async_trait::async_trait;
use models::{CreateLabelInput, Label};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agent_repository::RepositoryResult;

#[async_trait]
pub trait LabelRepository: Send + Sync {
    /// List labels by company
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Label>>;

    /// Create a new label
    async fn create(&self, input: CreateLabelInput) -> RepositoryResult<Label>;

    /// Delete a label
    async fn delete(&self, id: Uuid) -> RepositoryResult<()>;

    /// Get label by ID
    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Option<Label>>;

    /// Get label by company and name
    async fn get_by_company_and_name(&self, company_id: Uuid, name: &str) -> RepositoryResult<Option<Label>>;
}

/// PostgreSQL implementation of LabelRepository
pub struct PgLabelRepository {
    pool: PgPool,
}

impl PgLabelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LabelRepository for PgLabelRepository {
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Label>> {
        let labels = sqlx::query_as::<_, Label>(
            r#"
            SELECT id, company_id, name, color, created_at
            FROM labels
            WHERE company_id = $1
            ORDER BY name ASC
            "#,
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(labels)
    }

    async fn create(&self, input: CreateLabelInput) -> RepositoryResult<Label> {
        let label = sqlx::query_as::<_, Label>(
            r#"
            INSERT INTO labels (company_id, name, color)
            VALUES ($1, $2, $3)
            RETURNING id, company_id, name, color, created_at
            "#,
        )
        .bind(input.company_id)
        .bind(&input.name)
        .bind(&input.color)
        .fetch_one(&self.pool)
        .await?;

        Ok(label)
    }

    async fn delete(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            DELETE FROM labels
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_by_id(&self, id: Uuid) -> RepositoryResult<Option<Label>> {
        let label = sqlx::query_as::<_, Label>(
            r#"
            SELECT id, company_id, name, color, created_at
            FROM labels
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(label)
    }

    async fn get_by_company_and_name(&self, company_id: Uuid, name: &str) -> RepositoryResult<Option<Label>> {
        let label = sqlx::query_as::<_, Label>(
            r#"
            SELECT id, company_id, name, color, created_at
            FROM labels
            WHERE company_id = $1 AND name = $2
            "#,
        )
        .bind(company_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(label)
    }
}
