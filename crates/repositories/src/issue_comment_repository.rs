use async_trait::async_trait;
use models::{CommentActorType, IssueComment, IssueCommentAuthorType, Pagination};
use uuid::Uuid;
use serde_json::Value as JsonValue;
use crate::RepositoryError;

/// Input for creating an Issue comment
#[derive(Debug, Clone)]
pub struct CreateIssueCommentInput {
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub body: String,
    pub actor_type: CommentActorType,
    pub actor_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub metadata: Option<JsonValue>,
    /// Paperclip `authorType`; defaults to the legacy `actorType` text.
    pub author_type: Option<String>,
    /// User the agent comment is posted on behalf of.
    pub on_behalf_of_user_id: Option<String>,
    /// Best-effort attribution for sentinel-authored comments.
    pub derived_author_agent_id: Option<Uuid>,
    pub derived_created_by_run_id: Option<Uuid>,
    /// Stored as TEXT: `run` | `log_scan` | `best_effort`.
    pub derived_author_source: Option<String>,
    pub source_trust: Option<JsonValue>,
}

/// Input for updating an Issue comment
#[derive(Debug, Clone)]
pub struct UpdateIssueCommentInput {
    pub body: Option<String>,
    pub metadata: Option<JsonValue>,
}

#[async_trait]
pub trait IssueCommentRepository: Send + Sync {
    /// Create a new comment
    async fn create(&self, input: CreateIssueCommentInput) -> Result<IssueComment, RepositoryError>;

    /// Get a comment by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<IssueComment>, RepositoryError>;

    /// List all comments for an issue
    async fn list_by_issue(&self, issue_id: Uuid, pagination: &Pagination) -> Result<Vec<IssueComment>, RepositoryError>;

    /// Count comments for an issue
    async fn count_by_issue(&self, issue_id: Uuid) -> Result<i64, RepositoryError>;

    /// Update a comment
    async fn update(&self, id: Uuid, input: UpdateIssueCommentInput) -> Result<IssueComment, RepositoryError>;

    /// Redact a comment in-place and retain its identity for audit/history.
    async fn tombstone(
        &self,
        id: Uuid,
        deleted_by_type: IssueCommentAuthorType,
        deleted_by_id: Uuid,
        deleted_by_run_id: Option<Uuid>,
    ) -> Result<Option<IssueComment>, RepositoryError>;
}
