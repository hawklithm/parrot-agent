//! Cost/Budget routes — 整域新增 (CO1-CO20)
//!
//! 对应 FEATURE_GAP_TASKS.md §3.2 Costs/Budgets

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use uuid::Uuid;

use crate::app_state::AppState;

pub fn cost_routes() -> Router<AppState> {
    Router::new()
        // Cost events
        .route("/companies/:company_id/cost-events", post(record_cost_event))
        .route("/companies/:company_id/finance-events", post(record_finance_event))
        .route("/companies/:company_id/costs/summary", get(get_cost_summary))
        .route("/companies/:company_id/costs/by-agent", get(get_costs_by_agent))
        .route("/companies/:company_id/costs/by-agent-model", get(get_costs_by_agent_model))
        .route("/companies/:company_id/costs/by-provider", get(get_costs_by_provider))
        .route("/companies/:company_id/costs/by-biller", get(get_costs_by_biller))
        .route("/companies/:company_id/costs/by-project", get(get_costs_by_project))
        .route("/companies/:company_id/costs/window-spend", get(get_window_spend))
        .route("/companies/:company_id/costs/quota-windows", get(get_quota_windows))
        .route("/companies/:company_id/costs/finance-summary", get(get_finance_summary))
        .route("/companies/:company_id/costs/finance-by-biller", get(get_finance_by_biller))
        .route("/companies/:company_id/costs/finance-by-kind", get(get_finance_by_kind))
        .route("/companies/:company_id/costs/finance-events", get(list_finance_events))
        .route("/issues/:id/cost-summary", get(get_issue_cost_summary))
        // Budgets
        .route("/companies/:company_id/budgets/overview", get(get_budget_overview))
        .route("/companies/:company_id/budgets/policies", post(create_budget_policy))
        .route("/companies/:company_id/budgets", patch(update_budget))
        .route("/agents/:agent_id/budgets", patch(update_agent_budget))
        .route("/companies/:company_id/budget-incidents/:incident_id/resolve", post(resolve_budget_incident))
}

/// CO1: POST /companies/:company_id/cost-events
async fn record_cost_event(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "recorded": true})))
}

/// CO2: POST /companies/:company_id/finance-events
async fn record_finance_event(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "recorded": true})))
}

/// CO3: GET /companies/:company_id/costs/summary
async fn get_cost_summary(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "totalCostCents": 0,
        "periodStart": chrono::Utc::now(),
        "periodEnd": chrono::Utc::now(),
    })))
}

/// CO4: GET /companies/:company_id/costs/by-agent
async fn get_costs_by_agent(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "costs": []})))
}

/// CO5: GET /companies/:company_id/costs/by-agent-model
async fn get_costs_by_agent_model(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "costs": []})))
}

/// CO6: GET /companies/:company_id/costs/by-provider
async fn get_costs_by_provider(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "costs": []})))
}

/// CO7: GET /companies/:company_id/costs/by-biller
async fn get_costs_by_biller(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "costs": []})))
}

/// CO8: GET /companies/:company_id/costs/by-project
async fn get_costs_by_project(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "costs": []})))
}

/// CO9: GET /companies/:company_id/costs/window-spend
async fn get_window_spend(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "windowSpend": 0})))
}

/// CO10: GET /companies/:company_id/costs/quota-windows
async fn get_quota_windows(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "quotaWindows": []})))
}

/// CO11: GET /companies/:company_id/costs/finance-summary
async fn get_finance_summary(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "financeSummary": {}})))
}

/// CO12: GET /companies/:company_id/costs/finance-by-biller
async fn get_finance_by_biller(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "financeByBiller": []})))
}

/// CO13: GET /companies/:company_id/costs/finance-by-kind
async fn get_finance_by_kind(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "financeByKind": []})))
}

/// CO14: GET /companies/:company_id/costs/finance-events
async fn list_finance_events(
    State(_state): State<AppState>,
    Path(_company_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    Ok(Json(vec![]))
}

/// CO15: GET /issues/:id/cost-summary (already mapped in issues.rs)
async fn get_issue_cost_summary(
    State(_state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"issueId": id, "totalCostCents": 0})))
}

/// CO16: GET /companies/:company_id/budgets/overview
async fn get_budget_overview(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "totalBudgetCents": 100000,
        "spentCents": 0,
        "remainingCents": 100000,
    })))
}

/// CO17: POST /companies/:company_id/budgets/policies
async fn create_budget_policy(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "companyId": company_id,
        "policyId": Uuid::new_v4(),
        "created": true,
    }))))
}

/// CO18: PATCH /companies/:company_id/budgets
async fn update_budget(
    State(_state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "updated": true})))
}

/// CO19: PATCH /agents/:agent_id/budgets
async fn update_agent_budget(
    State(_state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"agentId": agent_id, "updated": true})))
}

/// CO20: POST /companies/:company_id/budget-incidents/:incident_id/resolve
async fn resolve_budget_incident(
    State(_state): State<AppState>,
    Path((company_id, incident_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({"companyId": company_id, "incidentId": incident_id, "resolved": true})))
}
