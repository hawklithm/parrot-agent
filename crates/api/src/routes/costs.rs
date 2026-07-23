//! Cost/Budget routes — 整域新增 (CO1-CO20)
//!
//! 对应 FEATURE_GAP_TASKS.md §3.2 Costs/Budgets

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use services::{CreateCostEventInput, CreateFinanceEventInput, BudgetIncidentResolveInput, UpsertPolicyInput};

/// Query parameter for issue tree summary
#[derive(Debug, Deserialize)]
pub struct ExcludeRootParams {
    pub exclude_root: Option<bool>,
}

/// Query parameter for project id
#[derive(Debug, Deserialize)]
pub struct ProjectIdParams {
    pub project_id: Option<Uuid>,
}

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
        .route("/issues/:id/cost-tree-summary", get(get_issue_tree_cost_summary))
        // Budgets
        .route("/companies/:company_id/budgets/overview", get(get_budget_overview))
        .route("/companies/:company_id/budgets/policies", get(list_budget_policies))
        .route("/companies/:company_id/budgets/policies", post(create_budget_policy))
        .route("/companies/:company_id/budgets", patch(update_budget))
        .route("/companies/:company_id/budget-incidents/:incident_id/resolve", post(resolve_budget_incident))
        // Budget invocation block
        .route("/companies/:company_id/budgets/invocation-block/:agent_id", get(get_invocation_block))
}

/// 时间范围查询参数
#[derive(Debug, Deserialize)]
pub struct TimeRangeParams {
    #[serde(alias = "from")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(alias = "to")]
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

impl TimeRangeParams {
    fn default_range() -> (DateTime<Utc>, DateTime<Utc>) {
        let end = Utc::now();
        let start = end - chrono::Duration::days(30);
        (start, end)
    }

    fn get_range(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        let (default_start, default_end) = Self::default_range();
        (
            self.start_time.unwrap_or(default_start),
            self.end_time.unwrap_or(default_end),
        )
    }
}

/// CO1: POST /companies/:company_id/cost-events
async fn record_cost_event(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateCostEventInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let event = state.cost_service.create_event(company_id, body).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(event).unwrap_or_default())))
}

/// CO2: POST /companies/:company_id/finance-events
async fn record_finance_event(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<CreateFinanceEventInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let event = state.finance_service.create_event(company_id, body).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(event).unwrap_or_default())))
}

/// CO3: GET /companies/:company_id/costs/summary
async fn get_cost_summary(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let summary = state.cost_service.get_summary(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "companyId": company_id,
        "spendCents": summary.spend_cents,
        "budgetCents": summary.budget_cents,
        "utilizationPercent": summary.utilization_percent,
        "periodStart": start,
        "periodEnd": end,
    })))
}

