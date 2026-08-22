//! Company dashboard routes (#108).
//!
//! 对齐 Paperclip `routes/dashboard.ts` → `services/dashboard.ts` 的
//! `summary`：`GET /companies/:company_id/dashboard` 返回 agents（按状态桶）、
//! tasks（issue 状态桶）、costs（本月花费/预算/利用率）、pendingApprovals、
//! budgets（open incidents / 预算审批）与 runActivity（最近 14 天运行分布）。
//!
//! Parrot 差异说明：`heartbeat_runs` 无 `error_code` / `retry_of_run_id` 列，
//! 因此 runActivity 的 `failedByErrorCode` 为空、`recovered` 恒 0；
//! `budget_policies` 无 Paperclip 的 `paused` 列，因此 pausedAgents /
//! pausedProjects 恒 0（policy 级暂停语义未迁移）。

use crate::{app_state::AppState, errors::AppError};
use axum::{
    extract::{Extension, Path, State},
    routing::get,
    Json, Router,
};
use chrono::{Datelike, Duration, Utc};
use serde_json::{json, Value};
use services::auth::AuthorizationActor;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::routes::{require_company_access, AccessMode};

fn db_err(e: sqlx::Error) -> AppError {
    AppError::InternalServerError(e.to_string())
}

/// Paperclip `DASHBOARD_RUN_ACTIVITY_DAYS`.
const RUN_ACTIVITY_DAYS: i64 = 14;

pub fn dashboard_routes() -> Router<AppState> {
    Router::new().route(
        "/companies/:company_id/dashboard",
        get(get_dashboard_summary),
    )
}

async fn get_dashboard_summary(
    State(state): State<AppState>,
    Extension(actor): Extension<AuthorizationActor>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    require_company_access(&actor, company_id, AccessMode::Read).map_err(|_| {
        AppError::Forbidden("Dashboard company access denied".to_string())
    })?;
    let pool = &state.pool;

    let month_budget_cents = company_month_budget(pool, company_id).await?;
    let month_spend_cents = month_spend(pool, company_id).await?;
    let utilization = if month_budget_cents > 0 {
        (month_spend_cents as f64 / month_budget_cents as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(json!({
        "companyId": company_id,
        "agents": agent_buckets(pool, company_id).await?,
        "tasks": task_buckets(pool, company_id).await?,
        "costs": {
            "monthSpendCents": month_spend_cents,
            "monthBudgetCents": month_budget_cents,
            "monthUtilizationPercent": (utilization * 100.0).round() / 100.0,
        },
        "pendingApprovals": pending_approval_count(pool, company_id).await?,
        "budgets": budget_buckets(pool, company_id).await?,
        "runActivity": run_activity(pool, company_id).await?,
    })))
}

async fn company_month_budget(pool: &PgPool, company_id: Uuid) -> Result<i64, AppError> {
    let row = sqlx::query("SELECT budget_monthly_cents FROM companies WHERE id = $1")
        .bind(company_id)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::NotFound("Company not found".to_string()))?;
    Ok(row.try_get::<Option<i64>, _>("budget_monthly_cents").unwrap_or(None).unwrap_or(0))
}

/// Agent counts by operational bucket; `idle` agents are operational and count
/// as active (Paperclip).
async fn agent_buckets(pool: &PgPool, company_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT status::text AS status, COUNT(*)::bigint AS count \
         FROM agents WHERE company_id = $1 GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let mut buckets: serde_json::Map<String, Value> =
        ["active", "running", "paused", "error"].iter().map(|k| (k.to_string(), json!(0))).collect();
    for row in &rows {
        let status: String = row.get("status");
        let count: i64 = row.get("count");
        let bucket = if status == "idle" { "active" } else { status.as_str() };
        if let Some(slot) = buckets.get_mut(bucket) {
            *slot = json!(slot.as_i64().unwrap_or(0) + count);
        }
    }
    Ok(Value::Object(buckets))
}

