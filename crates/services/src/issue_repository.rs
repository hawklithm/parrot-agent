use async_trait::async_trait;
use uuid::Uuid;
use crate::models::{Issue, IssueStatus, IssuePriority};

/// Pagination parameters
#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
    pub cursor: Option<String>,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
            cursor: None,
        }
    }
}

/// Issue query filter
#[derive(Debug, Clone, Default)]
pub struct IssueQueryFilter {
    pub status: Option<Vec<IssueStatus>>,
    pub priority: Option<Vec<IssuePriority>>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub search_query: Option<String>,
}

/// Issue repository trait for data access
#[async_trait]
pub trait IssueRepository: Send + Sync {
    /// Create a new issue
    async fn create(&self, issue: Issue) -> Result<Issue, String>;
    
    /// Get issue by ID
    async fn get_by_id(&self, id: Uuid, company_id: Uuid) -> Result<Option<Issue>, String>;
    
    /// List issues by company with filtering and pagination
    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, String>;
    
    /// Count issues by company with filtering
    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
    ) -> Result<i64, String>;
    
    /// Update issue
    async fn update(&self, issue: Issue) -> Result<Issue, String>;
    
    /// Delete issue (soft delete or cancel)
    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<bool, String>;
}
