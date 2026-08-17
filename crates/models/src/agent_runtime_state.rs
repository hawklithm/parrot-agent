use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent Runtime State - 对齐 Paperclip 的 agent_runtime_state 表
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRuntimeState {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub adapter_type: String,
    pub session_id: Option<String>,
    pub session_display_id: Option<String>,
    pub session_params_json: Option<serde_json::Value>,
    pub state_json: serde_json::Value,
    pub last_run_id: Option<Uuid>,
    pub last_run_status: Option<String>,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_input_tokens: i64,
    pub total_cost_cents: i64,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Update Runtime State Input - 用于增量更新
#[derive(Debug, Clone)]
pub struct UpdateRuntimeStateInput {
    pub agent_id: Uuid,
    pub last_run_id: Option<Uuid>,
    pub last_run_status: Option<String>,
    pub input_tokens_delta: i64,
    pub output_tokens_delta: i64,
    pub cached_tokens_delta: i64,
    pub cost_cents_delta: i64,
    pub last_error: Option<String>,
}
