use async_trait::async_trait;
use models::{Issue, IssueStatus, Pagination, CreateIssueInput, UpdateIssueInput, IssueQueryFilter};
use uuid::Uuid;
use std::sync::Arc;
use repositories::{IssueRepository, RepositoryError};

/// Service-level errors
#[derive(Debug, thiserror::Error)]
pub enum IssueServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Issue not found: {0}")]
    NotFound(Uuid),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: IssueStatus, to: IssueStatus },

    #[error("Circular reference detected in issue tree")]
    CircularReference,

    #[error("Issue tree depth limit exceeded (max: {max})")]
    DepthLimitExceeded { max: i32 },

    #[error("Parent issue belongs to different company")]
    CrossCompanyParent,

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type IssueServiceResult<T> = Result<T, IssueServiceError>;

/// Issue Service trait
#[async_trait]
pub trait IssueService: Send + Sync {
    /// Create a new issue
    async fn create(&self, input: CreateIssueInput) -> IssueServiceResult<Issue>;

    /// Get an issue by ID
    async fn get(&self, id: Uuid) -> IssueServiceResult<Issue>;

    /// List issues with filtering and pagination
    async fn list(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> IssueServiceResult<Vec<Issue>>;

    /// Count issues matching filter
    async fn count(&self, company_id: Uuid, filter: &IssueQueryFilter) -> IssueServiceResult<i64>;

    /// Update an issue
    async fn update(&self, id: Uuid, input: UpdateIssueInput) -> IssueServiceResult<Issue>;

    /// Delete an issue
    async fn delete(&self, id: Uuid) -> IssueServiceResult<()>;

    /// Search issues by text
    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        pagination: &Pagination,
    ) -> IssueServiceResult<Vec<Issue>>;

    /// Get issue by identifier
    async fn get_by_identifier(&self, identifier: &str) -> IssueServiceResult<Issue>;

    /// List child issues
    async fn list_children(&self, parent_id: Uuid, pagination: &Pagination) -> IssueServiceResult<Vec<Issue>>;
}

/// Issue Service implementation
pub struct IssueServiceImpl<R: IssueRepository> {
    repository: Arc<R>,
    max_tree_depth: i32,
}

impl<R: IssueRepository> IssueServiceImpl<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self {
            repository,
            max_tree_depth: 10, // Default max depth
        }
    }

    pub fn with_max_depth(mut self, max_depth: i32) -> Self {
        self.max_tree_depth = max_depth;
        self
    }

    /// Validate state transition
    fn validate_state_transition(&self, from: IssueStatus, to: IssueStatus) -> IssueServiceResult<()> {
        // Use the state machine from models
        let state_machine = models::IssueStateMachine::new();
        if state_machine.validate_transition(from, to) {
            Ok(())
        } else {
            Err(IssueServiceError::InvalidStateTransition { from, to })
        }
    }

    /// Check for circular references in issue tree
    async fn check_circular_reference(&self, issue_id: Uuid, parent_id: Uuid) -> IssueServiceResult<()> {
        let mut current_id = parent_id;
        let mut visited = std::collections::HashSet::new();
        visited.insert(issue_id);

        loop {
            if visited.contains(&current_id) {
                return Err(IssueServiceError::CircularReference);
            }
            visited.insert(current_id);

            let parent = self.repository.get_by_id(current_id).await?;
            match parent {
                Some(p) => {
                    if let Some(next_parent_id) = p.parent_id {
                        current_id = next_parent_id;
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        Ok(())
    }

    /// Calculate tree depth from root
    async fn calculate_tree_depth(&self, parent_id: Uuid) -> IssueServiceResult<i32> {
        let mut depth = 1;
        let mut current_id = parent_id;

        loop {
            let parent = self.repository.get_by_id(current_id).await?;
            match parent {
                Some(p) => {
                    if let Some(next_parent_id) = p.parent_id {
                        depth += 1;
                        if depth > self.max_tree_depth {
                            return Err(IssueServiceError::DepthLimitExceeded {
                                max: self.max_tree_depth,
                            });
                        }
                        current_id = next_parent_id;
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        Ok(depth)
    }

    /// Validate parent issue constraints
    async fn validate_parent(&self, company_id: Uuid, parent_id: Uuid) -> IssueServiceResult<()> {
        let parent = self.repository.get_by_id(parent_id).await?
            .ok_or(IssueServiceError::NotFound(parent_id))?;

        // Check same company
        if parent.company_id != company_id {
            return Err(IssueServiceError::CrossCompanyParent);
        }

        // Check tree depth
        self.calculate_tree_depth(parent_id).await?;

        Ok(())
    }
}

#[async_trait]
impl<R: IssueRepository + 'static> IssueService for IssueServiceImpl<R> {
    async fn create(&self, input: CreateIssueInput) -> IssueServiceResult<Issue> {
        // Validate parent if specified
        if let Some(parent_id) = input.parent_id {
            self.validate_parent(input.company_id, parent_id).await?;
        }

        let issue = self.repository.create(input).await?;
        Ok(issue)
    }

    async fn get(&self, id: Uuid) -> IssueServiceResult<Issue> {
        self.repository.get_by_id(id).await?
            .ok_or(IssueServiceError::NotFound(id))
    }

    async fn list(
        &self,
        company_id: Uuid,
        filter: &IssueQueryFilter,
        pagination: &Pagination,
    ) -> IssueServiceResult<Vec<Issue>> {
        let issues = self.repository.list_by_company(company_id, filter, pagination).await?;
        Ok(issues)
    }

    async fn count(&self, company_id: Uuid, filter: &IssueQueryFilter) -> IssueServiceResult<i64> {
        let count = self.repository.count_by_company(company_id, filter).await?;
        Ok(count)
    }

    async fn update(&self, id: Uuid, input: UpdateIssueInput) -> IssueServiceResult<Issue> {
        // Get current issue
        let current = self.get(id).await?;

        // Validate state transition if status is being changed
        if let Some(new_status) = input.status {
            self.validate_state_transition(current.status, new_status)?;
        }

        // Note: parent_id changes are not supported via UpdateIssueInput
        // Parent relationships should be established during creation

        let updated = self.repository.update(id, input).await?;
        Ok(updated)
    }

    async fn delete(&self, id: Uuid) -> IssueServiceResult<()> {
        self.repository.delete(id).await?;
        Ok(())
    }

    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        pagination: &Pagination,
    ) -> IssueServiceResult<Vec<Issue>> {
        let issues = self.repository.search(company_id, query, pagination).await?;
        Ok(issues)
    }

    async fn get_by_identifier(&self, identifier: &str) -> IssueServiceResult<Issue> {
        self.repository.get_by_identifier(identifier).await?
            .ok_or_else(|| IssueServiceError::Validation(format!("Issue not found: {}", identifier)))
    }

    async fn list_children(&self, parent_id: Uuid, pagination: &Pagination) -> IssueServiceResult<Vec<Issue>> {
        let children = self.repository.list_by_parent(parent_id, pagination).await?;
        Ok(children)
    }
}
