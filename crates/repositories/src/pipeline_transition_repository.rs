use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::pipeline::PipelineTransition;

#[async_trait]
pub trait PipelineTransitionRepository: Send + Sync {
    async fn create(&self, transition: PipelineTransition) -> RepositoryResult<PipelineTransition>;
    async fn find_by_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>>;
    async fn find_by_from_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>>;
    async fn find_by_pipeline_id(&self, pipeline_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>>;
}

pub struct PostgresPipelineTransitionRepository {
    pool: PgPool,
}

impl PostgresPipelineTransitionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const TRANSITION_COLS: &str = "id, pipeline_id, from_stage_id, to_stage_id, label, conditions";

#[async_trait]
impl PipelineTransitionRepository for PostgresPipelineTransitionRepository {
    async fn create(&self, transition: PipelineTransition) -> RepositoryResult<PipelineTransition> {
        sqlx::query(
            r#"INSERT INTO pipeline_transitions
               (id, pipeline_id, from_stage_id, to_stage_id, label, conditions)
               VALUES ($1, $2, $3, $4, $5, $6)"#
        )
        .bind(transition.id)
        .bind(transition.pipeline_id)
        .bind(transition.from_stage_id)
        .bind(transition.to_stage_id)
        .bind(&transition.label)
        .bind(&transition.conditions)
        .execute(&self.pool)
        .await?;
        Ok(transition)
    }

    async fn find_by_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>> {
        let rows = sqlx::query_as::<_, PipelineTransition>(
            &format!("SELECT {} FROM pipeline_transitions WHERE to_stage_id = $1", TRANSITION_COLS)
        )
        .bind(stage_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn find_by_from_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>> {
        let rows = sqlx::query_as::<_, PipelineTransition>(
            &format!("SELECT {} FROM pipeline_transitions WHERE from_stage_id = $1", TRANSITION_COLS)
        )
        .bind(stage_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn find_by_pipeline_id(&self, pipeline_id: Uuid) -> RepositoryResult<Vec<PipelineTransition>> {
        let rows = sqlx::query_as::<_, PipelineTransition>(
            &format!("SELECT {} FROM pipeline_transitions WHERE pipeline_id = $1", TRANSITION_COLS)
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
