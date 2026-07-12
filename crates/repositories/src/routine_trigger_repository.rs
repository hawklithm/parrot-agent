use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::RepositoryResult;
use models::routine::{RoutineTrigger, TriggerKind};

#[async_trait]
pub trait RoutineTriggerRepository: Send + Sync {
    async fn create(&self, trigger: RoutineTrigger) -> RepositoryResult<RoutineTrigger>;
    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<RoutineTrigger>>;
    async fn find_by_routine_id(&self, routine_id: Uuid) -> RepositoryResult<Vec<RoutineTrigger>>;
    async fn find_by_type(&self, trigger_type: TriggerKind) -> RepositoryResult<Vec<RoutineTrigger>>;
    async fn find_enabled(&self) -> RepositoryResult<Vec<RoutineTrigger>>;
    async fn update(&self, trigger: RoutineTrigger) -> RepositoryResult<RoutineTrigger>;
    async fn delete(&self, id: Uuid) -> RepositoryResult<()>;
}

pub struct PostgresRoutineTriggerRepository {
    pool: PgPool,
}

impl PostgresRoutineTriggerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RoutineTriggerRepository for PostgresRoutineTriggerRepository {
    async fn create(&self, trigger: RoutineTrigger) -> RepositoryResult<RoutineTrigger> {
        sqlx::query(
            r#"INSERT INTO routine_triggers
               (id, company_id, routine_id, kind, label, enabled, cron_expression, timezone,
                next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec,
                last_rotated_at, last_result, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)"#
        )
        .bind(trigger.id)
        .bind(trigger.company_id)
        .bind(trigger.routine_id)
        .bind(trigger.kind)
        .bind(&trigger.label)
        .bind(trigger.enabled)
        .bind(&trigger.cron_expression)
        .bind(&trigger.timezone)
        .bind(trigger.next_run_at)
        .bind(trigger.last_fired_at)
        .bind(&trigger.public_id)
        .bind(&trigger.secret_id)
        .bind(&trigger.signing_mode)
        .bind(trigger.replay_window_sec)
        .bind(trigger.last_rotated_at)
        .bind(&trigger.last_result)
        .bind(trigger.created_at)
        .bind(trigger.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(trigger)
    }

    async fn find_by_id(&self, id: Uuid) -> RepositoryResult<Option<RoutineTrigger>> {
        let trigger = sqlx::query_as::<_, RoutineTrigger>(
            r#"SELECT id, company_id, routine_id, kind, label, enabled, cron_expression, timezone,
                      next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec,
                      last_rotated_at, last_result, created_at, updated_at
               FROM routine_triggers WHERE id = $1"#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(trigger)
    }

    async fn find_by_routine_id(&self, routine_id: Uuid) -> RepositoryResult<Vec<RoutineTrigger>> {
        let triggers = sqlx::query_as::<_, RoutineTrigger>(
            r#"SELECT id, company_id, routine_id, kind, label, enabled, cron_expression, timezone,
                      next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec,
                      last_rotated_at, last_result, created_at, updated_at
               FROM routine_triggers WHERE routine_id = $1 ORDER BY created_at ASC"#
        )
        .bind(routine_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(triggers)
    }

    async fn find_by_type(&self, trigger_type: TriggerKind) -> RepositoryResult<Vec<RoutineTrigger>> {
        let triggers = sqlx::query_as::<_, RoutineTrigger>(
            r#"SELECT id, company_id, routine_id, kind, label, enabled, cron_expression, timezone,
                      next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec,
                      last_rotated_at, last_result, created_at, updated_at
               FROM routine_triggers WHERE kind = $1 ORDER BY created_at ASC"#
        )
        .bind(trigger_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(triggers)
    }

    async fn find_enabled(&self) -> RepositoryResult<Vec<RoutineTrigger>> {
        let triggers = sqlx::query_as::<_, RoutineTrigger>(
            r#"SELECT id, company_id, routine_id, kind, label, enabled, cron_expression, timezone,
                      next_run_at, last_fired_at, public_id, secret_id, signing_mode, replay_window_sec,
                      last_rotated_at, last_result, created_at, updated_at
               FROM routine_triggers WHERE enabled = true ORDER BY next_run_at ASC NULLS LAST"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(triggers)
    }

    async fn update(&self, trigger: RoutineTrigger) -> RepositoryResult<RoutineTrigger> {
        sqlx::query(
            r#"UPDATE routine_triggers
               SET kind = $2, label = $3, enabled = $4, cron_expression = $5, timezone = $6,
                   next_run_at = $7, last_fired_at = $8, public_id = $9, secret_id = $10,
                   signing_mode = $11, replay_window_sec = $12, last_rotated_at = $13,
                   last_result = $14, updated_at = $15
               WHERE id = $1"#
        )
        .bind(trigger.id)
        .bind(trigger.kind)
        .bind(&trigger.label)
        .bind(trigger.enabled)
        .bind(&trigger.cron_expression)
        .bind(&trigger.timezone)
        .bind(trigger.next_run_at)
        .bind(trigger.last_fired_at)
        .bind(&trigger.public_id)
        .bind(&trigger.secret_id)
        .bind(&trigger.signing_mode)
        .bind(trigger.replay_window_sec)
        .bind(trigger.last_rotated_at)
        .bind(&trigger.last_result)
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(trigger)
    }

    async fn delete(&self, id: Uuid) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM routine_triggers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
