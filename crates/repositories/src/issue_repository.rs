use async_trait::async_trait;
use models::{Issue, IssueQueryFilter, Pagination, CreateIssueInput, UpdateIssueInput};
use uuid::Uuid;
use crate::RepositoryError;

#[async_trait]
pub trait IssueRepository: Send + Sync {
    /// Get a single issue by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Issue>, RepositoryError>;

    /// List issues by company with optional filtering and pagination
    async fn list_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError>;

    /// Count issues by company with optional filtering
    async fn count_by_company(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
    ) -> Result<i64, RepositoryError>;

    /// Create a new issue
    async fn create(&self, input: CreateIssueInput) -> Result<Issue, RepositoryError>;

    /// Update an existing issue
    async fn update(&self, id: Uuid, input: UpdateIssueInput) -> Result<Issue, RepositoryError>;

    /// Delete (soft delete) an issue by setting cancelled status
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Search issues by title/description
    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError>;

    /// Get issue by identifier
    async fn get_by_identifier(&self, identifier: &str) -> Result<Option<Issue>, RepositoryError>;

    /// List issues by parent
    async fn list_by_parent(
        &self,
        parent_id: Uuid,
        pagination: &Pagination,
    ) -> Result<Vec<Issue>, RepositoryError>;

    /// List child issues (alias for list_by_parent for API compatibility)
    async fn list_children(
        &self,
        parent_id: Uuid,
    ) -> Result<Vec<Issue>, RepositoryError> {
        self.list_by_parent(parent_id, &Pagination { limit: 1000, offset: 0, cursor: None }).await
    }

    /// Get issues by multiple IDs
    async fn get_by_ids(&self, ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError>;

    /// Walk up the parent chain from an issue and return all ancestors.
    /// Returns them in order from immediate parent to root.
    async fn list_ancestors(&self, issue_id: Uuid) -> Result<Vec<Issue>, RepositoryError>;
}
