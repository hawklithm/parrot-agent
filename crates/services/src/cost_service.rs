//! Cost / Budget / Finance services — 成本、预算、财务服务
//!
//! 对应 paperclip costs.ts + budgets.ts + finance.ts

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};
use models::budget::{
    BudgetIncident, BudgetIncidentStatus, BudgetMetric, BudgetPolicy as BudgetPolicyModel,
    BudgetScopeType, BudgetThresholdType, BudgetWindowKind,
};
use models::finance_event::{FinanceDirection, FinanceEvent as FinanceEventModel};
use repositories::agent_repository::AgentRepository;
use repositories::budget_repository::{
    BudgetIncidentRepository, BudgetPolicyRepository,
};
use repositories::company_repository::CompanyRepository;
use repositories::cost_event_repository::CostEventRepository;
use repositories::finance_event_repository::FinanceEventRepository;
use crate::server_adapter::AdapterRegistry;

// ============================================================================
// Data types
// ============================================================================

/// 成本事件（API 层使用）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostEventDto {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub cost_cents: i32,
    pub provider: String,
    pub model: String,
    pub biller: String,
    pub billing_type: String,
    pub input_tokens: i32,
    pub cached_input_tokens: i32,
    pub output_tokens: i32,
    pub occurred_at: DateTime<Utc>,
}

impl From<models::cost_event::CostEvent> for CostEventDto {
    fn from(e: models::cost_event::CostEvent) -> Self {
        Self {
            id: e.id,
            company_id: e.company_id,
            agent_id: e.agent_id,
            cost_cents: e.cost_cents,
            provider: e.provider,
            model: e.model,
            biller: e.biller,
            billing_type: e.billing_type,
            input_tokens: e.input_tokens,
            cached_input_tokens: e.cached_input_tokens,
            output_tokens: e.output_tokens,
            occurred_at: e.occurred_at,
        }
    }
}

/// 成本汇总（按维度聚合）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSummaryDto {
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

impl From<models::cost_event::CostSummaryRow> for CostSummaryDto {
    fn from(r: models::cost_event::CostSummaryRow) -> Self {
        Self {
            dimension: r.dimension,
            total_cost_cents: r.total_cost_cents,
            total_input_tokens: r.total_input_tokens,
            total_cached_input_tokens: r.total_cached_input_tokens,
            total_output_tokens: r.total_output_tokens,
            event_count: r.event_count,
            api_run_count: r.api_run_count,
            subscription_run_count: r.subscription_run_count,
            subscription_cached_input_tokens: r.subscription_cached_input_tokens,
            subscription_input_tokens: r.subscription_input_tokens,
            subscription_output_tokens: r.subscription_output_tokens,
            provider_count: r.provider_count,
            model_count: r.model_count,
        }
    }
}

/// 成本摘要（含预算对比）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSummaryWithBudget {
    pub company_id: Uuid,
    pub spend_cents: i64,
    pub budget_cents: i64,
    pub utilization_percent: f64,
}

/// 多窗口花费条目（对应 paperclip windowSpend 返回结构）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSpendEntry {
    pub provider: String,
    pub biller: String,
    pub window: String,
    pub window_hours: i64,
    pub cost_cents: f64,
    pub input_tokens: f64,
    pub cached_input_tokens: f64,
    pub output_tokens: f64,
}

/// 窗口期花费（单窗口版本，保留向后兼容）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSpend {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_cost_cents: i64,
}

/// 配额窗口
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: Uuid,
    pub company_id: Uuid,
    pub budget_cents: i64,
    pub spent_cents: i64,
    pub remaining_cents: i64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// 预算策略摘要（对应 paperclip BudgetPolicySummary）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPolicySummary {
    pub policy_id: Uuid,
    pub company_id: Uuid,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub scope_name: String,
    pub metric: String,
    pub window_kind: String,
    pub amount: i64,
    pub observed_amount: i64,
    pub remaining_amount: i64,
    pub utilization_percent: f64,
    pub warn_percent: i32,
    pub hard_stop_enabled: bool,
    pub notify_enabled: bool,
    pub is_active: bool,
    pub status: String,
    pub paused: bool,
    pub pause_reason: Option<String>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}

/// 预算事件明细（对应 paperclip BudgetIncident）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetIncidentDto {
    pub id: Uuid,
    pub company_id: Uuid,
    pub policy_id: Uuid,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub scope_name: String,
    pub metric: String,
    pub window_kind: String,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub threshold_type: String,
    pub amount_limit: i64,
    pub amount_observed: i64,
    pub status: String,
    pub approval_id: Option<Uuid>,
    pub approval_status: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 预算概览（对应 paperclip BudgetOverview）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetOverview {
    pub company_id: Uuid,
    pub policies: Vec<BudgetPolicySummary>,
    pub active_incidents: Vec<BudgetIncidentDto>,
    pub paused_agent_count: i64,
    pub paused_project_count: i64,
    pub pending_approval_count: i64,
}

/// 预算策略（API 层使用，旧版本保留兼容）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPolicy {
    pub id: Uuid,
    pub company_id: Uuid,
    pub max_monthly_cents: i64,
    pub alert_threshold_percent: f64,
    pub notify_agent_ids: Vec<Uuid>,
}

/// 预算事件（财务事件）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceEventDto {
    pub id: Uuid,
    pub company_id: Uuid,
    pub biller: String,
    pub kind: String,
    pub amount_cents: i32,
    pub description: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl From<FinanceEventModel> for FinanceEventDto {
    fn from(e: FinanceEventModel) -> Self {
        Self {
            id: e.id,
            company_id: e.company_id,
            biller: e.biller,
            kind: e.event_kind,
            amount_cents: e.amount_cents,
            description: e.description,
            occurred_at: e.occurred_at,
        }
    }
}

/// 财务摘要
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSummaryDto {
    pub company_id: Uuid,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub net_cents: i64,
    pub estimated_debit_cents: i64,
    pub event_count: i64,
}

impl From<repositories::finance_event_repository::FinanceSummaryRow> for FinanceSummaryDto {
    fn from(r: repositories::finance_event_repository::FinanceSummaryRow) -> Self {
        Self {
            company_id: Uuid::nil(),
            debit_cents: r.debit_cents,
            credit_cents: r.credit_cents,
            net_cents: r.debit_cents - r.credit_cents,
            estimated_debit_cents: r.estimated_debit_cents,
            event_count: r.event_count,
        }
    }
}

/// 财务摘要行（by_biller/by_kind 用，含额外维度字段）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceSummaryRowDto {
    pub dimension: String,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub net_cents: i64,
    pub estimated_debit_cents: i64,
    pub event_count: i64,
    pub kind_count: Option<i64>,
    pub biller_count: Option<i64>,
}

