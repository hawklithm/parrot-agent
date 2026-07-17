use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::routine::{Routine, RoutineRun};

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
            r#"SELECT id, company_id, project_id, goal_id, parent_issue_id, title, description,
                      assignee_agent_id, priority, status, concurrency_policy, catch_up_policy,
                      variables, env, latest_revision_id, latest_revision_number, responsible_user_id,
                      last_triggered_at, last_enqueued_at, created_at, updated_at
               FROM routines WHERE id = $1"#
        )
        .bind(routine_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(routine)
    }

    async fn list_by_company(&self, company_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, project_id, goal_id, parent_issue_id, title, description,
                      assignee_agent_id, priority, status, concurrency_policy, catch_up_policy,
                      variables, env, latest_revision_id, latest_revision_number, responsible_user_id,
                      last_triggered_at, last_enqueued_at, created_at, updated_at
               FROM routines WHERE company_id = $1 ORDER BY created_at DESC"#
        )
        .bind(company_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn list_by_agent(&self, agent_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, project_id, goal_id, parent_issue_id, title, description,
                      assignee_agent_id, priority, status, concurrency_policy, catch_up_policy,
                      variables, env, latest_revision_id, latest_revision_number, responsible_user_id,
                      last_triggered_at, last_enqueued_at, created_at, updated_at
               FROM routines WHERE assignee_agent_id = $1 ORDER BY created_at DESC"#
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn list_by_goal(&self, goal_id: Uuid) -> RepositoryResult<Vec<Routine>> {
        let routines = sqlx::query_as::<_, Routine>(
            r#"SELECT id, company_id, project_id, goal_id, parent_issue_id, title, description,
                      assignee_agent_id, priority, status, concurrency_policy, catch_up_policy,
                      variables, env, latest_revision_id, latest_revision_number, responsible_user_id,
                      last_triggered_at, last_enqueued_at, created_at, updated_at
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
               SET title = $2, description = $3, status = $4, priority = $5,
                   assignee_agent_id = $6, concurrency_policy = $7, catch_up_policy = $8,
                   variables = $9, env = $10, latest_revision_id = $11,
                   latest_revision_number = $12, responsible_user_id = $13,
                   last_triggered_at = $14, last_enqueued_at = $15, updated_at = $16
               WHERE id = $1"#
        )
        .bind(routine.id)
        .bind(&routine.title)
        .bind(&routine.description)
        .bind(&routine.status)
        .bind(routine.priority)
        .bind(routine.assignee_agent_id)
        .bind(&routine.concurrency_policy)
        .bind(&routine.catch_up_policy)
        .bind(&routine.variables)
        .bind(&routine.env)
        .bind(routine.latest_revision_id)
        .bind(routine.latest_revision_number)
        .bind(routine.responsible_user_id)
        .bind(routine.last_triggered_at)
        .bind(routine.last_enqueued_at)
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
            r#"SELECT r.id, r.company_id, r.project_id, r.goal_id, r.parent_issue_id, r.title, r.description,
                      r.assignee_agent_id, r.priority, r.status, r.concurrency_policy, r.catch_up_policy,
                      r.variables, r.env, r.latest_revision_id, r.latest_revision_number, r.responsible_user_id,
                      r.last_triggered_at, r.last_enqueued_at, r.created_at, r.updated_at
               FROM routines r
               INNER JOIN routine_triggers rt ON r.id = rt.routine_id
               WHERE r.status = 'active'
                 AND rt.enabled = true
                 AND rt.kind = 'schedule'
                 AND rt.next_run_at IS NOT NULL
                 AND rt.next_run_at <= $1
               ORDER BY rt.next_run_at ASC"#
        )
        .bind(Utc::now())
        .fetch_all(&self.pool)
        .await?;
        Ok(routines)
    }

    async fn create_run(&self, run: RoutineRun) -> RepositoryResult<RoutineRun> {
        sqlx::query(
            r#"INSERT INTO routine_runs
               (id, company_id, routine_id, trigger_id, source, status, triggered_at,
                routine_revision_id, idempotency_key, trigger_payload, dispatch_fingerprint,
                linked_issue_id, coalesced_into_run_id, failure_reason, completed_at,
                created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)"#
        )
        .bind(run.id)
        .bind(run.company_id)
        .bind(run.routine_id)
        .bind(run.trigger_id)
        .bind(&run.source)
        .bind(&run.status)
        .bind(run.triggered_at)
        .bind(run.routine_revision_id)
        .bind(&run.idempotency_key)
        .bind(&run.trigger_payload)
        .bind(&run.dispatch_fingerprint)
        .bind(run.linked_issue_id)
        .bind(run.coalesced_into_run_id)
        .bind(&run.failure_reason)
        .bind(run.completed_at)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(run)
    }

    async fn get_run(&self, run_id: Uuid) -> RepositoryResult<Option<RoutineRun>> {
        let run = sqlx::query_as::<_, RoutineRun>(
            r#"SELECT id, company_id, routine_id, trigger_id, source, status, triggered_at,
                      routine_revision_id, idempotency_key, trigger_payload, dispatch_fingerprint,
                      linked_issue_id, coalesced_into_run_id, failure_reason, completed_at,
                      created_at, updated_at
               FROM routine_runs WHERE id = $1"#
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(run)
    }

    async fn list_runs(&self, routine_id: Uuid, limit: i64) -> RepositoryResult<Vec<RoutineRun>> {
        let runs = sqlx::query_as::<_, RoutineRun>(
            r#"SELECT id, company_id, routine_id, trigger_id, source, status, triggered_at,
                      routine_revision_id, idempotency_key, trigger_payload, dispatch_fingerprint,
                      linked_issue_id, coalesced_into_run_id, failure_reason, completed_at,
                      created_at, updated_at
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
               SET status = $2, dispatch_fingerprint = $3, linked_issue_id = $4,
                   coalesced_into_run_id = $5, failure_reason = $6, completed_at = $7,
                   updated_at = $8
               WHERE id = $1"#
        )
        .bind(run.id)
        .bind(&run.status)
        .bind(&run.dispatch_fingerprint)
        .bind(run.linked_issue_id)
        .bind(run.coalesced_into_run_id)
        .bind(&run.failure_reason)
        .bind(run.completed_at)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(run)
    }
}
