use async_trait::async_trait;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeControlMode,
};
use uuid::Uuid;
use crate::RepositoryError;

/// Input for creating a tree hold
#[derive(Debug, Clone, Default)]
pub struct CreateTreeHoldInput {
    pub company_id: Uuid,
    pub root_issue_id: Uuid,
    pub mode: IssueTreeControlMode,
    pub reason: Option<String>,
    pub release_policy: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
    pub created_by_actor_type: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<String>,
    pub created_by_run_id: Option<Uuid>,
}

/// Attribution recorded when a hold is released.
#[derive(Debug, Clone, Default)]
pub struct ReleaseTreeHoldInput {
    pub released_by_actor_type: Option<String>,
    pub released_by_agent_id: Option<Uuid>,
    pub released_by_user_id: Option<String>,
    pub released_by_run_id: Option<Uuid>,
    pub release_reason: Option<String>,
    pub release_metadata: Option<serde_json::Value>,
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

    /// Release a tree hold with full Paperclip attribution
    async fn release_with_actor(
        &self,
        hold_id: Uuid,
        input: ReleaseTreeHoldInput,
    ) -> Result<IssueTreeHold, RepositoryError>;

    /// Get hold members
    async fn get_members(&self, hold_id: Uuid) -> Result<Vec<IssueTreeHoldMember>, RepositoryError>;

    /// Create hold members in batch
    async fn create_members(&self, members: Vec<IssueTreeHoldMember>) -> Result<(), RepositoryError>;

    /// Mark a member as restored after a successful release transition.
    async fn mark_member_restored(
        &self,
        hold_id: Uuid,
        issue_id: Uuid,
    ) -> Result<(), RepositoryError>;

    /// Set (or clear) the apply failure recorded for a hold.
    async fn set_apply_error(
        &self,
        hold_id: Uuid,
        error: Option<String>,
    ) -> Result<(), RepositoryError>;
}
