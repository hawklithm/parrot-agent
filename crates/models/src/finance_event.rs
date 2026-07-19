use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "finance_direction", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum FinanceDirection {
    Debit,
    Credit,
}

// ============================================================================
// FinanceEvent
// ============================================================================

/// 财务事件 — 对应 finance_events 表
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct FinanceEvent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub cost_event_id: Option<Uuid>,
    pub biller: String,
    pub event_kind: String,
    pub direction: FinanceDirection,
    pub amount_cents: i32,
    pub currency: String,
    pub estimated: bool,
    pub description: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
