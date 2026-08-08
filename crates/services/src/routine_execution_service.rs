use chrono::{DateTime, Utc};
use models::AppError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Routine run source
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutineRunSource {
    Schedule,
    Manual,
    Api,
    Webhook,
}

impl RoutineRunSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoutineRunSource::Schedule => "schedule",
            RoutineRunSource::Manual => "manual",
            RoutineRunSource::Api => "api",
            RoutineRunSource::Webhook => "webhook",
        }
    }
}

/// Input for dispatching a routine run
#[derive(Debug, Clone)]
pub struct DispatchRoutineRunInput {
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: RoutineRunSource,
    pub payload: Option<serde_json::Value>,
    pub variables: Option<std::collections::HashMap<String, String>>,
    pub idempotency_key: Option<String>,
    pub project_id: Option<Uuid>,
    pub assignee_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub actor_agent_id: Option<Uuid>,
}

/// Routine run result
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRun {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: String,
    pub status: String,
    pub triggered_at: DateTime<Utc>,
    pub linked_issue_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Routine Execution Service
pub struct RoutineExecutionService {
    pool: PgPool,
}

impl RoutineExecutionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Dispatch a routine run
    /// Simplified implementation - full paperclip logic in server/src/services/routines.ts:1432-1749
    pub async fn dispatch_routine_run(
        &self,
        input: DispatchRoutineRunInput,
    ) -> Result<RoutineRun, AppError> {
        let triggered_at = Utc::now();
        let run_id = Uuid::new_v4();

        // Create the routine run record
        let source_str = input.source.as_str();
        sqlx::query(
            r#"
            INSERT INTO routine_runs (
                id, company_id, routine_id, trigger_id, source, status,
                triggered_aotency_key, created_at, updated_at
            )
            VALUES ($1, (SELECT company_id FROM routines WHERE id = $2), $2, $3, $4, $5, $6, $7, $6, $6)
            "#
        )
        .bind(run_id)
        .bind(input.routine_id)
        .bind(input.trigger_id)
        .bind(source_str)
        .bind("pending")
        .bind(triggered_at)
        .bind(&input.idempotency_key)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create routine run: {}", e)))?;

        // Fetch the created run
        let run = sqlx::query_as::<_, RoutineRun>(
            r#"
            SELECT 
                id, company_id, routine_id, trigger_id, source, status,
                triggered_at, linked_issue_id, idempotency_key, created_at, updated_at
            FROM routine_runs
            WHERE id = $1
            "#
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch routine run: {}", e)))?;

        Ok(run)
    }
}