impl From<repositories::finance_event_repository::FinanceSummaryRow> for FinanceSummaryRowDto {
    fn from(r: repositories::finance_event_repository::FinanceSummaryRow) -> Self {
        Self {
            dimension: r.dimension.clone(),
            debit_cents: r.debit_cents,
            credit_cents: r.credit_cents,
            net_cents: r.debit_cents - r.credit_cents,
            estimated_debit_cents: r.estimated_debit_cents,
            event_count: r.event_count,
            kind_count: None,
            biller_count: None,
        }
    }
}

/// 预算执行作用域（对应 paperclip BudgetEnforcementScope）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetEnforcementScope {
    pub scope_type: String,
    pub scope_id: Uuid,
    pub scope_name: String,
    pub reason: String,
}

/// 创建成本事件输入
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCostEventInput {
    pub agent_id: Uuid,
    pub cost_cents: i32,
    pub provider: String,
    pub model: String,
    pub biller: String,
    pub billing_type: String,
    pub input_tokens: i32,
    pub cached_input_tokens: i32,
    pub output_tokens: i32,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub billing_code: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
}

/// 预算事件创建输入
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFinanceEventInput {
    pub agent_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
    pub cost_event_id: Option<Uuid>,
    pub biller: String,
    pub kind: String,
    pub amount_cents: i32,
    pub direction: Option<FinanceDirection>,
    pub currency: Option<String>,
    pub estimated: Option<bool>,
    pub description: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
}

/// 预算事件解决输入
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetIncidentResolveInput {
    pub resolution: String,
    pub resolved_by_user_id: Uuid,
    pub amount: Option<i64>,
    pub decision_note: Option<String>,
}

/// 预算策略创建/更新输入
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertPolicyInput {
    pub scope_type: String,
    pub scope_id: Uuid,
    pub metric: Option<String>,
    pub window_kind: Option<String>,
    pub amount: i64,
    pub warn_percent: Option<i32>,
    pub hard_stop_enabled: Option<bool>,
    pub notify_enabled: Option<bool>,
    pub is_active: Option<bool>,
}

// ============================================================================
// CostService
// ============================================================================

#[async_trait]
pub trait CostService: Send + Sync {
    /// 创建成本事件
    async fn create_event(&self, company_id: Uuid, input: CreateCostEventInput) -> ServiceResult<CostEventDto>;

