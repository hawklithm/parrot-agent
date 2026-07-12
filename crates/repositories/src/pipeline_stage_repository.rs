use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::pipeline::PipelineStage;

#[async_trait]
pub trait PipelineStageRepository: Send + Sync {
    async fn create(&self, stage: PipelineStage) -> RepositoryResult<PipelineStage>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<PipelineStage>>;
}

pub struct PostgresPipelineStageRepository {
    pool: PgPool,
}

impl PostgresPipelineStageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const STAGE_COLS: &str = "id, pipeline_id, key, name, kind, position, config, created_at, updated_at";

#[async_trait]
impl PipelineStageRepository for PostgresPipelineStageRepository {
    async fn create(&self, stage: PipelineStage) -> RepositoryResult<PipelineStage> {
        sqlx::query(
            r#"INSERT INTO pipeline_stages
               (id, pipeline_id, key, name, kind, position, config, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
        )
        .bind(stage.id)
        .bind(stage.pipeline_id)
        .bind(&stage.key)
        .bind(&stage.name)
        .bind(stage.kind)
        .bind(stage.position)
        .bind(&stage.config)
        .bind(stage.created_at)
        .bind(stage.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(stage)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<PipelineStage>> {
        let stage = sqlx::query_as::<_, PipelineStage>(
            &format!("SELECT {} FROM pipeline_stages WHERE id = $1", STAGE_COLS)
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(stage)
    }
}
