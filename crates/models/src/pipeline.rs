use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

/// Pipeline stage kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "pipeline_stage_kind", rename_all = "snake_case")]
pub enum PipelineStageKind {
    Open,
    Working,
    Review,
    Done,
    Cancelled,
}

/// Terminal kind for pipeline cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "terminal_kind", rename_all = "snake_case")]
pub enum TerminalKind {
    Done,
    Cancelled,
}

/// Stage approver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageApprover {
    pub kind: ApproverKind,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproverKind {
    AnyHuman,
    User,
    Agent,
}

/// Pipeline stage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStageConfig {
    pub autonomy: Option<String>,
    pub auto_advance_on_children_terminal: Option<bool>,
    pub approve_to_stage_key: Option<String>,
    pub reject_to_stage_key: Option<String>,
    pub request_changes_to_stage_key: Option<String>,
    pub require_reject_reason: Option<bool>,
    pub require_request_changes_reason: Option<bool>,
    pub require_children_terminal: Option<bool>,
    pub require_no_unresolved_drift: Option<bool>,
    pub disabled: Option<bool>,
    pub require_approval: Option<bool>,
    pub approver: Option<StageApprover>,
    pub reviewer_kind: Option<String>,
    pub variables: Option<JsonValue>,
    pub automation: Option<JsonValue>,
    pub breakdown: Option<JsonValue>,
}

/// Pipeline model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Pipeline {
    pub id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub project_id: Option<Uuid>,
    pub enforce_transitions: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pipeline stage model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PipelineStage {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub key: String,
    pub name: String,
    pub kind: PipelineStageKind,
    pub position: i32,
    pub config: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pipeline case model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PipelineCase {
    pub id: Uuid,
    pub company_id: Uuid,
    pub pipeline_id: Uuid,
    pub stage_id: Uuid,
    pub case_key: String,
    pub title: String,
    pub summary: Option<String>,
    pub fields: JsonValue,
    pub terminal_kind: Option<TerminalKind>,
    pub version: i32,
    pub pending_suggestion: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Pipeline transition model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PipelineTransition {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub from_stage_id: Uuid,
    pub to_stage_id: Uuid,
    pub label: Option<String>,
    pub conditions: JsonValue,
}

/// Case event model for event sourcing
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseEvent {
    pub id: Uuid,
    pub case_id: Uuid,
    pub event_type: String,
    pub payload: JsonValue,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Input structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePipelineInput {
    pub company_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub project_id: Option<Uuid>,
    pub enforce_transitions: bool,
    pub stages: Vec<CreateStageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStageInput {
    pub key: String,
    pub name: String,
    pub kind: PipelineStageKind,
    pub position: i32,
    pub config: PipelineStageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaseInput {
    pub pipeline_id: Uuid,
    pub title: String,
    pub fields: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseAdvanceInput {
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub to_stage_key: String,
    pub actor_type: String,
    pub actor_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseReviewDecision {
    Approve,
    Reject,
    RequestChanges,[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseReviewInput {
    pub decision: CaseReviewDecision,
    pub reason: Option<String>,
    pub actor_type: String,
    pub actor_id: Uuid,
}

impl Pipeline {
    pub fn new(input: CreatePipelineInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            company_id: input.company_id,
            key: input.key,
            name: input.name,
            description: input.description,
            project_id: input.project_id,
            enforce_transitions: input.enforce_transitions,
            created_at: now,
            updated_at: now,
        }
    }
}

impl PipelineCase {
    pub fn new(company_id: Uuid, pipeline_id: Uuid, stage_id: Uuid, input: CreateCaseInput) -> Self {
        let now = Utc::now();
        let case_key = format!("CASE-{}", Uuid::new_v4().simple().to_string()[..8].to_uppercase());

        Self {
            id: Uuid::new_v4(),
            company_id,
            pipeline_id,
            stage_id,
            case_key,
            title: input.title,
            summary: None,
            fields: input.fields,
            tend: None,
            version: 1,
            pending_suggestion: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal_kind.is_some()
    }

    pub fn advance_to_stage(&mut self, new_stage_id: Uuid) {
        self.stage_id = new_stage_id;
        self.version += 1;
        self.updated_at = Utc::now();
    }

    pub fn mark_done(&mut self) {
        self.terminal_kind = Some(TerminalKind::Done);
        self.version += 1;
        self.updated_at = Utc::now();
    }

    pub fn mark_cancelled(&mut self) {
        self.terminal_kind = Some(TerminalKind::Cancelled);
        self.version += 1;
        self.updated_at = Utc::now();
    }
}

impl CaseEvent {
    pub fn new(case_id: Uuid, event_type: String, payload: JsonValue) -> Self {
        Self {
            id: Uuid::new_v4(),
            case_id,
            event_type,
            payload,
            actor_type: None,
            actor_id: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_actor(mut self, actor_type: String, actor_id: Uuid) -> Self {
        self.actor_type = Some(actor_type);
        self.actor_id = Some(actor_id);
        self
    }
}
