use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::pipeline::Pipeline;

#[async_trait]
pub trait PipelineRepository: Send + Sync {
    async fn create(&self, pipeline: Pipeline) -> RepositoryResult<Pipeline>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Pipeline>>;
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Pipeline>>;
}

pub struct PostgresPipelineRepository {
    pool: PgPool,
}

impl PostgresPipelineRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const PIPELINE_COLS: &str = "id, company_id, key, name, description, project_id, enforce_transitions, created_at, updated_at";

#[async_trait]
impl PipelineRepository for PostgresPipelineRepository {
    async fn create(&self, pipeline: Pipeline) -> RepositoryResult<Pipeline> {
        sqlx::query(
            r#"INSERT INTO pipelines
               (id, company_id, key, name, description, project_id, enforce_transitions, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(pipeline.id)
        .bind(pipeline.company_id)
        .bind(&pipeline.key)
        .bind(&pipeline.name)
        .bind(&pipeline.description)
        .bind(pipeline.project_id)
        .bind(pipeline.enforce_transitions)
        .bind(pipeline.created_at)
        .bind(pipeline.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(pipeline)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<Pipeline>> {
        let pipeline = sqlx::query_as::<_, Pipeline>(
            &format!("SELECT {} FROM pipelines WHERE id = $1", PIPELINE_COLS)
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pipeline)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Pipeline>> {
        let pipelines = sqlx::query_as::<_, Pipeline>(
            &format!("SELECT {} FROM pipelines WHERE company_id = $1 ORDER BY created_at DESC", PIPELINE_COLS)
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(pipelines)
    }
}
