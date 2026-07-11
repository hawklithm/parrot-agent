use async_trait::async_trait;
use models::{IssueComment, CommentActorType, Pagination};
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

    /// Delete a comment
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
}
