use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::pipeline::{PipelineCase, CaseEvent};

#[async_trait]
pub trait PipelineCaseRepository: Send + Sync {
    async fn create(&self, case: PipelineCase) -> RepositoryResult<PipelineCase>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<PipelineCase>>;
    async fn update(&self, case: PipelineCase) -> RepositoryResult<PipelineCase>;
    async fn find_by_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineCase>>;
    async fn find_by_pipeline_id(&self, pipeline_id: Uuid) -> RepositoryResult<Vec<PipelineCase>>;
    async fn find_by_parent_case_id(&self, parent_case_id: Uuid) -> RepositoryResult<Vec<PipelineCase>>;
    async fn find_events_by_case_id(&self, case_id: Uuid) -> RepositoryResult<Vec<CaseEvent>>;
    async fn create_event(&self, event: CaseEvent) -> RepositoryResult<CaseEvent>;
}

pub struct PostgresPipelineCaseRepository {
    pool: PgPool,
}

impl PostgresPipelineCaseRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const CASE_COLS: &str = "id, company_id, pipeline_id, stage_id, case_key, title, summary, fields, terminal_kind, version, pending_suggestion, created_at, updated_at";

const EVENT_COLS: &str = "id, case_id, event_type, payload, actor_type, actor_id, created_at";

#[async_trait]
impl PipelineCaseRepository for PostgresPipelineCaseRepository {
    async fn create(&self, case: PipelineCase) -> RepositoryResult<PipelineCase> {
        sqlx::query(
            r#"INSERT INTO pipeline_cases
               (id, company_id, pipeline_id, stage_id, case_key, title, summary, fields,
                terminal_kind, version, pending_suggestion, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#
        )
        .bind(case.id)
        .bind(case.company_id)
        .bind(case.pipeline_id)
        .bind(case.stage_id)
        .bind(&case.case_key)
        .bind(&case.title)
        .bind(&case.summary)
        .bind(&case.fields)
        .bind(case.terminal_kind)
        .bind(case.version)
        .bind(&case.pending_suggestion)
        .bind(case.created_at)
        .bind(case.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(case)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<PipelineCase>> {
        let case = sqlx::query_as::<_, PipelineCase>(
            &format!("SELECT {} FROM pipeline_cases WHERE id = $1", CASE_COLS)
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(case)
    }

    async fn update(&self, case: PipelineCase) -> RepositoryResult<PipelineCase> {
        sqlx::query(
            r#"UPDATE pipeline_cases
               SET stage_id = $2, title = $3, summary = $4, fields = $5,
                   terminal_kind = $6, version = $7, pending_suggestion = $8, updated_at = $9
               WHERE id = $1"#
        )
        .bind(case.id)
        .bind(case.stage_id)
        .bind(&case.title)
        .bind(&case.summary)
        .bind(&case.fields)
        .bind(case.terminal_kind)
        .bind(case.version)
        .bind(&case.pending_suggestion)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(case)
    }

    async fn find_by_stage_id(&self, stage_id: Uuid) -> RepositoryResult<Vec<PipelineCase>> {
        let cases = sqlx::query_as::<_, PipelineCase>(
            &format!("SELECT {} FROM pipeline_cases WHERE stage_id = $1 ORDER BY created_at DESC", CASE_COLS)
        )
        .bind(stage_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(cases)
    }

    async fn find_by_pipeline_id(&self, pipeline_id: Uuid) -> RepositoryResult<Vec<PipelineCase>> {
        let cases = sqlx::query_as::<_, PipelineCase>(
            &format!("SELECT {} FROM pipeline_cases WHERE pipeline_id = $1 ORDER BY created_at DESC", CASE_COLS)
        )
        .bind(pipeline_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(cases)
    }

    async fn find_by_parent_case_id(&self, parent_case_id: Uuid) -> RepositoryResult<Vec<PipelineCase>> {
        // Use fields JSONB to store parent_case_id relationship
        // Query: pipeline_cases WHERE fields->>'parent_case_id' = $1
        let cases = sqlx::query_as::<_, PipelineCase>(
            &format!("SELECT {} FROM pipeline_cases WHERE fields->>'parent_case_id' = $1 ORDER BY created_at ASC", CASE_COLS)
        )
        .bind(parent_case_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(cases)
    }

    async fn find_events_by_case_id(&self, case_id: Uuid) -> RepositoryResult<Vec<CaseEvent>> {
        let events = sqlx::query_as::<_, CaseEvent>(
            &format!("SELECT {} FROM pipeline_case_events WHERE case_id = $1 ORDER BY created_at ASC", EVENT_COLS)
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(events)
    }

    async fn create_event(&self, event: CaseEvent) -> RepositoryResult<CaseEvent> {
        sqlx::query(
            r#"INSERT INTO pipeline_case_events
               (id, case_id, event_type, payload, actor_type, actor_id, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#
        )
        .bind(event.id)
        .bind(event.case_id)
        .bind(&event.event_type)
        .bind(&event.payload)
        .bind(&event.actor_type)
        .bind(event.actor_id)
        .bind(event.created_at)
        .execute(&self.pool)
        .await?;
        Ok(event)
    }
}
