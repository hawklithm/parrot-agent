use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::routine::RoutineRevision;

#[async_trait]
pub trait RoutineRevisionRepository: Send + Sync {
    async fn create(&self, revision: RoutineRevision) -> RepositoryResult<RoutineRevision>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<RoutineRevision>>;
    async fn find_by_routine_id(&self, routine_id: Uuid) -> RepositoryResult<Vec<RoutineRevision>>;
}

pub struct PostgresRoutineRevisionRepository {
    pool: PgPool,
}

impl PostgresRoutineRevisionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoutineRevisionRepository for PostgresRoutineRevisionRepository {
    async fn create(&self, revision: RoutineRevision) -> RepositoryResult<RoutineRevision> {
        sqlx::query(
            r#"INSERT INTO routine_revisions
               (id, company_id, routine_id, revision_number, title, description, snapshot,
                change_summary, restored_from_revision_id, created_by_agent_id, created_by_user_id,
                created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#
        )
        .bind(revision.id)
        .bind(revision.company_id)
        .bind(revision.routine_id)
        .bind(revision.revision_number)
        .bind(&revision.title)
        .bind(&revision.description)
        .bind(&revision.snapshot)
        .bind(&revision.change_summary)
        .bind(revision.restored_from_revision_id)
        .bind(revision.created_by_agent_id)
        .bind(revision.created_by_user_id)
        .bind(revision.created_at)
        .execute(&self.pool)
        .await?;
        Ok(revision)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<RoutineRevision>> {
        let revision = sqlx::query_as::<_, RoutineRevision>(
            r#"SELECT id, company_id, routine_id, revision_number, title, description, snapshot,
                      change_summary, restored_from_revision_id, created_by_agent_id, created_by_user_id,
                      created_at
               FROM routine_revisions WHERE id = $1"#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(revision)
    }

    async fn find_by_routine_id(&self, routine_id: Uuid) -> RepositoryResult<Vec<RoutineRevision>> {
        let revisions = sqlx::query_as::<_, RoutineRevision>(
            r#"SELECT id, company_id, routine_id, revision_number, title, description, snapshot,
                      change_summary, restored_from_revision_id, created_by_agent_id, created_by_user_id,
                      created_at
               FROM routine_revisions WHERE routine_id = $1 ORDER BY revision_number DESC"#
        )
        .bind(routine_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(revisions)
    }
}