/// Issue counts by task bucket: in_progress → inProgress, blocked → blocked,
/// done → done, everything else not done/cancelled counts as open.
async fn task_buckets(pool: &PgPool, company_id: Uuid) -> Result<Value, AppError> {
    let rows = sqlx::query(
        "SELECT status::text AS status, COUNT(*)::bigint AS count \
         FROM issues WHERE company_id = $1 AND hidden_at IS NULL GROUP BY status",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let mut buckets: serde_json::Map<String, Value> = ["open", "inProgress", "blocked", "done"]
        .iter()
        .map(|k| (k.to_string(), json!(0)))
        .collect();
    for row in &rows {
        let status: String = row.get("status");
        let count: i64 = row.get("count");
        match status.as_str() {
            "in_progress" => *buckets.get_mut("inProgress").unwrap() =
                json!(buckets["inProgress"].as_i64().unwrap() + count),
            "blocked" => *buckets.get_mut("blocked").unwrap() =
                json!(buckets["blocked"].as_i64().unwrap() + count),
            "done" => *buckets.get_mut("done").unwrap() =
                json!(buckets["done"].as_i64().unwrap() + count),
            "cancelled" => {}
            _ => *buckets.get_mut("open").unwrap() =
                json!(buckets["open"].as_i64().unwrap() + count),
        }
    }
    Ok(Value::Object(buckets))
}

async fn pending_approval_count(pool: &PgPool, company_id: Uuid) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM approvals WHERE company_id = $1 AND status = 'pending'",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    Ok(count)
}

/// Month-to-date spend. Parrot's `cost_events` has no `company_id`, so the sum
/// joins through `agents` (Paperclip's costEvents are company-scoped).
async fn month_spend(pool: &PgPool, company_id: Uuid) -> Result<i64, AppError> {
    let month_start = {
        let now = Utc::now();
        now.date_naive()
            .with_day(1)
            .unwrap_or_else(|| now.date_naive())
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc())
            .unwrap_or(now)
    };
    let cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ce.amount_cents), 0)::bigint \
         FROM cost_events ce JOIN agents a ON a.id = ce.agent_id \
         WHERE a.company_id = $1 AND ce.created_at >= $2",
    )
    .bind(company_id)
    .bind(month_start)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    Ok(cents)
}

/// Budget summary. Parrot's `budget_policies` carry no Paperclip `paused` flag,
/// so the paused counts are 0; active incidents and pending approvals are real.
async fn budget_buckets(pool: &PgPool, company_id: Uuid) -> Result<Value, AppError> {
    let active_incidents: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM budget_incidents WHERE company_id = $1 AND status = 'open'",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    let pending_approvals: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM budget_incidents bi \
         JOIN approvals ap ON ap.id = bi.approval_id \
         WHERE bi.company_id = $1 AND ap.status = 'pending'",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    Ok(json!({
        "activeIncidents": active_incidents,
        "pendingApprovals": pending_approvals,
        "pausedAgents": 0,
        "pausedProjects": 0,
    }))
}

/// Per-day run distribution over the trailing 14 days (UTC), newest first.
async fn run_activity(pool: &PgPool, company_id: Uuid) -> Result<Value, AppError> {
    let now = Utc::now();
    let window_start = now - Duration::days(RUN_ACTIVITY_DAYS - 1);
    let rows = sqlx::query(
        "SELECT to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS date, \
                status::text AS status, COUNT(*)::bigint AS count \
           FROM heartbeat_runs \
          WHERE company_id = $1 AND created_at >= $2 \
          GROUP BY date, status",
    )
    .bind(company_id)
    .bind(window_start.date_naive().and_hms_opt(0, 0, 0).map(|d| d.and_utc()).unwrap_or(window_start))
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

    let mut buckets: Vec<Value> = Vec::with_capacity(RUN_ACTIVITY_DAYS as usize);
    for offset in 0..RUN_ACTIVITY_DAYS {
        let day = (now - Duration::days(offset)).date_naive();
        buckets.push(json!({
            "date": day.format("%Y-%m-%d").to_string(),
            "succeeded": 0,
            "failed": 0,
            "recovered": 0,
            "other": 0,
            "total": 0,
            "failedByErrorCode": {},
        }));
    }
    for row in &rows {
        let date: String = row.get("date");
        let status: String = row.get("status");
        let count: i64 = row.get("count");
        let Some(bucket) = buckets
            .iter_mut()
            .find(|b| b["date"].as_str() == Some(date.as_str()))
        else {
            continue;
        };
        let obj = bucket.as_object_mut().expect("bucket is an object");
        let bump = |obj: &mut serde_json::Map<String, Value>, key: &str| {
            let next = obj.get(key).and_then(Value::as_i64).unwrap_or(0) + count;
            obj.insert(key.to_string(), json!(next));
        };
        match status.as_str() {
            "succeeded" => bump(obj, "succeeded"),
            "failed" | "timed_out" => bump(obj, "failed"),
            _ => bump(obj, "other"),
        }
        bump(obj, "total");
    }
    Ok(Value::Array(buckets))
}
