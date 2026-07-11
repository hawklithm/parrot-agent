use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "routine_trigger_type", rename_all = "snake_case")]
pub enum RoutineTriggerType {
    Cron,
    Manual,
    Webhook,
    Event,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "routine_status", rename_all = "snake_case")]
pub enum RoutineStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineTriggerConfig {
    pub trigger_type: RoutineTriggerType,
    pub cron_expression: Option<String>,
    pub webhook_secret: Option<String>,
    pub event_pattern: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Routine {
    pub id: Uuid,
    pub company_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub agent_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub trigger_config: sqlx::types::Json<RoutineTriggerConfig>,
    pub status: RoutineStatus,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub run_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "routine_run_status", rename_all = "snake_case")]
pub enum RoutineRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RoutineRun {
    pub id: Uuid,
    pub routine_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub status: RoutineRunStatus,
    pub trigger_source: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub output: Option<sqlx::types::Json<serde_json::Value>>,
    pub created_at: DateTime<Utc>,
}

impl Routine {
    pub fn new(
        company_id: Uuid,
        agent_id: Uuid,
        name: String,
        description: Option<String>,
        trigger_config: RoutineTriggerConfig,
        created_by_user_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            goal_id: None,
            agent_id,
            name,
            description,
            trigger_config: sqlx::types::Json(trigger_config),
            status: RoutineStatus::Active,
            last_run_at: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            failure_count: 0,
            created_at: now,
            updated_at: now,
            created_by_user_id,
        }
    }
}

impl RoutineRun {
    pub fn new(routine_id: Uuid, trigger_source: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            routine_id,
            issue_id: None,
            status: RoutineRunStatus::Pending,
            trigger_source,
            started_at: None,
            completed_at: None,
            error_message: None,
            output: None,
            created_at: Utc::now(),
        }
    }

    pub fn start(&mut self) {
        self.status = RoutineRunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self, output: Option<serde_json::Value>) {
        self.status = RoutineRunStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.output = output.map(sqlx::types::Json);
    }

    pub fn fail(&mut self, error_message: String) {
        self.status = RoutineRunStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error_message = Some(error_message);
    }
}
