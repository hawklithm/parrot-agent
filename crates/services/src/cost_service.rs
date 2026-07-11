use async_trait::async_trait;
use chrono::{DateTime, Utc, Datelike};
use std::collections::HashMap;
use uuid::Uuid;

/// CostEvent - 成本事件记录
#[derive(Debug, Clone)]
pub struct CostEvent {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub cost_cents: i32,
    pub event_type: CostEventType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostEventType {
    TokenUsage,
    ToolExecution,
    StorageUsage,
    Other,
}

/// CostEventService - 成本事件服务接口
#[async_trait]
pub trait CostEventService: Send + Sync {
    /// 创建成本事件
    async fn create_cost_event(&self, event: CostEvent) -> Result<(), CostServiceError>;

    /// 按Agent聚合成本（按月）
    async fn aggregate_by_agent(&self, agent_id: Uuid, year: i32, month: u32) -> Result<i32, CostServiceError>;

    /// 按Company聚合成本（按月）
    async fn aggregate_by_company(&self, company_id: Uuid, year: i32, month: u32) -> Result<i32, CostServiceError>;

    /// 月度滚动计算（重置上月数据）
    async fn monthly_rollover(&self, company_id: Uuid) -> Result<(), CostServiceError>;

    /// 获取Agent当月花费
    async fn get_agent_monthly_spend(&self, agent_id: Uuid) -> Result<i32, CostServiceError> {
        let now = Utc::now();
        self.aggregate_by_agent(agent_id, now.year(), now.month()).await
    }
}

/// Agent花费计算辅助函数
pub async fn hydrate_agent_spend<C: CostEventService>(
    cost_service: &C,
    agent_id: Uuid,
) -> Result<i32, CostServiceError> {
    cost_service.get_agent_monthly_spend(agent_id).await
}

/// 预算校验
pub fn check_budget_exceeded(spent_cents: i32, budget_cents: i32) -> bool {
    spent_cents > budget_cents
}

/// 计算预算使用率
pub fn calculate_budget_utilization(spent_cents: i32, budget_cents: i32) -> f32 {
    if budget_cents == 0 {
        return 0.0;
    }
    (spent_cents as f32 / budget_cents as f32) * 100.0
}

#[derive(Debug, thiserror::Error)]
pub enum CostServiceError {
    #[error("Repository error: {0}")]
    RepositoryError(String),

    #[error("Invalid date range: {0}")]
    InvalidDateRange(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_budget_exceeded() {
        assert!(check_budget_exceeded(1500, 1000));
        assert!(!check_budget_exceeded(800, 1000));
        assert!(!check_budget_exceeded(1000, 1000));
    }

    #[test]
    fn test_budget_utilization() {
        assert_eq!(calculate_budget_utilization(500, 1000), 50.0);
        assert_eq!(calculate_budget_utilization(1000, 1000), 100.0);
        assert_eq!(calculate_budget_utilization(1500, 1000), 150.0);
        assert_eq!(calculate_budget_utilization(0, 1000), 0.0);
        assert_eq!(calculate_budget_utilization(100, 0), 0.0);
    }
}
