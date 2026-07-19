use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "budget_scope_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BudgetScopeType {
    Company,
    Agent,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "budget_window_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BudgetWindowKind {
    CalendarMonthUtc,
    Lifetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "budget_metric", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BudgetMetric {
    BilledCents,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "budget_threshold_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BudgetThresholdType {
    Soft,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "budget_incident_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum BudgetIncidentStatus {
    Open,
    Resolved,
    Dismissed,
}

// ============================================================================
// BudgetPolicy
// ============================================================================

/// 预算策略 — 对应 budget_policies 表
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPolicy {
    pub id: Uuid,
    pub company_id: Uuid,
    pub scope_type: BudgetScopeType,
    pub scope_id: Uuid,
    pub metric: BudgetMetric,
    pub window_kind: BudgetWindowKind,
    pub amount: i64,
    pub warn_percent: i32,
    pub hard_stop_enabled: bool,
    pub notify_enabled: bool,
    pub is_active: bool,
    pub created_by_user_id: Option<Uuid>,
    pub updated_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// BudgetIncident
// ============================================================================

/// 预算事件（阈值触发记录）— 对应 budget_incidents 表
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct BudgetIncident {
    pub id: Uuid,
    pub company_id: Uuid,
    pub policy_id: Uuid,
    pub scope_type: BudgetScopeType,
    pub scope_id: Uuid,
    pub metric: BudgetMetric,
    pub window_kind: BudgetWindowKind,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub threshold_type: BudgetThresholdType,
    pub amount_limit: i64,
    pub amount_observed: i64,
    pub status: BudgetIncidentStatus,
    pub approval_id: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
