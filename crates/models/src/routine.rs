use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "routine_status", rename_all = "snake_case")]
pub enum RoutineStatus {
    Active,
    Paused,
    Draft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "concurrency_policy", rename_all = "snake_case")]
pub enum ConcurrencyPolicy {
    CoalesceIfActive,
    Parallel,
    SkipIfActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "catch_up_policy", rename_all = "snake_case")]
pub enum CatchUpPolicy {
    RunMissed,
    SkipMissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "trigger_kind", rename_all = "snake_case")]
pub enum TriggerKind {
    Schedule,
    Webhook,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "run_source", rename_all = "snake_case")]
pub enum RunSource {
    Schedule,
    Manual,
    Webhook,
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "run_status", rename_all = "snake_case")]
pub enum RunStatus {
    Received,
    Queued,
    Dispatched,
    Coalesced,
    Skipped,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineVariable {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub var_type: RoutineVariableType,
    pub default_value: Option<JsonValue>,
    pub required: bool,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineVariableType {
    Text,
    Number,
    Boolean,
    Select,
    Secret,
}

/// Routine trigger configuration (stored as JSONB)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineTriggerConfig {
    #[serde(flatten)]
    pub config: JsonValue,
}

/// Trigger type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "trigger_type", rename_all = "snake_case")]
pub enum TriggerType {
    Schedule,
    Webhook,
    Manual,
    Event,
    Cron,
}

/// Trigger status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "trigger_status", rename_all = "snake_case")]
pub enum TriggerStatus {
    Enabled,
    Disabled,
    Paused,
    Active,
    Failed,
    Configuration,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub parent_issue_id: Option<Uuid>,
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub agent_id: Uuid,
    pub assignee_agent_id: Uuid,
    pub priority: i32,
    pub status: RoutineStatus,
    pub concurrency_policy: ConcurrencyPolicy,
    pub catch_up_policy: CatchUpPolicy,
    pub trigger_config: JsonValue,
    pub variables: JsonValue,
    pub env: JsonValue,
    pub latest_revision_id: Option<Uuid>,
    pub latest_revision_number: i32,
    pub responsible_user_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub run_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub last_enqueued_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoutineTrigger {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub kind: TriggerKind,
    pub label: Option<String>,
    pub enabled: bool,
    pub trigger_type: TriggerType,
    pub config: JsonValue,
    pub status: TriggerStatus,
    pub next_trigger_at: Option<DateTime<Utc>>,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub public_id: Option<String>,
    pub secret_id: Option<String>,
    pub signing_mode: Option<String>,
    pub replay_window_sec: Option<i32>,
    pub last_rotated_at: Option<DateTime<Utc>>,
    pub last_result: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRevision {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub revision_number: i32,
    pub title: String,
    pub description: Option<String>,
    pub snapshot: JsonValue,
    pub change_summary: Option<String>,
    pub restored_from_revision_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRevisionSnapshotV1 {
    pub version: i32,
    pub routine_snapshot: JsonValue,
    pub triggers_snapshot: Vec<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRun {
    pub id: Uuid,
    pub company_id: Uuid,
    pub routine_id: Uuid,
    pub trigger_id: Option<Uuid>,
    pub source: RunSource,
    pub status: RunStatus,
    pub triggered_at: DateTime<Utc>,
    pub routine_revision_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub trigger_payload: Option<JsonValue>,
    pub dispatch_fingerprint: Option<String>,
    pub linked_issue_id: Option<Uuid>,
    pub coalesced_into_run_id: Option<Uuid>,
    pub failure_reason: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoutineInput {
    pub company_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub assignee_agent_id: Uuid,
    pub priority: i32,
    pub status: RoutineStatus,
    pub concurrency_policy: ConcurrencyPolicy,
    pub catch_up_policy: CatchUpPolicy,
    pub variables: Vec<RoutineVariable>,
    pub env: JsonValue,
    pub responsible_user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoutineInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<RoutineStatus>,
    pub priority: Option<i32>,
    pub assignee_agent_id: Option<Uuid>,
    pub concurrency_policy: Option<ConcurrencyPolicy>,
    pub catch_up_policy: Option<CatchUpPolicy>,
    pub variables: Option<Vec<RoutineVariable>>,
    pub env: Option<JsonValue>,
}

impl Routine {
    pub fn new(input: CreateRoutineInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            project_id: input.project_id,
            goal_id: input.goal_id,
            parent_issue_id: None,
            name: input.title.clone(),
            title: input.title,
            description: input.description,
            agent_id: input.assignee_agent_id,
            assignee_agent_id: input.assignee_agent_id,
            priority: input.priority,
            status: input.status,
            concurrency_policy: input.concurrency_policy,
            catch_up_policy: input.catch_up_policy,
            trigger_config: JsonValue::Object(serde_json::Map::new()),
            variables: serde_json::to_value(input.variables).unwrap_or(JsonValue::Array(vec![])),
            env: input.env,
            latest_revision_id: None,
            latest_revision_number: 0,
            responsible_user_id: input.responsible_user_id,
            created_by_user_id: input.responsible_user_id,
            last_run_at: None,
            next_run_at: None,
            run_count: 0,
            success_count: 0,
            failure_count: 0,
            last_triggered_at: None,
            last_enqueued_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl RoutineTrigger {
    pub fn new_schedule(company_id: Uuid, routine_id: Uuid, cron_expression: String, timezone: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            routine_id,
            kind: TriggerKind::Schedule,
            label: None,
            enabled: true,
            trigger_type: TriggerType::Schedule,
            config: JsonValue::Object(serde_json::Map::new()),
            status: TriggerStatus::Active,
            next_trigger_at: None,
            last_triggered_at: None,
            cron_expression: Some(cron_expression),
            timezone,
            next_run_at: None,
            last_fired_at: None,
            public_id: None,
            secret_id: None,
            signing_mode: None,
            replay_window_sec: None,
            last_rotated_at: None,
            last_result: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_webhook(company_id: Uuid, routine_id: Uuid) -> Self {
        let now = Utc::now();
        let public_id = format!("wh_{}", Uuid::new_v4().simple());
        let secret_id = format!("whsec_{}", Uuid::new_v4().simple());

        Self {
            id: Uuid::new_v4(),
            company_id,
            routine_id,
            kind: TriggerKind::Webhook,
            label: None,
            enabled: true,
            trigger_type: TriggerType::Webhook,
            config: JsonValue::Object(serde_json::Map::new()),
            status: TriggerStatus::Active,
            next_trigger_at: None,
            last_triggered_at: None,
            cron_expression: None,
            timezone: None,
            next_run_at: None,
            last_fired_at: None,
            public_id: Some(public_id),
            secret_id: Some(secret_id),
            signing_mode: Some("hmac_sha256".to_string()),
            replay_window_sec: Some(300),
            last_rotated_at: Some(now),
            last_result: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl RoutineRun {
    pub fn new(company_id: Uuid, routine_id: Uuid, source: RunSource) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id,
            routine_id,
            trigger_id: None,
            source,
            status: RunStatus::Received,
            triggered_at: now,
            routine_revision_id: None,
            idempotency_key: None,
            trigger_payload: None,
            dispatch_fingerprint: None,
            linked_issue_id: None,
            coalesced_into_run_id: None,
            failure_reason: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn mark_queued(&mut self) {
        self.status = RunStatus::Queued;
        self.updated_at = Utc::now();
    }

    pub fn mark_dispatched(&mut self, fingerprint: String) {
        self.status = RunStatus::Dispatched;
        self.dispatch_fingerprint = Some(fingerprint);
        self.updated_at = Utc::now();
    }

    pub fn mark_succeeded(&mut self) {
        self.status = RunStatus::Succeeded;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn mark_failed(&mut self, reason: String) {
        self.status = RunStatus::Failed;
        self.failure_reason = Some(reason);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn mark_coalesced(&mut self, into_run_id: Uuid) {
        self.status = RunStatus::Coalesced;
        self.coalesced_into_run_id = Some(into_run_id);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn mark_skipped(&mut self, reason: String) {
        self.status = RunStatus::Skipped;
        self.failure_reason = Some(reason);
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
