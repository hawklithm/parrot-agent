use async_trait::async_trait;
use models::{IssueComment, CommentActorType, Pagination};
use uuid::Uuid;
use std::sync::Arc;
use repositories::{
    IssueCommentRepository, IssueRepository,
    CreateIssueCommentInput, UpdateIssueCommentInput,
    RepositoryError,
};

/// Service-level errors for Comment operations
#[derive(Debug, thiserror::Error)]
pub enum CommentServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Comment not found: {0}")]
    NotFound(Uuid),

    #[error("Issue not found: {0}")]
    IssueNotFound(Uuid),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type CommentServiceResult<T> = Result<T, CommentServiceError>;

/// Issue Comment Service trait
#[async_trait]
pub trait IssueCommentService: Send + Sync {
    /// Add a new comment to an issue
    async fn add_comment(
        &self,
        issue_id: Uuid,
        body: String,
        actor_type: CommentActorType,
        actor_id: Option<Uuid>,
        actor_run_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
    ) -> CommentServiceResult<IssueComment>;

    /// List comments for an issue
    async fn list_comments(
        &self,
        issue_id: Uuid,
        pagination: &Pagination,
    ) -> CommentServiceResult<Vec<IssueComment>>;

    /// Count comments for an issue
    async fn count_comments(&self, issue_id: Uuid) -> CommentServiceResult<i64>;

    /// Get a single comment by ID
    async fn get_comment(&self, comment_id: Uuid) -> CommentServiceResult<IssueComment>;

    /// Update a comment
    async fn update_comment(
        &self,
        comment_id: Uuid,
        body: String,
        actor_id: Uuid,
    ) -> CommentServiceResult<IssueComment>;

    /// Delete a comment
    async fn delete_comment(
        &self,
        comment_id: Uuid,
        actor_id: Uuid,
    ) -> CommentServiceResult<()>;
}

/// Issue Comment Service implementation
pub struct IssueCommentServiceImpl<CR, IR>
where
    CR: IssueCommentRepository,
    IR: IssueRepository,
{
    comment_repository: Arc<CR>,
    issue_repository: Arc<IR>,
}

impl<CR, IR> IssueCommentServiceImpl<CR, IR>
where
    CR: IssueCommentRepository,
    IR: IssueRepository,
{
    pub fn new(comment_repository: Arc<CR>, issue_repository: Arc<IR>) -> Self {
        Self {
            comment_repository,
            issue_repository,
        }
    }

    /// Verify that the actor can modify the comment
    async fn check_permission(&self, comment: &IssueComment, actor_id: Uuid) -> CommentServiceResult<()> {
        // Check if actor is the comment author
        if let Some(comment_actor_id) = comment.actor_id {
            if comment_actor_id == actor_id {
                return Ok(());
            }
        }

        // TODO: Check if actor is admin when we have access control
        // For now, only allow comment author to modify

        Err(CommentServiceError::PermissionDenied(
            "Only the comment author can modify this comment".to_string(),
        ))
    }
}

#[async_trait]
impl<CR, IR> IssueCommentService for IssueCommentServiceImpl<CR, IR>
where
    CR: IssueCommentRepository + 'static,
    IR: IssueRepository + 'static,
{
    async fn add_comment(
        &self,
        issue_id: Uuid,
        body: String,
        actor_type: CommentActorType,
        actor_id: Option<Uuid>,
        actor_run_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
    ) -> CommentServiceResult<IssueComment> {
        // Verify issue exists
        let issue = self.issue_repository.get_by_id(issue_id).await?
            .ok_or(CommentServiceError::IssueNotFound(issue_id))?;

        // Create comment
        let input = CreateIssueCommentInput {
            company_id: issue.company_id,
            issue_id,
            body,
            actor_type,
            actor_id,
            actor_run_id,
            metadata,
        };

        let comment = self.comment_repository.create(input).await?;

        // TODO: Update issue's last_activity_at when we add that field to UpdateIssueInput
        // For now, the comment creation timestamp serves as activity indicator

        Ok(comment)
    }

    async fn list_comments(
        &self,
        issue_id: Uuid,
        pagination: &Pagination,
    ) -> CommentServiceResult<Vec<IssueComment>> {
        let comments = self.comment_repository.list_by_issue(issue_id, pagination).await?;
        Ok(comments)
    }

    async fn count_comments(&self, issue_id: Uuid) -> CommentServiceResult<i64> {
        let count = self.comment_repository.count_by_issue(issue_id).await?;
        Ok(count)
    }

    async fn get_comment(&self, comment_id: Uuid) -> CommentServiceResult<IssueComment> {
        let comment = self.comment_repository.get_by_id(comment_id).await?
            .ok_or(CommentServiceError::NotFound(comment_id))?;
        Ok(comment)
    }

    async fn update_comment(
        &self,
        comment_id: Uuid,
        body: String,
        actor_id: Uuid,
    ) -> CommentServiceResult<IssueComment> {
        // Get current comment
        let current = self.comment_repository.get_by_id(comment_id).await?
            .ok_or(CommentServiceError::NotFound(comment_id))?;

        // Check permission
        self.check_permission(&current, actor_id).await?;

        // Update comment
        let input = UpdateIssueCommentInput {
            body: Some(body),
            metadata: None,
        };

        let updated = self.comment_repository.update(comment_id, input).await?;
        Ok(updated)
    }

    async fn delete_comment(
        &self,
        comment_id: Uuid,
        actor_id: Uuid,
    ) -> CommentServiceResult<()> {
        // Get current comment
        let current = self.comment_repository.get_by_id(comment_id).await?
            .ok_or(CommentServiceError::NotFound(comment_id))?;

        // Check permission
        self.check_permission(&current, actor_id).await?;

        // Delete comment
        self.comment_repository.delete(comment_id).await?;
        Ok(())
    }
}
