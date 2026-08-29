use async_trait::async_trait;
use models::{CommentActorType, IssueComment, IssueCommentAuthorType, Pagination};
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

    #[error("Conflict: {0}")]
    Conflict(String),

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

    /// Add a comment with full Paperclip attribution (author type, on-behalf-of
    /// user, and best-effort derived author attribution).
    async fn add_comment_attributed(
        &self,
        issue_id: Uuid,
        body: String,
        actor_type: CommentActorType,
        actor_id: Option<Uuid>,
        actor_run_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
        attribution: CommentAttribution,
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
        actor_type: IssueCommentAuthorType,
        actor_id: Uuid,
    ) -> CommentServiceResult<IssueComment>;

    /// Delete a comment
    async fn delete_comment(
        &self,
        comment_id: Uuid,
        actor_type: IssueCommentAuthorType,
        actor_id: Uuid,
        actor_run_id: Option<Uuid>,
    ) -> CommentServiceResult<()>;
}

/// Full Paperclip-style comment attribution.
#[derive(Debug, Clone, Default)]
pub struct CommentAttribution {
    /// Paperclip `authorType`; `None` falls back to the actor type.
    pub author_type: Option<String>,
    /// User the agent comment is posted on behalf of.
    pub on_behalf_of_user_id: Option<String>,
    /// Best-effort attribution for sentinel-authored comments.
    pub derived_author_agent_id: Option<Uuid>,
    pub derived_created_by_run_id: Option<Uuid>,
    pub derived_author_source: Option<String>,
    pub source_trust: Option<serde_json::Value>,
}

/// Issue Comment Service implementation
pub struct IssueCommentServiceImpl<CR, IR>
where
    CR: IssueCommentRepository,
    IR: IssueRepository,
{
    comment_repository: Arc<CR>,
    issue_repository: Arc<IR>,
    /// Optional pool used to resolve comment attribution: the responsible user
    /// an agent comment was posted on behalf of, plus best-effort derived
    /// attribution for sentinel-authored comments. Without it, comments are
    /// still created — just without derived attribution.
    pool: Option<sqlx::PgPool>,
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
            pool: None,
        }
    }

    /// Attach a pool so `add_comment` can resolve on-behalf-of attribution.
    pub fn with_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Resolve the user an agent comment is posted on behalf of.
    ///
    /// Paperclip prefers an explicit request value and falls back to the
    /// creating heartbeat run's responsible user. A stale or unknown run id
    /// yields `None` rather than failing the insert.
    async fn resolve_on_behalf_of_user_id(
        &self,
        company_id: Uuid,
        actor_type: CommentActorType,
        requested: Option<String>,
        created_by_run_id: Option<Uuid>,
    ) -> Option<String> {
        if requested.is_some() {
            return requested;
        }
        // Only agent comments carry derived on-behalf-of attribution.
        if !matches!(actor_type, CommentActorType::Agent) {
            return None;
        }
        let (pool, run_id) = (self.pool.as_ref()?, created_by_run_id?);
        sqlx::query_scalar::<_, String>(
            "SELECT responsible_user_id FROM heartbeat_runs
              WHERE id = $1 AND company_id = $2 AND responsible_user_id IS NOT NULL",
        )
        .bind(run_id)
        .bind(company_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    }

    /// Verify that the actor can modify the comment
    async fn check_permission(
        &self,
        comment: &IssueComment,
        actor_type: &IssueCommentAuthorType,
        actor_id: Uuid,
    ) -> CommentServiceResult<()> {
        if comment.deleted_at.is_some() {
            return Err(CommentServiceError::Conflict(
                "Deleted comments cannot be modified".to_string(),
            ));
        }

        let is_author = match (actor_type, &comment.author_type) {
            (IssueCommentAuthorType::Agent, IssueCommentAuthorType::Agent) => {
                comment.author_agent_id == Some(actor_id)
            }
            (IssueCommentAuthorType::User, IssueCommentAuthorType::User) => {
                comment.author_user_id == Some(actor_id)
            }
            _ => false,
        };
        if is_author {
            return Ok(());
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

        let comment = self
            .add_comment_attributed(
                issue_id,
                body,
                actor_type,
                actor_id,
                actor_run_id,
                metadata,
                CommentAttribution::default(),
            )
            .await?;

        Ok(comment)
    }

    async fn add_comment_attributed(
        &self,
        issue_id: Uuid,
        body: String,
        actor_type: CommentActorType,
        actor_id: Option<Uuid>,
        actor_run_id: Option<Uuid>,
        metadata: Option<serde_json::Value>,
        attribution: CommentAttribution,
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
            author_type: attribution.author_type,
            on_behalf_of_user_id: self
                .resolve_on_behalf_of_user_id(
                    issue.company_id,
                    actor_type,
                    attribution.on_behalf_of_user_id,
                    actor_run_id,
                )
                .await,
            derived_author_agent_id: attribution.derived_author_agent_id,
            derived_created_by_run_id: attribution.derived_created_by_run_id,
            derived_author_source: attribution.derived_author_source,
            source_trust: attribution.source_trust,
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
        actor_type: IssueCommentAuthorType,
        actor_id: Uuid,
    ) -> CommentServiceResult<IssueComment> {
        // Get current comment
        let current = self.comment_repository.get_by_id(comment_id).await?
            .ok_or(CommentServiceError::NotFound(comment_id))?;

        // Check permission
        self.check_permission(&current, &actor_type, actor_id).await?;

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
        actor_type: IssueCommentAuthorType,
        actor_id: Uuid,
        actor_run_id: Option<Uuid>,
    ) -> CommentServiceResult<()> {
        // Get current comment
        let current = self.comment_repository.get_by_id(comment_id).await?
            .ok_or(CommentServiceError::NotFound(comment_id))?;

        // Check permission
        if current.deleted_at.is_some() {
            return Ok(());
        }
        self.check_permission(&current, &actor_type, actor_id).await?;

        // Redact the body in the same transaction that updates the issue's
        // activity timestamp. A concurrent delete is treated as idempotent.
        if self
            .comment_repository
            .tombstone(comment_id, actor_type, actor_id, actor_run_id)
            .await?
            .is_none()
        {
            if self
                .comment_repository
                .get_by_id(comment_id)
                .await?
                .is_some_and(|comment| comment.deleted_at.is_none())
            {
                return Err(CommentServiceError::Conflict(
                    "Comment changed while it was being deleted".to_string(),
                ));
            }
        }
        Ok(())
    }
}
