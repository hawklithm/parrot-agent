use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::models::{Issue, CreateIssueInput, UpdateIssueInput};
use crate::issue_repository::{IssueQueryFilter, Pagination};

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
    
    /// Search issues
    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, String>;
}
