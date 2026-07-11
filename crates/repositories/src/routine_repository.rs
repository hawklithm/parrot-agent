use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::routine::{Routine, RoutineRun, RoutineStatus, RoutineRunStatus};

#[async_trait]
pub trait RoutineRepository: Send + Sync {
    async fn create(&self, routine: Routine) -> RepositoryResult<Routine>;
    async fn get(&self, routine_id: Uuid) -> RepositoryResult<Option<Routine>>;
    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Routine>>;
    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Routine>>;
    async fn list_by_goal(&self, goal_id: Uuid) -> RepositoryResult<Vec<Routine>>;
    async fn update(&self, routine: Routine) -> RepositoryResult<Routine>;
    async fn delete(&self, routine_id: Uuid) -> RepositoryResult<()>;
    async fn list_pending_cron_routines(&self) -> RepositoryResult<Vec<Routine>>;

    async fn create_run(&self, run: RoutineRun) -> RepositoryResult<RoutineRun>;
    async fn get_run(&self, run_id: Uuid) -> RepositoryResult<Option<RoutineRun>>;
    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> RepositoryResult<Vec<RoutineRun>>;
    async fn update_run(&self, run: RoutineRun) -> RepositoryResult<RoutineRun>;
}

pub struct PostgresRoutineRepository {
    pool: PgPool,
}

impl PostgresRoutineRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoutineRepository for PostgresRoutineRepository {
    async fn create(&self, routine: Routine) -> RepositoryResult<Routine> {
        sqlx::query(
            r#"INSERT INTO routines
               (id, company_id, goal_id, agent_id, name, description, trigger_config,
                status, last_run_at, next_run_at, run_count, success_count, failure_count,
                created_at, updated_at, created_by_user_id)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#
        )
        .bind(routine.id)
        .bind(routine.company_id)
        .bind(routine.goal_id)
        .bind(routine.agent_id)
        .bind(&routine.name)
        .bind(&routine.description)
        .bind(&routine.trigger_config)
        .bind(&routine.status)
        .bind(routine.last_run_at)
        .bind(routine.next_run_at)
        .bind(routine.run_count)
        .bind(routine.success_count)
        .bind(routine.failure_count)
        .bind(routine.created_at)
        .bind(routine.updated_at)
        .bind(routine.created_by_user_id)
        .execute(&self.pool)
        .await?;
        Ok(routine)
    }

    async fn get(&self, routine_id: Uuid) -> RepositoryResult<Option<Routine>> {
        let routine = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, goal_id, agent_id, name, description, trigger_config,
                      status, last_run_at, next_run_at, run_count, success_count, failure_count,
                      created_at, updated_at, created_by_user_id
               FROM routines WHERE id = $1"#
        )
        .bind(routine_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(routine)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, goal_id, agent_id, name, description, trigger_config,
                      status, last_run_at, next_run_at, run_count, success_count, failure_count,
                      created_at, updated_at, created_by_user_id
               FROM routines WHERE company_id = $1 ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, goal_id, agent_id, name, description, trigger_config,
                      status, last_run_at, next_run_at, run_count, success_count, failure_count,
                      created_at, updated_at, created_by_user_id
               FROM routines WHERE agent_id = $1 ORDER BY created_at DESC"#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn list_by_goal(&self, goal_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, goal_id, agent_id, name, description, trigger_config,
                      status, last_run_at, next_run_at, run_count, success_count, failure_count,
                      created_at, updated_at, created_by_user_id
               FROM routines WHERE goal_id = $1 ORDER BY created_at DESC"#
        )
        .bind(goal_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn update(&self, routine: Routine) -> RepositoryResult<Routine> {
        sqlx::query(
            r#"UPDATE routines
               SET name = $2, description = $3, trigger_config = $4, status = $5,
                   last_run_at = $6, next_run_at = $7, run_count = $8,
                   success_count = $9, failure_count = $10, updated_at = $11
               WHERE id = $1"#
        )
        .bind(routine.id)
        .bind(&routine.name)
        .bind(&routine.description)
        .bind(&routine.trigger_config)
        .bind(&routine.status)
        .bind(routine.last_run_at)
        .bind(routine.next_run_at)
        .bind(routine.run_count)
        .bind(routine.success_count)
        .bind(routine.failure_count)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(routine)
    }

    async fn delete(&self, routine_id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM routines WHERE id = $1")
            .bind(routine_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_pending_cron_routines(&self) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, goal_id, agent_id, name, description, trigger_config,
                      status, last_run_at, next_run_at, run_count, success_count, failure_count,
                      created_at, updated_at, created_by_user_id
               FROM routines
               WHERE status = 'active'
                 AND next_run_at IS NOT NULL
                 AND next_run_at <= $1
               ORDER BY next_run_at ASC"#
        )
        .bind(Utc::now())
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn create_run(&self, run: RoutineRun) -> RepositoryResult<RoutineRun> {
        sqlx::query(
            r#"INSERT INTO routine_runs
               (id, routine_id, issue_id, status, trigger_source, started_at,
                completed_at, error_message, output, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(run.id)
        .bind(run.routine_id)
        .bind(run.issue_id)
        .bind(&run.status)
        .bind(&run.trigger_source)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(&run.error_message)
        .bind(&run.output)
        .bind(run.created_at)
        .execute(&self.pool)
        .await?;
        Ok(run)
    }

    async fn get_run(&self, run_id: Uuid) -> RepositoryResult<Option<RoutineRun>> {
        let run = sqlx::query_as::<_, RoutineRun>(
            r#"SELECT id, routine_id, issue_id, status, trigger_source, started_at,
                      completed_at, error_message, output, created_at
               FROM routine_runs WHERE id = $1"#
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(run)
    }

    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> RepositoryResult<Vec<RoutineRun>> {
        let runs = sqlx::query_as::<_, RoutineRun>(
            r#"SELECT id, routine_id, issue_id, status, trigger_source, started_at,
                      completed_at, error_message, output, created_at
               FROM routine_runs
               WHERE routine_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#
        )
        .bind(routine_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(runs)
    }

    async fn update_run(&self, run: RoutineRun) -> RepositoryResult<RoutineRun> {
        sqlx::query(
            r#"UPDATE routine_runs
               SET issue_id = $2, status = $3, started_at = $4, completed_at = $5,
                   error_message = $6, output = $7
               WHERE id = $1"#
        )
        .bind(run.id)
        .bind(run.issue_id)
        .bind(&run.status)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(&run.error_message)
        .bind(&run.output)
        .execute(&self.pool)
        .await?;
        Ok(run)
    }
}
