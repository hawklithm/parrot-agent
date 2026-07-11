// Issue状态一致性检查器 - 检测Issue状态与运行状态的不一致
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait IssueConsistencyChecker: Send + Sync {
    /// 检查1: Issue.status=in_progress但无active Run
    async fn check_orphaned_in_progress(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 检查2: Issue.status=blocked但Approval已approved
    async fn check_wrongly_blocked(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 检查3: Issue.checkout_run_id指向不存在Run
    async fn check_invalid_run_ref(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 修复: release孤立的Issue
    async fn fix_release_issue(&self, issue_id: Uuid) -> Result<(), String>;

    /// 修复: 解除错误阻塞
    async fn fix_unblock_issue(&self, issue_id: Uuid) -> Result<(), String>;

    /// 修复: 清除无效run引用
    async fn fix_clear_run_ref(&self, issue_id: Uuid) -> Result<(), String>;
}

// Environment Lease一致性检查器
#[async_trait]
pub trait LeaseConsistencyChecker: Send + Sync {
    /// 检查1: lease.status=active但last_used_at超时
    async fn check_expired_leases(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 检查2: environment.status=in_use但无active lease
    async fn check_orphaned_environments(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 检查3: 租约关联的Workspace已删除
    async fn check_invalid_workspace_refs(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 修复: 释放过期租约
    async fn fix_release_lease(&self, lease_id: Uuid) -> Result<(), String>;

    /// 修复: 更新environment状态为active
    async fn fix_reset_environment_status(&self, environment_id: Uuid) -> Result<(), String>;

    /// 修复: 释放租约并记录异常
    async fn fix_release_and_log(&self, lease_id: Uuid, reason: &str) -> Result<(), String>;
}

// Agent状态一致性检查器
#[async_trait]
pub trait AgentConsistencyChecker: Send + Sync {
    /// 检查1: Agent.status=running但lastHeartbeatAt超时(>5分钟)
    async fn check_heartbeat_timeout(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 检查2: Agent.reportsTo指向已terminated的Agent
    async fn check_invalid_reports_to(&self, company_id: Uuid) -> Result<Vec<Uuid>, String>;

    /// 修复: 更新status=paused
    async fn fix_pause_agent(&self, agent_id: Uuid) -> Result<(), String>;

    /// 修复: 清除reportsTo
    async fn fix_clear_reports_to(&self, agent_id: Uuid) -> Result<(), String>;
}

pub struct ConsistencyReport {
    pub resource_type: String,
    pub resource_id: Uuid,
    pub expected_state: String,
    pub actual_state: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
}
