use async_trait::async_trait;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeControlMode,
    IssueTreeHoldStatus, CreateIssueTreeHoldInput,
};
use uuid::Uuid;
use crate::RepositoryError;

/// Input for creating a tree hold
#[derive(Debug, Clone)]
pub struct CreateTreeHoldInput {
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
}

#[async_trait]
pub trait IssueTreeHoldRepository: Send + Sync {
    /// Create a new tree hold
    async fn create(&self, input: CreateTreeHoldInput) -> Result<IssueTreeHold, RepositoryError>;

    /// Get a tree hold by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<IssueTreeHold>, RepositoryError>;

    /// List active holds for an issue (checks if issue is in any active hold's member list)
    async fn list_active_for_issue(&self, issue_id: Uuid) -> Result<Vec<IssueTreeHold>, RepositoryError>;

    /// List all holds for a root issue
    async fn list_by_root_issue(&self, root_issue_id: Uuid) -> Result<Vec<IssueTreeHold>, RepositoryError>;

    /// Release a tree hold
    async fn release(
        &self,
        hold_id: Uuid,
        released_by_type: Option<String>,
        released_by_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, RepositoryError>;

    /// Get hold members
    async fn get_members(&self, hold_id: Uuid) -> Result<Vec<IssueTreeHoldMember>, RepositoryError>;

    /// Create hold members in batch
    async fn create_members(&self, members: Vec<IssueTreeHoldMember>) -> Result<(), RepositoryError>;
}