    /// 获取成本摘要（含预算对比）
    async fn get_summary(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<CostSummaryWithBudget>;

    /// 按 Agent 聚合
    async fn by_agent(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>>;

    /// 按 Agent+Model 聚合
    async fn by_agent_model(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>>;

    /// 按 Provider 聚合
    async fn by_provider(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>>;

    /// 按 Biller 聚合
    async fn by_biller(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>>;

    /// 按 Project 聚合
    async fn by_project(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>>;

    /// 窗口期花费（多窗口按 provider 聚合，对应 paperclip windowSpend）
    async fn window_spend(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<WindowSpend>;

    /// 多窗口花费（5h/24h/7d 按 provider 分组）
    async fn window_spend_multi(&self, company_id: Uuid) -> ServiceResult<Vec<WindowSpendEntry>>;

    /// 获取配额窗口
    async fn get_quota_windows(&self, company_id: Uuid) -> ServiceResult<Vec<QuotaWindow>>;

    /// 获取 Issue 成本汇总
    async fn issue_cost_summary(&self, issue_id: Uuid) -> ServiceResult<CostSummaryDto>;

    /// 获取 Issue 树成本汇总（含子 Issue 递归聚合 + 运行次数/时间）
    async fn issue_tree_summary(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        exclude_root: bool,
    ) -> ServiceResult<serde_json::Value>;
}

// ============================================================================
// BudgetService
// ============================================================================

#[async_trait]
pub trait BudgetService: Send + Sync {
    /// 获取预算概览（含策略列表 + 活跃事件）
    async fn get_overview(&self, company_id: Uuid) -> ServiceResult<BudgetOverview>;

    /// 列出预算策略
    async fn list_policies(&self, company_id: Uuid) -> ServiceResult<Vec<BudgetPolicySummary>>;

    /// 创建/更新预算策略
    async fn upsert_policy(&self, company_id: Uuid, max_monthly_cents: i64, alert_threshold_percent: f64) -> ServiceResult<BudgetPolicy>;

    /// 创建/更新预算策略（完整参数版）
    async fn upsert_policy_full(&self, company_id: Uuid, input: UpsertPolicyInput, actor_user_id: Option<Uuid>) -> ServiceResult<BudgetPolicySummary>;

    /// 获取调用阻止（检查预算是否阻止新的工作）
    async fn get_invocation_block(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        project_id: Option<Uuid>,
    ) -> ServiceResult<Option<BudgetEnforcementScope>>;

    /// 解决预算事件（支持 raise_budget_and_resume / dismiss）
    async fn resolve_incident(&self, company_id: Uuid, incident_id: Uuid, input: BudgetIncidentResolveInput) -> ServiceResult<()>;

    /// 评估成本事件（每次创建成本事件后调用）
    async fn evaluate_cost_event(&self, company_id: Uuid, agent_id: Uuid, project_id: Option<Uuid>) -> ServiceResult<()>;
}

// ============================================================================
// FinanceService
// ============================================================================

#[async_trait]
pub trait FinanceService: Send + Sync {
    /// 创建财务事件
    async fn create_event(&self, company_id: Uuid, input: CreateFinanceEventInput) -> ServiceResult<FinanceEventDto>;

    /// 财务摘要
    async fn get_summary(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<FinanceSummaryDto>;

    /// 按 Biller 聚合财务
    async fn by_biller(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<FinanceSummaryRowDto>>;

    /// 按 Kind 聚合财务
    async fn by_kind(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<FinanceSummaryRowDto>>;

    /// 列出财务事件
    async fn list_events(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>, limit: i64) -> ServiceResult<Vec<FinanceEventDto>>;
}

// ============================================================================
// DefaultCostService
// ============================================================================

pub struct DefaultCostService {
    repo: Arc<dyn CostEventRepository>,
    agent_repo: Arc<dyn AgentRepository>,
    company_repo: Arc<CompanyRepository>,
    adapter_registry: Option<Arc<AdapterRegistry>>,
}

impl DefaultCostService {
    pub fn new(
        repo: Arc<dyn CostEventRepository>,
        agent_repo: Arc<dyn AgentRepository>,
        company_repo: Arc<CompanyRepository>,
    ) -> Self {
        Self { repo, agent_repo, company_repo, adapter_registry: None }
    }

    pub fn with_adapter_registry(mut self, adapter_registry: Arc<AdapterRegistry>) -> Self {
        self.adapter_registry = Some(adapter_registry);
        self
    }
}

#[async_trait]
impl CostService for DefaultCostService {
    async fn create_event(&self, company_id: Uuid, input: CreateCostEventInput) -> ServiceResult<CostEventDto> {
        // 1. Validate agent belongs to company
        let agent = self.agent_repo.get_by_id(input.agent_id).await
            .map_err(|e| ServiceError::NotFound(format!("Agent not found: {}", e)))?;
        if agent.company_id != company_id {
            return Err(ServiceError::Validation("Agent does not belong to company".to_string()));
        }

        // 2. Create the cost event
        use models::cost_event::CostEvent;
        let event = CostEvent {
            id: Uuid::new_v4(),
            company_id,
            agent_id: input.agent_id,
            issue_id: input.issue_id,
            project_id: input.project_id,
            goal_id: input.goal_id,
            heartbeat_run_id: input.heartbeat_run_id,
            billing_code: input.billing_code,
            provider: input.provider,
            biller: input.biller,
            billing_type: input.billing_type,
            model: input.model,
            input_tokens: input.input_tokens,
            cached_input_tokens: input.cached_input_tokens,
            output_tokens: input.output_tokens,
            cost_cents: input.cost_cents,
            occurred_at: input.occurred_at.unwrap_or_else(Utc::now),
            created_at: Utc::now(),
        };
        let saved = self.repo.create(event).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // 3. Update agent monthly spend
        let agent_spend = self.repo.current_month_spend_by_agent(input.agent_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // 4. Update company monthly spend
        let company_spend = self.repo.current_month_spend(company_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        // Update via direct SQL since CompanyRepository.update needs an UpdateCompanyInput
        // that doesn't expose spent_monthly_cents directly
        sqlx::query(
            "UPDATE companies SET spent_monthly_cents = $2, updated_at = NOW() WHERE id = $1"
        )
        .bind(company_id)
        .bind(company_spend as i64)
        .execute(&self.company_repo.pool)
        .await
        .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // Also update agent spent_monthly_cents if the column exists
        // We do this via raw SQL to avoid schema coupling issues
        let _ = sqlx::query(
            "UPDATE agents SET spent_monthly_cents = $2, updated_at = NOW() WHERE id = $1"
        )
        .bind(input.agent_id)
        .bind(agent_spend)
        .execute(&self.company_repo.pool)
        .await;

        let _ = sqlx::query("INSERT INTO activity_logs (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at) VALUES ($1, $2, 'cost.event_created', 'agent', $3, 'budget', $4, $5, NOW())")
            .bind(Uuid::new_v4()).bind(company_id).bind(input.agent_id).bind(saved.id)
            .bind(serde_json::json!({"costCents": saved.cost_cents, "provider": saved.provider, "billingType": saved.billing_type}))
            .execute(&self.company_repo.pool).await;

        Ok(CostEventDto::from(saved))
    }

    async fn get_summary(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<CostSummaryWithBudget> {
        // Get total spend
        let row = self.repo.summarize(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // Get company budget
        let company = self.company_repo.get_by_id(company_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        let budget_cents = company.and_then(|c| c.budget_monthly_cents).unwrap_or(0);

        let utilization = if budget_cents > 0 {
            (row.total_cost_cents as f64 / budget_cents as f64) * 100.0
        } else {
            0.0
        };

        Ok(CostSummaryWithBudget {
            company_id,
            spend_cents: row.total_cost_cents,
            budget_cents,
            utilization_percent: utilization,
        })
    }

    async fn by_agent(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>> {
        let rows = self.repo.by_agent(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(CostSummaryDto::from).collect())
    }

    async fn by_agent_model(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>> {
        let rows = self.repo.by_agent_model(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(CostSummaryDto::from).collect())
    }

    async fn by_provider(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>> {
        let rows = self.repo.by_provider(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(CostSummaryDto::from).collect())
    }

    async fn by_biller(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>> {
        let rows = self.repo.by_biller(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(CostSummaryDto::from).collect())
    }

    async fn by_project(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<CostSummaryDto>> {
        let rows = self.repo.by_project(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(CostSummaryDto::from).collect())
    }

    async fn window_spend(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<WindowSpend> {
        let total = self.repo.window_spend(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(WindowSpend {
            period_start: start_time,
            period_end: end_time,
            total_cost_cents: total,
        })
    }

    async fn window_spend_multi(&self, company_id: Uuid) -> ServiceResult<Vec<WindowSpendEntry>> {
        let now = Utc::now();
        let windows = [
            ("5h", 5i64),
            ("24h", 24i64),
            ("7d", 168i64),
        ];

        let mut results = Vec::new();
        for (label, hours) in windows {
            let since = now - chrono::Duration::hours(hours);
            let events = self.repo.list_by_company(company_id, since, now).await
                .map_err(|e| ServiceError::Repository(e.to_string()))?;
            let mut by_provider: std::collections::BTreeMap<String, (std::collections::BTreeSet<String>, i64, i64, i64, i64)> = std::collections::BTreeMap::new();
            for event in events {
                let entry = by_provider.entry(event.provider).or_default();
                entry.0.insert(event.biller);
                entry.1 += i64::from(event.cost_cents);
                entry.2 += i64::from(event.input_tokens);
                entry.3 += i64::from(event.cached_input_tokens);
                entry.4 += i64::from(event.output_tokens);
            }
            for (provider, (billers, cost_cents, input_tokens, cached_input_tokens, output_tokens)) in by_provider {
                results.push(WindowSpendEntry {
                    provider,
                    biller: if billers.len() == 1 { billers.into_iter().next().unwrap_or_default() } else { "mixed".to_string() },
                    window: label.to_string(),
                    window_hours: hours,
                    cost_cents: cost_cents as f64,
                    input_tokens: input_tokens as f64,
                    cached_input_tokens: cached_input_tokens as f64,
                    output_tokens: output_tokens as f64,
                });
            }
        }

        Ok(results)
    }

    async fn get_quota_windows(&self, company_id: Uuid) -> ServiceResult<Vec<QuotaWindow>> {
        let _ = company_id;
        let Some(registry) = &self.adapter_registry else { return Ok(vec![]); };
        let results = futures::future::join_all(registry.adapters().into_iter().map(|adapter| async move {
            tokio::time::timeout(std::time::Duration::from_secs(20), adapter.get_quota_windows()).await
        })).await;
        let mut windows = Vec::new();
        for result in results {
            if let Ok(Ok(adapter_windows)) = result {
                windows.extend(adapter_windows);
            }
        }
        Ok(windows)
    }

    async fn issue_cost_summary(&self, issue_id: Uuid) -> ServiceResult<CostSummaryDto> {
        let row = self.repo.issue_cost_summary(issue_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(CostSummaryDto::from(row))
    }

    async fn issue_tree_summary(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        exclude_root: bool,
    ) -> ServiceResult<serde_json::Value> {
        let result = self.repo.issue_tree_cost_summary(company_id, issue_id, exclude_root).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        Ok(serde_json::json!({
            "issueId": result.issue_id,
            "issueCount": result.issue_count,
            "includeDescendants": result.include_descendants,
            "costCents": result.cost_cents,
            "inputTokens": result.input_tokens,
            "cachedInputTokens": result.cached_input_tokens,
            "outputTokens": result.output_tokens,
            "runCount": result.run_count,
            "runtimeMs": result.runtime_ms,
        }))
    }
}

// ============================================================================
// DefaultBudgetService
// ============================================================================

pub struct DefaultBudgetService {
    cost_repo: Arc<dyn CostEventRepository>,
    policy_repo: Arc<dyn BudgetPolicyRepository>,
    incident_repo: Arc<dyn BudgetIncidentRepository>,
    company_repo: Arc<CompanyRepository>,
}

impl DefaultBudgetService {
    pub fn new(
        cost_repo: Arc<dyn CostEventRepository>,
        policy_repo: Arc<dyn BudgetPolicyRepository>,
        incident_repo: Arc<dyn BudgetIncidentRepository>,
        company_repo: Arc<CompanyRepository>,
    ) -> Self {
        Self { cost_repo, policy_repo, incident_repo, company_repo }
    }

    async fn log_budget_activity(&self, company_id: Uuid, event_type: &str, resource_id: Uuid, actor_id: Option<Uuid>, metadata: serde_json::Value) {
        let _ = sqlx::query(
            "INSERT INTO activity_logs (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at) VALUES ($1, $2, $3, $4, $5, 'budget', $6, $7, NOW())"
        )
        .bind(Uuid::new_v4())
        .bind(company_id)
        .bind(event_type)
        .bind(if actor_id.is_some() { "user" } else { "system" })
        .bind(actor_id.unwrap_or_else(Uuid::nil))
        .bind(resource_id)
        .bind(metadata)
        .execute(&self.company_repo.pool)
        .await;
    }

    async fn create_budget_override_approval(&self, policy: &BudgetPolicyModel, observed: i64, window_start: DateTime<Utc>, window_end: DateTime<Utc>) -> ServiceResult<Uuid> {
        let approval_id = Uuid::new_v4();
        let payload = serde_json::json!({
            "policyId": policy.id, "scopeType": format!("{:?}", policy.scope_type).to_lowercase(),
            "scopeId": policy.scope_id, "metric": "billed_cents", "windowKind": format!("{:?}", policy.window_kind).to_lowercase(),
            "thresholdType": "hard", "budgetAmount": policy.amount, "observedAmount": observed,
            "windowStart": window_start, "windowEnd": window_end,
            "guidance": "Raise the budget and resume the scope, or keep the scope paused."
        });
        sqlx::query("INSERT INTO approvals (id, company_id, approval_type, requested_by_agent_id, requested_by_user_id, status, payload, created_at, updated_at) VALUES ($1, $2, 'budget_override_required', NULL, NULL, 'pending', $3, NOW(), NOW())")
            .bind(approval_id).bind(policy.company_id).bind(payload)
            .execute(&self.company_repo.pool).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(approval_id)
    }

    /// Compute observed spend for a policy within its window
    async fn compute_observed_amount(&self, policy: &BudgetPolicyModel) -> ServiceResult<i64> {
        let now = Utc::now();
        let (start, end) = match policy.window_kind {
            BudgetWindowKind::Lifetime => (
                DateTime::<Utc>::from_timestamp(0, 0).unwrap_or(now),
                DateTime::<Utc>::from_timestamp_millis(253402300799000).unwrap_or(now),
            ),
            BudgetWindowKind::CalendarMonthUtc => {
                let year = now.year();
                let month = now.month();
                let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|d| d.and_utc())
                    .unwrap_or(now);
                let end = if month == 12 {
                    chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
                } else {
                    chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
                }
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                .map(|d| d.and_utc())
                .unwrap_or(now);
                (start, end)
            }
        };

        let total = self.cost_repo.window_spend(policy.company_id, start, end).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // If policy has a specific scope, we need more granular filtering
        // For now, return total company spend as approximation
        // TODO: Implement scope-aware aggregation when needed
        Ok(total)
    }

    /// Pause a scope for budget reasons
    async fn pause_scope(&self, policy: &BudgetPolicyModel) -> ServiceResult<()> {
        let now = Utc::now();
        match policy.scope_type {
            BudgetScopeType::Agent => {
                // Update agent status to paused
                let _ = sqlx::query(
                    "UPDATE agents SET status = 'paused', pause_reason = 'budget', paused_at = $2, updated_at = $2 WHERE id = $1 AND status IN ('active', 'idle', 'running', 'error')"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
            BudgetScopeType::Project => {
                let _ = sqlx::query(
                    "UPDATE projects SET pause_reason = 'budget', paused_at = $2, updated_at = $2 WHERE id = $1"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
            BudgetScopeType::Company => {
                let _ = sqlx::query(
                    "UPDATE companies SET status = 'paused', pause_reason = 'budget', paused_at = $2, updated_at = $2 WHERE id = $1"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
        }
        Ok(())
    }

    /// Resume a scope that was paused due to budget
    async fn resume_scope(&self, policy: &BudgetPolicyModel) -> ServiceResult<()> {
        let now = Utc::now();
        match policy.scope_type {
            BudgetScopeType::Agent => {
                let _ = sqlx::query(
                    "UPDATE agents SET status = 'idle', pause_reason = NULL, paused_at = NULL, updated_at = $2 WHERE id = $1 AND pause_reason = 'budget'"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
            BudgetScopeType::Project => {
                let _ = sqlx::query(
                    "UPDATE projects SET pause_reason = NULL, paused_at = NULL, updated_at = $2 WHERE id = $1 AND pause_reason = 'budget'"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
            BudgetScopeType::Company => {
                let _ = sqlx::query(
                    "UPDATE companies SET status = 'active', pause_reason = NULL, paused_at = NULL, updated_at = $2 WHERE id = $1 AND pause_reason = 'budget'"
                )
                .bind(policy.scope_id)
                .bind(now)
                .execute(&self.company_repo.pool)
                .await;
            }
        }
        Ok(())
    }

    /// Check if a scope is paused
    async fn check_scope_paused(&self, scope_type: &BudgetScopeType, scope_id: Uuid) -> ServiceResult<(String, bool, Option<String>)> {
        match scope_type {
            BudgetScopeType::Agent => {
                let row: Option<(String, Option<String>,)> = sqlx::query_as(
                    "SELECT name, pause_reason FROM agents WHERE id = $1"
                )
                .bind(scope_id)
                .fetch_optional(&self.company_repo.pool)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?;
                if let Some((name, pause_reason)) = row {
                    Ok((name, pause_reason.as_deref() == Some("budget"), pause_reason))
                } else {
                    Ok(("agent".to_string(), false, None))
                }
            }
            BudgetScopeType::Project => {
                let row: Option<(String, Option<String>,)> = sqlx::query_as(
                    "SELECT name, pause_reason FROM projects WHERE id = $1"
                )
                .bind(scope_id)
                .fetch_optional(&self.company_repo.pool)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?;
                if let Some((name, pause_reason)) = row {
                    Ok((name, pause_reason.as_deref() == Some("budget"), pause_reason))
                } else {
                    Ok(("project".to_string(), false, None))
                }
            }
            BudgetScopeType::Company => {
                let row: Option<(String, Option<String>,)> = sqlx::query_as(
                    "SELECT name, pause_reason FROM companies WHERE id = $1"
                )
                .bind(scope_id)
                .fetch_optional(&self.company_repo.pool)
                .await
                .map_err(|e| ServiceError::Repository(e.to_string()))?;
                if let Some((name, pause_reason)) = row {
                    Ok((name, pause_reason.as_deref() == Some("budget"), pause_reason))
                } else {
                    Ok(("company".to_string(), false, None))
                }
            }
        }
    }

    /// Build a policy summary from a policy row
    async fn build_policy_summary(&self, policy: &BudgetPolicyModel) -> ServiceResult<BudgetPolicySummary> {
        let observed_amount = self.compute_observed_amount(policy).await?;
        let (scope_name, paused, pause_reason) = self.check_scope_paused(&policy.scope_type, policy.scope_id).await?;

        let now = Utc::now();
        let (window_start, window_end) = match policy.window_kind {
            BudgetWindowKind::Lifetime => (
                DateTime::<Utc>::from_timestamp(0, 0).unwrap_or(now),
                DateTime::<Utc>::from_timestamp_millis(253402300799000).unwrap_or(now),
            ),
            BudgetWindowKind::CalendarMonthUtc => {
                let year = now.year();
                let month = now.month();
                let start = chrono::NaiveDate::from_ymd_opt(year, month, 1)
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|d| d.and_utc())
                    .unwrap_or(now);
                let end = if month == 12 {
                    chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
                } else {
                    chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
                }
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                .map(|d| d.and_utc())
                .unwrap_or(now);
                (start, end)
            }
        };

        let amount = if policy.is_active { policy.amount } else { 0 };
        let utilization_percent = if amount > 0 {
            (observed_amount as f64 / amount as f64) * 100.0
        } else {
            0.0
        };

        let status = if !policy.is_active || amount <= 0 {
            "ok"
        } else if observed_amount >= amount {
            "hard_stop"
        } else if observed_amount >= (policy.amount * policy.warn_percent as i64) / 100 {
            "warning"
        } else {
            "ok"
        };

        Ok(BudgetPolicySummary {
            policy_id: policy.id,
            company_id: policy.company_id,
            scope_type: format!("{:?}", policy.scope_type).to_lowercase(),
            scope_id: policy.scope_id,
            scope_name: normalize_scope_name(policy.scope_type, &scope_name),
            metric: "billed_cents".to_string(),
            window_kind: format!("{:?}", policy.window_kind).to_lowercase(),
            amount,
            observed_amount,
            remaining_amount: if amount > 0 { (amount - observed_amount).max(0) } else { 0 },
            utilization_percent,
            warn_percent: policy.warn_percent,
            hard_stop_enabled: policy.hard_stop_enabled,
            notify_enabled: policy.notify_enabled,
            is_active: policy.is_active,
            status: status.to_string(),
            paused,
            pause_reason,
            window_start,
            window_end,
        })
    }
}

fn normalize_scope_name(scope_type: BudgetScopeType, name: &str) -> String {
    match scope_type {
        BudgetScopeType::Company => name.to_string(),
        _ => {
            if name.trim().is_empty() {
                format!("{:?}", scope_type).to_lowercase()
            } else {
                name.to_string()
            }
        }
    }
}

#[async_trait]
impl BudgetService for DefaultBudgetService {
    async fn get_overview(&self, company_id: Uuid) -> ServiceResult<BudgetOverview> {
        // Get all policies
        let policies = self.policy_repo.list_by_company(company_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let policy_summaries: Vec<BudgetPolicySummary> = {
            let mut summaries = Vec::new();
            for p in &policies {
                summaries.push(self.build_policy_summary(p).await?);
            }
            summaries
        };

        // Get active incidents
        let incidents = self.incident_repo.list_by_company(company_id, Some(BudgetIncidentStatus::Open)).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let active_incidents: Vec<BudgetIncidentDto> = incidents.into_iter().map(|inc| {
            let scope_name = match inc.scope_type {
                BudgetScopeType::Company => "company".to_string(),
                BudgetScopeType::Agent => "agent".to_string(),
                BudgetScopeType::Project => "project".to_string(),
            };
            BudgetIncidentDto {
                id: inc.id,
                company_id: inc.company_id,
                policy_id: inc.policy_id,
                scope_type: format!("{:?}", inc.scope_type).to_lowercase(),
                scope_id: inc.scope_id,
                scope_name,
                metric: format!("{:?}", inc.metric).to_lowercase(),
                window_kind: format!("{:?}", inc.window_kind).to_lowercase(),
                window_start: inc.window_start,
                window_end: inc.window_end,
                threshold_type: format!("{:?}", inc.threshold_type).to_lowercase(),
                amount_limit: inc.amount_limit,
                amount_observed: inc.amount_observed,
                status: format!("{:?}", inc.status).to_lowercase(),
                approval_id: inc.approval_id,
                approval_status: None,
                resolved_at: inc.resolved_at,
                created_at: inc.created_at,
                updated_at: inc.updated_at,
            }
        }).collect();

        let paused_agent_count = policy_summaries.iter()
            .filter(|p| p.scope_type == "agent" && p.paused).count() as i64;
        let paused_project_count = policy_summaries.iter()
            .filter(|p| p.scope_type == "project" && p.paused).count() as i64;
        let pending_approval_count = active_incidents.iter()
            .filter(|i| i.approval_status.as_deref() == Some("pending")).count() as i64;

        Ok(BudgetOverview {
            company_id,
            policies: policy_summaries,
            active_incidents,
            paused_agent_count,
            paused_project_count,
            pending_approval_count,
        })
    }

    async fn list_policies(&self, company_id: Uuid) -> ServiceResult<Vec<BudgetPolicySummary>> {
        let policies = self.policy_repo.list_by_company(company_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let mut summaries = Vec::new();
        for p in &policies {
            summaries.push(self.build_policy_summary(p).await?);
        }
        Ok(summaries)
    }

    async fn upsert_policy(&self, company_id: Uuid, max_monthly_cents: i64, alert_threshold_percent: f64) -> ServiceResult<BudgetPolicy> {
        let now = Utc::now();
        let policy = BudgetPolicyModel {
            id: Uuid::new_v4(),
            company_id,
            scope_type: BudgetScopeType::Company,
            scope_id: company_id,
            metric: BudgetMetric::BilledCents,
            window_kind: BudgetWindowKind::CalendarMonthUtc,
            amount: max_monthly_cents,
            warn_percent: alert_threshold_percent as i32,
            hard_stop_enabled: true,
            notify_enabled: true,
            is_active: max_monthly_cents > 0,
            created_by_user_id: None,
            updated_by_user_id: None,
            created_at: now,
            updated_at: now,
        };

        let saved = self.policy_repo.upsert(&policy).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // Sync budget to company table
        sqlx::query(
            "UPDATE companies SET budget_monthly_cents = $2, updated_at = NOW() WHERE id = $1"
        )
        .bind(company_id)
        .bind(max_monthly_cents)
        .execute(&self.company_repo.pool)
        .await
        .map_err(|e| ServiceError::Repository(e.to_string()))?;

        Ok(BudgetPolicy {
            id: saved.id,
            company_id: saved.company_id,
            max_monthly_cents: saved.amount,
            alert_threshold_percent: saved.warn_percent as f64,
            notify_agent_ids: vec![],
        })
    }

    async fn upsert_policy_full(&self, company_id: Uuid, input: UpsertPolicyInput, actor_user_id: Option<Uuid>) -> ServiceResult<BudgetPolicySummary> {
        let scope_type = match input.scope_type.as_str() {
            "agent" => BudgetScopeType::Agent,
            "project" => BudgetScopeType::Project,
            _ => BudgetScopeType::Company,
        };

        // Verify scope belongs to company
        self.check_scope_paused(&scope_type, input.scope_id).await?;

        let metric = BudgetMetric::BilledCents;
        let window_kind = match input.window_kind.as_deref() {
            Some("lifetime") => BudgetWindowKind::Lifetime,
            _ => BudgetWindowKind::CalendarMonthUtc,
        };
        let amount = input.amount.max(0);
        let is_active = amount > 0 && input.is_active.unwrap_or(true);

        let now = Utc::now();
        let policy = BudgetPolicyModel {
            id: Uuid::new_v4(),
            company_id,
            scope_type,
            scope_id: input.scope_id,
            metric,
            window_kind,
            amount,
            warn_percent: input.warn_percent.unwrap_or(80),
            hard_stop_enabled: input.hard_stop_enabled.unwrap_or(true),
            notify_enabled: input.notify_enabled.unwrap_or(true),
            is_active,
            created_by_user_id: actor_user_id,
            updated_by_user_id: actor_user_id,
            created_at: now,
            updated_at: now,
        };

        let saved = self.policy_repo.upsert(&policy).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        // Sync budget to parent tables for company/agent scope with calendar_month_utc
        if scope_type == BudgetScopeType::Company && window_kind == BudgetWindowKind::CalendarMonthUtc {
            let _ = sqlx::query(
                "UPDATE companies SET budget_monthly_cents = $2, updated_at = NOW() WHERE id = $1"
            )
            .bind(company_id)
            .bind(amount)
            .execute(&self.company_repo.pool)
            .await;
        }

        // Resume or pause based on new amount
        let observed_amount = self.compute_observed_amount(&saved).await?;
        if amount > 0 && observed_amount < amount {
            self.resume_scope(&saved).await?;
        } else if amount > 0 && saved.hard_stop_enabled && observed_amount >= amount {
            self.pause_scope(&saved).await?;
        } else if amount == 0 {
            self.resume_scope(&saved).await?;
        }

        self.log_budget_activity(company_id, "budget.policy_upserted", saved.id, actor_user_id, serde_json::json!({
            "scopeType": format!("{:?}", saved.scope_type).to_lowercase(), "scopeId": saved.scope_id,
            "amount": saved.amount, "windowKind": format!("{:?}", saved.window_kind).to_lowercase()
        })).await;

        self.build_policy_summary(&saved).await
    }

    async fn get_invocation_block(
        &self,
        company_id: Uuid,
        agent_id: Uuid,
        project_id: Option<Uuid>,
    ) -> ServiceResult<Option<BudgetEnforcementScope>> {
        // 1. Check company-level pause
        let (company_name, company_paused, company_pause_reason) = self.check_scope_paused(&BudgetScopeType::Company, company_id).await?;
        if company_paused {
            let reason = if company_pause_reason.as_deref() == Some("budget") {
                "Company is paused because its budget hard-stop was reached.".to_string()
            } else {
                "Company is paused and cannot start new work.".to_string()
            };
            return Ok(Some(BudgetEnforcementScope {
                scope_type: "company".to_string(),
                scope_id: company_id,
                scope_name: company_name,
                reason,
            }));
        }

        // 2. Check company-level policy hard stop
        if let Ok(Some(policy)) = self.policy_repo.get_by_scope(company_id, BudgetScopeType::Company, company_id).await {
            if policy.hard_stop_enabled && policy.is_active && policy.amount > 0 {
                let observed = self.compute_observed_amount(&policy).await?;
                if observed >= policy.amount {
                    return Ok(Some(BudgetEnforcementScope {
                        scope_type: "company".to_string(),
                        scope_id: company_id,
                        scope_name: company_name,
                        reason: "Company cannot start new work because its budget hard-stop is exceeded.".to_string(),
                    }));
                }
            }
        }

        // 3. Check agent-level pause from budget
        let (agent_name, agent_paused, _) = self.check_scope_paused(&BudgetScopeType::Agent, agent_id).await?;
        if agent_paused {
            return Ok(Some(BudgetEnforcementScope {
                scope_type: "agent".to_string(),
                scope_id: agent_id,
                scope_name: agent_name,
                reason: "Agent is paused because its budget hard-stop was reached.".to_string(),
            }));
        }

        // 4. Check agent-level policy hard stop
        if let Ok(Some(policy)) = self.policy_repo.get_by_scope(company_id, BudgetScopeType::Agent, agent_id).await {
            if policy.hard_stop_enabled && policy.is_active && policy.amount > 0 {
                let observed = self.compute_observed_amount(&policy).await?;
                if observed >= policy.amount {
                    return Ok(Some(BudgetEnforcementScope {
                        scope_type: "agent".to_string(),
                        scope_id: agent_id,
                        scope_name: agent_name,
                        reason: "Agent cannot start because its budget hard-stop is still exceeded.".to_string(),
                    }));
                }
            }
        }

        // 5. Check project-level if applicable
        if let Some(pid) = project_id {
            let (project_name, project_paused, _) = self.check_scope_paused(&BudgetScopeType::Project, pid).await?;
            if let Ok(Some(policy)) = self.policy_repo.get_by_scope(company_id, BudgetScopeType::Project, pid).await {
                if policy.hard_stop_enabled && policy.is_active && policy.amount > 0 {
                    let observed = self.compute_observed_amount(&policy).await?;
                    if observed >= policy.amount {
                        return Ok(Some(BudgetEnforcementScope {
                            scope_type: "project".to_string(),
                            scope_id: pid,
                            scope_name: project_name,
                            reason: "Project cannot start work because its budget hard-stop is still exceeded.".to_string(),
                        }));
                    }
                }
            }

            if project_paused {
                return Ok(Some(BudgetEnforcementScope {
                    scope_type: "project".to_string(),
                    scope_id: pid,
                    scope_name: project_name,
                    reason: "Project is paused because its budget hard-stop was reached.".to_string(),
                }));
            }
        }

        Ok(None)
    }

    async fn resolve_incident(&self, company_id: Uuid, incident_id: Uuid, input: BudgetIncidentResolveInput) -> ServiceResult<()> {
        let incident = self.incident_repo.get_by_id(incident_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        if incident.company_id != company_id {
            return Err(ServiceError::NotFound("Budget incident not found".to_string()));
        }

        let action = input.resolution.as_str();

        if action == "raise_budget_and_resume" {
            // Get the policy
            if let Ok(policy) = self.policy_repo.get_by_id(incident.policy_id).await {
                let current_observed = self.compute_observed_amount(&policy).await?;
                let next_amount = input.amount.unwrap_or_else(|| (current_observed as f64 * 1.5) as i64).max(current_observed);

                // Update policy amount
                self.policy_repo.update_amount(incident.policy_id, next_amount, Some(input.resolved_by_user_id)).await
                    .map_err(|e| ServiceError::Repository(e.to_string()))?;

                // Sync to company if applicable
                if policy.scope_type == BudgetScopeType::Company && policy.window_kind == BudgetWindowKind::CalendarMonthUtc {
                    let _ = sqlx::query(
                        "UPDATE companies SET budget_monthly_cents = $2, updated_at = NOW() WHERE id = $1"
                    )
                    .bind(policy.scope_id)
                    .bind(next_amount)
                    .execute(&self.company_repo.pool)
                    .await;
                }

                // Resume the scope
                self.resume_scope(&policy).await?;

                // Resolve all open incidents for this policy
                self.incident_repo.resolve_open_for_policy(incident.policy_id).await
                    .map_err(|e| ServiceError::Repository(e.to_string()))?;
                if let Some(approval_id) = incident.approval_id {
                    sqlx::query("UPDATE approvals SET status = 'approved', decision_note = $2, decided_by_user_id = $3, decided_at = NOW(), updated_at = NOW() WHERE id = $1")
                        .bind(approval_id).bind(input.decision_note.as_deref().unwrap_or("Budget raised and scope resumed.")).bind(input.resolved_by_user_id)
                        .execute(&self.company_repo.pool).await
                        .map_err(|e| ServiceError::Repository(e.to_string()))?;
                }
            }
        } else {
            // Dismiss the incident
            self.incident_repo.dismiss(incident_id).await
                .map_err(|e| ServiceError::Repository(e.to_string()))?;
            if let Some(approval_id) = incident.approval_id {
                sqlx::query("UPDATE approvals SET status = 'rejected', decision_note = $2, decided_by_user_id = $3, decided_at = NOW(), updated_at = NOW() WHERE id = $1")
                    .bind(approval_id).bind(input.decision_note.as_deref().unwrap_or("Budget incident dismissed.")).bind(input.resolved_by_user_id)
                    .execute(&self.company_repo.pool).await
                    .map_err(|e| ServiceError::Repository(e.to_string()))?;
            }
        }

        self.log_budget_activity(company_id, "budget.incident_resolved", incident_id, Some(input.resolved_by_user_id), serde_json::json!({"resolution": input.resolution, "amount": input.amount, "decisionNote": input.decision_note})).await;

        Ok(())
    }

    async fn evaluate_cost_event(&self, company_id: Uuid, agent_id: Uuid, project_id: Option<Uuid>) -> ServiceResult<()> {
        // Get all active policies for this company
        let policies = self.policy_repo.list_active_by_company(company_id).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        for policy in policies {
            // Check if this policy is relevant to this cost event
            let is_relevant = match policy.scope_type {
                BudgetScopeType::Company => policy.scope_id == company_id,
                BudgetScopeType::Agent => policy.scope_id == agent_id,
                BudgetScopeType::Project => {
                    if let Some(pid) = project_id {
                        policy.scope_id == pid
                    } else {
                        false
                    }
                }
            };

            if !is_relevant || !policy.is_active || policy.amount <= 0 {
                continue;
            }

            let observed_amount = self.compute_observed_amount(&policy).await?;
            let soft_threshold = (policy.amount * policy.warn_percent as i64) / 100;

            // Check soft threshold
            if policy.notify_enabled && observed_amount >= soft_threshold {
                let existing = self.incident_repo.find_open(
                    policy.id,
                    chrono::NaiveDate::from_ymd_opt(Utc::now().year(), Utc::now().month(), 1)
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                        .map(|d| d.and_utc())
                        .unwrap_or_else(Utc::now),
                    BudgetThresholdType::Soft,
                ).await.map_err(|e| ServiceError::Repository(e.to_string()))?;

                if existing.is_none() {
                    let now = Utc::now();
                    let month_start = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                        .map(|d| d.and_utc())
                        .unwrap_or(now);
                    let month_end = if now.month() == 12 {
                        chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
                    } else {
                        chrono::NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
                    }
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|d| d.and_utc())
                    .unwrap_or(now);

                    let incident = BudgetIncident {
                        id: Uuid::new_v4(),
                        company_id,
                        policy_id: policy.id,
                        scope_type: policy.scope_type,
                        scope_id: policy.scope_id,
                        metric: policy.metric,
                        window_kind: policy.window_kind,
                        window_start: month_start,
                        window_end: month_end,
                        threshold_type: BudgetThresholdType::Soft,
                        amount_limit: policy.amount,
                        amount_observed: observed_amount,
                        status: BudgetIncidentStatus::Open,
                        approval_id: None,
                        resolved_at: None,
                        created_at: now,
                        updated_at: now,
                    };
                    if self.incident_repo.create(&incident).await.is_ok() {
                        self.log_budget_activity(company_id, "budget.soft_threshold_crossed", incident.id, None, serde_json::json!({"policyId": policy.id, "observedAmount": observed_amount})).await;
                    }
                }
            }

            // Check hard threshold
            if policy.hard_stop_enabled && observed_amount >= policy.amount {
                // Resolve any open soft incidents first
                let _ = self.incident_repo.resolve_open_soft_for_policy(policy.id).await;

                let existing = self.incident_repo.find_open(
                    policy.id,
                    chrono::NaiveDate::from_ymd_opt(Utc::now().year(), Utc::now().month(), 1)
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                        .map(|d| d.and_utc())
                        .unwrap_or_else(Utc::now),
                    BudgetThresholdType::Hard,
                ).await.map_err(|e| ServiceError::Repository(e.to_string()))?;

                if existing.is_none() {
                    let now = Utc::now();
                    let month_start = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                        .map(|d| d.and_utc())
                        .unwrap_or(now);
                    let month_end = if now.month() == 12 {
                        chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
                    } else {
                        chrono::NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
                    }
                    .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                    .map(|d| d.and_utc())
                    .unwrap_or(now);

                    let approval_id = self.create_budget_override_approval(&policy, observed_amount, month_start, month_end).await?;
                    let incident = BudgetIncident {
                        id: Uuid::new_v4(),
                        company_id,
                        policy_id: policy.id,
                        scope_type: policy.scope_type,
                        scope_id: policy.scope_id,
                        metric: policy.metric,
                        window_kind: policy.window_kind,
                        window_start: month_start,
                        window_end: month_end,
                        threshold_type: BudgetThresholdType::Hard,
                        amount_limit: policy.amount,
                        amount_observed: observed_amount,
                        status: BudgetIncidentStatus::Open,
                        approval_id: Some(approval_id),
                        resolved_at: None,
                        created_at: now,
                        updated_at: now,
                    };
                    if self.incident_repo.create(&incident).await.is_ok() {
                        self.log_budget_activity(company_id, "budget.hard_threshold_crossed", incident.id, None, serde_json::json!({"policyId": policy.id, "approvalId": approval_id, "observedAmount": observed_amount})).await;
                    }
                }

                // Pause the scope
                let _ = self.pause_scope(&policy).await;
            }
        }

        Ok(())
    }
}

// ============================================================================
// DefaultFinanceService
// ============================================================================

pub struct DefaultFinanceService {
    repo: Arc<dyn FinanceEventRepository>,
    _company_repo: Arc<CompanyRepository>,
}

impl DefaultFinanceService {
    pub fn new(
        repo: Arc<dyn FinanceEventRepository>,
        company_repo: Arc<CompanyRepository>,
    ) -> Self {
        Self { repo, _company_repo: company_repo }
    }

    async fn assert_belongs_to_company(&self, table: &str, id: Uuid, company_id: Uuid, label: &str) -> ServiceResult<()> {
        let sql = match table {
            "agents" => "SELECT company_id FROM agents WHERE id = $1",
            "issues" => "SELECT company_id FROM issues WHERE id = $1",
            "projects" => "SELECT company_id FROM projects WHERE id = $1",
            "goals" => "SELECT company_id FROM goals WHERE id = $1",
            "heartbeat_runs" => "SELECT company_id FROM heartbeat_runs WHERE id = $1",
            "cost_events" => "SELECT company_id FROM cost_events WHERE id = $1",
            _ => return Err(ServiceError::Validation("Unsupported ownership resource".to_string())),
        };
        let owner: Option<Uuid> = sqlx::query_scalar(sql)
            .bind(id).fetch_optional(&self._company_repo.pool).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        match owner {
            Some(owner) if owner == company_id => Ok(()),
            Some(_) => Err(ServiceError::Validation(format!("{} does not belong to company", label))),
            None => Err(ServiceError::NotFound(format!("{} not found", label))),
        }
    }
}

#[async_trait]
impl FinanceService for DefaultFinanceService {
    async fn create_event(&self, company_id: Uuid, input: CreateFinanceEventInput) -> ServiceResult<FinanceEventDto> {
        if let Some(id) = input.agent_id { self.assert_belongs_to_company("agents", id, company_id, "Agent").await?; }
        if let Some(id) = input.issue_id { self.assert_belongs_to_company("issues", id, company_id, "Issue").await?; }
        if let Some(id) = input.project_id { self.assert_belongs_to_company("projects", id, company_id, "Project").await?; }
        if let Some(id) = input.goal_id { self.assert_belongs_to_company("goals", id, company_id, "Goal").await?; }
        if let Some(id) = input.heartbeat_run_id { self.assert_belongs_to_company("heartbeat_runs", id, company_id, "Heartbeat run").await?; }
        if let Some(id) = input.cost_event_id { self.assert_belongs_to_company("cost_events", id, company_id, "Cost event").await?; }
        let now = Utc::now();
        let event = FinanceEventModel {
            id: Uuid::new_v4(),
            company_id,
            agent_id: input.agent_id,
            issue_id: input.issue_id,
            project_id: input.project_id,
            goal_id: input.goal_id,
            heartbeat_run_id: input.heartbeat_run_id,
            cost_event_id: input.cost_event_id,
            biller: input.biller,
            event_kind: input.kind,
            direction: input.direction.unwrap_or(FinanceDirection::Debit),
            amount_cents: input.amount_cents,
            currency: input.currency.unwrap_or_else(|| "USD".to_string()),
            estimated: input.estimated.unwrap_or(false),
            description: input.description,
            occurred_at: input.occurred_at.unwrap_or(now),
            created_at: now,
        };

        let saved = self.repo.create(&event).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let _ = sqlx::query("INSERT INTO activity_logs (id, company_id, event_type, actor_type, actor_id, resource_type, resource_id, metadata, created_at) VALUES ($1, $2, 'finance.event_created', 'system', $3, 'budget', $4, $5, NOW())")
            .bind(Uuid::new_v4()).bind(company_id).bind(Uuid::nil()).bind(saved.id)
            .bind(serde_json::json!({"amountCents": saved.amount_cents, "biller": saved.biller, "kind": saved.event_kind}))
            .execute(&self._company_repo.pool).await;

        Ok(FinanceEventDto::from(saved))
    }

    async fn get_summary(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<FinanceSummaryDto> {
        let row = self.repo.summarize(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;

        let mut dto = FinanceSummaryDto::from(row);
        dto.company_id = company_id;
        Ok(dto)
    }

    async fn by_biller(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<FinanceSummaryRowDto>> {
        let rows = self.repo.by_biller(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(|r| {
            let mut dto = FinanceSummaryRowDto::from(r);
            dto.kind_count = None;
            dto
        }).collect())
    }

    async fn by_kind(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> ServiceResult<Vec<FinanceSummaryRowDto>> {
        let rows = self.repo.by_kind(company_id, start_time, end_time).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(rows.into_iter().map(|r| {
            let mut dto = FinanceSummaryRowDto::from(r);
            dto.biller_count = None;
            dto
        }).collect())
    }

    async fn list_events(&self, company_id: Uuid, start_time: DateTime<Utc>, end_time: DateTime<Utc>, limit: i64) -> ServiceResult<Vec<FinanceEventDto>> {
        let events = self.repo.list_by_company(company_id, start_time, end_time, limit.clamp(1, 500)).await
            .map_err(|e| ServiceError::Repository(e.to_string()))?;
        Ok(events.into_iter().map(FinanceEventDto::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_empty_non_company_scope_name() {
        assert_eq!(normalize_scope_name(BudgetScopeType::Agent, "  "), "agent");
        assert_eq!(normalize_scope_name(BudgetScopeType::Company, "Acme"), "Acme");
    }

    #[test]
    fn finance_summary_row_calculates_net_cents() {
        let dto = FinanceSummaryRowDto::from(repositories::finance_event_repository::FinanceSummaryRow {
            dimension: "openai".to_string(),
            debit_cents: 150,
            credit_cents: 40,
            estimated_debit_cents: 100,
            event_count: 2,
        });
        assert_eq!(dto.net_cents, 110);
    }

    #[test]
    fn budget_resolution_input_accepts_override_details() {
        let input: BudgetIncidentResolveInput = serde_json::from_value(serde_json::json!({
            "resolution": "raise_budget_and_resume", "resolvedByUserId": Uuid::nil(),
            "amount": 5000, "decisionNote": "Approved for launch"
        })).unwrap();
        assert_eq!(input.amount, Some(5000));
        assert_eq!(input.decision_note.as_deref(), Some("Approved for launch"));
    }
}
