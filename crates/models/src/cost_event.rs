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

/// IssueTreeCostSummary — Issue 树成本汇总（含子 Issue 递归聚合）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueTreeCostSummary {
    pub issue_id: Uuid,
    pub issue_count: i64,
    pub include_descendants: bool,
    pub cost_cents: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub run_count: i64,
    pub runtime_ms: f64,
}

/// RunSummaryRow — 运行汇总行（用于 issue_tree_cost_summary 的 heartbeat_runs 查询）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunSummaryRow {
    pub run_count: i64,
    pub runtime_ms: f64,
}

/// CostSummaryRow — 成本汇总行（通用聚合结果，支持按多种维度分组）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CostSummaryRow {
    pub dimension: String,
    pub total_cost_cents: i64,
    pub total_input_tokens: i64,
    pub total_cached_input_tokens: i64,
    pub total_output_tokens: i64,
    pub event_count: i64,
    pub api_run_count: i64,
    pub subscription_run_count: i64,
    pub subscription_cached_input_tokens: i64,
    pub subscription_input_tokens: i64,
    pub subscription_output_tokens: i64,
    pub provider_count: i64,
    pub model_count: i64,
}

/// Paperclip-compatible project cost aggregation.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CostByProjectRow {
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub cost_cents: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
}
