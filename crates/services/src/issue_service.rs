use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use models::{Issue, CreateIssueInput, UpdateIssueInput};
pub use crate::issue_repository::{IssueQueryFilter, Pagination};

/// Issue mutation result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueMutationResult {
    pub changed: bool,
    pub issue: Issue,
    pub change_kind: String, // "created" | "updated" | "deleted" | "status_changed"
}

/// Checkout input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutInput {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub expected_statuses: Vec<String>,
    pub checkout_run_id: Uuid,
}

/// Release input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInput {
    pub release_run_id: Uuid,
    pub result: Option<String>,
    pub target_status: Option<String>,
}

/// Force release input (admin operation)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceReleaseInput {
    pub admin_user_id: Uuid,
    pub reason: String,
    pub release_lease: bool,
}

/// Issue service trait for business logic
#[async_trait]
pub trait IssueService: Send + Sync {
    /// Create a new issue
    async fn create(&self, input: CreateIssueInput) -> Result<IssueMutationResult, String>;

    /// Create a child issue
    async fn create_child(&self, parent_id: Uuid, input: CreateIssueInput) -> Result<IssueMutationResult, String>;

    /// Get issue by ID
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Issue>, String>;

    /// List issues with filtering
    async fn list(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, String>;

    /// Update issue
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateIssueInput) -> Result<IssueMutationResult, String>;

    /// Delete issue
    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<IssueMutationResult, String>;

    /// Checkout issue for execution
    async fn checkout(&self, id: Uuid, company_id: Uuid, input: CheckoutInput) -> Result<Issue, String>;

    /// Release issue from execution
    async fn release(&self, id: Uuid, company_id: Uuid, input: ReleaseInput) -> Result<Issue, String>;

    /// Force release issue (admin operation)
    async fn force_release(&self, id: Uuid, company_id: Uuid, input: ForceReleaseInput) -> Result<Issue, String>;

    /// Search issues
    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, String>;

    /// Batch update issues (status, priority, assignee)
    async fn batch_update(
        &self,
        company_id: Uuid,
        issue_ids: Vec<Uuid>,
        status: Option<String>,
        priority: Option<String>,
        assignee_agent_id: Option<Uuid>,
        assignee_user_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, String>;

    /// Get heartbeat context for issue
    async fn get_heartbeat_context(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    // --- P1: Issue 子资源补齐 (I1-I44) ---

    /// I1: Get issue activity
    async fn get_activity(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I2: Get related cases
    async fn get_cases(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I3: Get active run
    async fn get_active_run(&self, id: Uuid, company_id: Uuid) -> Result<Option<serde_json::Value>, String>;

    /// I4: Get live runs
    async fn get_live_runs(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I5: Get run history
    async fn get_runs(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I6: Get accepted plan decompositions
    async fn get_accepted_plan_decompositions(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I7: Submit plan decomposition
    async fn submit_plan_decomposition(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// I8: Get approvals
    async fn get_approvals(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I9: Create approval
    async fn create_approval(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// I10: Delete approval
    async fn delete_approval(&self, id: Uuid, approval_id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// I12: Mark issue as read
    async fn mark_read(&self, id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// I13: Unmark issue as read
    async fn unmark_read(&self, id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// I14: Archive to inbox
    async fn archive_inbox(&self, id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// I15: Unarchive from inbox
    async fn unarchive_inbox(&self, id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// I27: Get recovery actions
    async fn get_recovery_actions(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// I28: Resolve recovery action
    async fn resolve_recovery_action(&self, id: Uuid, company_id: Uuid, action_id: Uuid) -> Result<(), String>;

    /// I39: Create work product
    async fn create_work_product(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// I42: Get single comment
    async fn get_comment(&self, comment_id: Uuid, company_id: Uuid) -> Result<Option<serde_json::Value>, String>;

    /// I43: Get cost summary
    async fn get_cost_summary(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;
}