/// CO4: GET /companies/:company_id/costs/by-agent
async fn get_costs_by_agent(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let costs = state.cost_service.by_agent(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(costs).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO5: GET /companies/:company_id/costs/by-agent-model
async fn get_costs_by_agent_model(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let costs = state.cost_service.by_agent_model(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(costs).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO6: GET /companies/:company_id/costs/by-provider
async fn get_costs_by_provider(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let costs = state.cost_service.by_provider(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(costs).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO7: GET /companies/:company_id/costs/by-biller
async fn get_costs_by_biller(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let costs = state.cost_service.by_biller(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(costs).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO8: GET /companies/:company_id/costs/by-project
async fn get_costs_by_project(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let costs = state.cost_service.by_project(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(costs).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO9: GET /companies/:company_id/costs/window-spend
async fn get_window_spend(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(_params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let spend = state.cost_service.window_spend_multi(company_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(spend).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO10: GET /companies/:company_id/costs/quota-windows
async fn get_quota_windows(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let windows = state.cost_service.get_quota_windows(company_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(windows).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO11: GET /companies/:company_id/costs/finance-summary
async fn get_finance_summary(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let summary = state.finance_service.get_summary(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(summary).unwrap_or_else(|_| serde_json::json!({}))))
}

/// CO12: GET /companies/:company_id/costs/finance-by-biller
async fn get_finance_by_biller(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let rows = state.finance_service.by_biller(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO13: GET /companies/:company_id/costs/finance-by-kind
async fn get_finance_by_kind(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let (start, end) = params.get_range();
    let rows = state.finance_service.by_kind(company_id, start, end).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(rows).unwrap_or_else(|_| serde_json::json!([]))))
}

/// CO14: GET /companies/:company_id/costs/finance-events
async fn list_finance_events(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Query(params): Query<TimeRangeParams>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    let (start, end) = params.get_range();
    let events = state.finance_service.list_events(company_id, start, end, params.limit.unwrap_or(100)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let json: Vec<_> = events.into_iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(Json(json))
}

/// CO15: GET /issues/:id/cost-summary
async fn get_issue_cost_summary(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state.cost_service.issue_cost_summary(id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "issueId": id,
        "totalCostCents": summary.total_cost_cents,
        "totalInputTokens": summary.total_input_tokens,
        "totalOutputTokens": summary.total_output_tokens,
        "eventCount": summary.event_count,
    })))
}

/// CO15b: GET /issues/:id/cost-tree-summary
async fn get_issue_tree_cost_summary(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExcludeRootParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let exclude_root = params.exclude_root.unwrap_or(false);
    let company_id: Option<Uuid> = sqlx::query_scalar("SELECT company_id FROM issues WHERE id = $1")
        .bind(id).fetch_optional(&state.pool).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let company_id = company_id.ok_or(StatusCode::NOT_FOUND)?;
    let result = state.cost_service.issue_tree_summary(company_id, id, exclude_root).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}

/// CO16: GET /companies/:company_id/budgets/overview
async fn get_budget_overview(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let overview = state.budget_service.get_overview(company_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::to_value(overview).unwrap_or_default()))
}

/// CO16b: GET /companies/:company_id/budgets/policies
async fn list_budget_policies(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let policies = state.budget_service.list_policies(company_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"companyId": company_id, "policies": policies})))
}

/// CO16c: GET /companies/:company_id/budgets/invocation-block/:agent_id
async fn get_invocation_block(
    State(state): State<AppState>,
    Path((company_id, agent_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<ProjectIdParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let block = state.budget_service.get_invocation_block(company_id, agent_id, params.project_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"block": block})))
}

/// CO17: POST /companies/:company_id/budgets/policies
async fn create_budget_policy(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(body): Json<UpsertPolicyInput>,
) -> Result<impl IntoResponse, StatusCode> {
    let policy = state.budget_service.upsert_policy_full(company_id, body, None).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(policy).unwrap_or_default())))
}

/// CO18: PATCH /companies/:company_id/budgets
async fn update_budget(
    State(state): State<AppState>,
    Path(company_id): Path<Uuid>,
    Json(mut body): Json<UpsertPolicyInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    body.scope_type = "company".to_string();
    body.scope_id = company_id;
    let _ = state.budget_service.upsert_policy_full(company_id, body, None).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"companyId": company_id, "updated": true})))
}

/// CO20: POST /companies/:company_id/budget-incidents/:incident_id/resolve
#[derive(Debug, Deserialize)]
struct ResolveIncidentBody {
    resolution: String,
    resolved_by_user_id: Uuid,
    amount: Option<i64>,
    decision_note: Option<String>,
}
async fn resolve_budget_incident(
    State(state): State<AppState>,
    Path((company_id, incident_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ResolveIncidentBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let input = BudgetIncidentResolveInput {
        resolution: body.resolution,
        resolved_by_user_id: body.resolved_by_user_id,
        amount: body.amount,
        decision_note: body.decision_note,
    };
    state.budget_service.resolve_incident(company_id, incident_id, input).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"companyId": company_id, "incidentId": incident_id, "resolved": true})))
}
