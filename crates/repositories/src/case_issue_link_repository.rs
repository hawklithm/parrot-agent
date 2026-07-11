use async_trait::async_trait;
use models::{CaseIssueLink, CaseIssueLinkRole};
use uuid::Uuid;
use crate::RepositoryError;

/// Input for creating a Case-Issue link
#[derive(Debug, Clone)]
pub struct CreateCaseIssueLinkInput {
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub issue_id: Uuid,
    pub role: CaseIssueLinkRole,
    pub created_by_run_id: Option<Uuid>,
}

#[async_trait]
pub trait CaseIssueLinkRepository: Send + Sync {
    /// Create a new Case-Issue link
    async fn create(&self, input: CreateCaseIssueLinkInput) -> Result<CaseIssueLink, RepositoryError>;

    /// Get a specific link by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CaseIssueLink>, RepositoryError>;

    /// List all issue links for a case
    async fn list_by_case(&self, case_id: Uuid) -> Result<Vec<CaseIssueLink>, RepositoryError>;

    /// List all case links for an issue
    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<CaseIssueLink>, RepositoryError>;

    /// Find a specific link by case_id, issue_id, and role
    async fn find_by_case_issue_role(
        &self,
        case_id: Uuid,
        issue_id: Uuid,
        role: CaseIssueLinkRole,
    ) -> Result<Option<CaseIssueLink>, RepositoryError>;

    /// Delete a specific link
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Delete all links between a case and an issue
    async fn delete_by_case_and_issue(&self, case_id: Uuid, issue_id: Uuid) -> Result<u64, RepositoryError>;
}
