use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// CostEvent - 成本事件记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CostEvent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub billing_code: Option<String>,
    pub provider: String,
    pub biller: String,
    pub billing_type: String,
    pub model: String,
    pub input_tokens: i32,
    pub cached_input_tokens: i32,
    pub output_tokens: i32,
    pub cost_cents: i32,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// CostSummary - 成本汇总（按Agent聚合）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CostSummary {
    pub agent_id: Uuid,
    pub total_cost_cents: i32,
    pub total_input_tokens: i64,
    pub total_cached_input_tokens: i64,
    pub total_output_tokens: i64,
    pub event_count: i64,
}
