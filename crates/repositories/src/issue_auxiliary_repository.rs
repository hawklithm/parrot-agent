use async_trait::async_trait;
use models::{
    IssueReadStatus, MarkIssueReadInput,
    IssueInboxArchive, ArchiveIssueInput,
    FeedbackVote, CreateFeedbackVoteInput,
    FeedbackTrace, FeedbackTraceBundle,
    RecoveryAction, CreateRecoveryActionInput, ResolveRecoveryActionInput,
    PlanDecomposition, CreatePlanDecompositionInput, AcceptPlanDecompositionInput,
};
use uuid::Uuid;
use crate::RepositoryError;

// ─── Issue Read Status ─────────────────────────────────────────

#[async_trait]
pub trait IssueReadStatusRepository: Send + Sync {
    /// Mark an issue as read by a user
    async fn mark_read(&self, company_id: Uuid, issue_id: Uuid, input: &MarkIssueReadInput) -> Result<IssueReadStatus, RepositoryError>;

    /// Unmark an issue as read (remove read status)
    async fn unmark_read(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError>;

    /// Get read status for a specific issue and user
    async fn get_read_status(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<Option<IssueReadStatus>, RepositoryError>;

    /// Get all read statuses for an issue
    async fn list_read_statuses(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<IssueReadStatus>, RepositoryError>;

    /// Check if a user has read an issue
    async fn is_read(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<bool, RepositoryError>;
}

// ─── Issue Inbox Archive ───────────────────────────────────────

#[async_trait]
pub trait IssueInboxArchiveRepository: Send + Sync {
    /// Archive an issue for a user
    async fn archive(&self, company_id: Uuid, issue_id: Uuid, input: &ArchiveIssueInput) -> Result<IssueInboxArchive, RepositoryError>;

    /// Unarchive an issue for a user
    async fn unarchive(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<(), RepositoryError>;

    /// Get archive status for a specific issue and user
    async fn get_archive(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<Option<IssueInboxArchive>, RepositoryError>;

    /// List archived issues for a user
    async fn list_archived(&self, company_id: Uuid, user_id: Uuid) -> Result<Vec<IssueInboxArchive>, RepositoryError>;

    /// Check if an issue is archived by a user
    async fn is_archived(&self, company_id: Uuid, issue_id: Uuid, user_id: Uuid) -> Result<bool, RepositoryError>;
}

// ─── Feedback Vote ─────────────────────────────────────────────

#[async_trait]
pub trait FeedbackVoteRepository: Send + Sync {
    /// Create or update a feedback vote (upsert)
    async fn upsert_vote(&self, company_id: Uuid, issue_id: Uuid, input: &CreateFeedbackVoteInput) -> Result<FeedbackVote, RepositoryError>;

    /// List feedback votes for an issue
    async fn list_votes(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<FeedbackVote>, RepositoryError>;

    /// Get a specific vote
    async fn get_vote(&self, vote_id: Uuid) -> Result<Option<FeedbackVote>, RepositoryError>;

    /// Delete a feedback vote
    async fn delete_vote(&self, vote_id: Uuid) -> Result<(), RepositoryError>;
}

// ─── Feedback Trace ────────────────────────────────────────────

#[async_trait]
pub trait FeedbackTraceRepository: Send + Sync {
    /// Create a feedback trace
    async fn create_trace(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        vote_id: Uuid,
        target_type: &str,
        target_id: Option<Uuid>,
        payload: &serde_json::Value,
    ) -> Result<FeedbackTrace, RepositoryError>;

    /// List feedback traces for an issue
    async fn list_traces(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<FeedbackTrace>, RepositoryError>;

    /// Get a trace bundle (trace + vote + issue info)
    async fn get_trace_bundle(&self, trace_id: Uuid) -> Result<Option<FeedbackTraceBundle>, RepositoryError>;

    /// Update trace status
    async fn update_trace_status(&self, trace_id: Uuid, status: &str, failure_reason: Option<&str>) -> Result<(), RepositoryError>;
}

// ─── Recovery Action ───────────────────────────────────────────

#[async_trait]
pub trait RecoveryActionRepository: Send + Sync {
    /// Create a recovery action
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreateRecoveryActionInput) -> Result<RecoveryAction, RepositoryError>;

    /// List recovery actions for an issue
    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError>;

    /// List pending recovery actions (for monitor scheduler)
    async fn list_pending(&self, company_id: Uuid, limit: i64) -> Result<Vec<RecoveryAction>, RepositoryError>;

    /// Resolve a recovery action
    async fn resolve(&self, action_id: Uuid, input: &ResolveRecoveryActionInput) -> Result<RecoveryAction, RepositoryError>;

    /// Reconcile recovery actions for an issue and its ancestors
    async fn reconcile_for_issue_and_ancestors(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError>;

    /// Resolve active recovery actions for an issue
    async fn resolve_active_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<RecoveryAction>, RepositoryError>;
}

// ─── Plan Decomposition ────────────────────────────────────────

#[async_trait]
pub trait PlanDecompositionRepository: Send + Sync {
    /// Create a plan decomposition
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreatePlanDecompositionInput) -> Result<PlanDecomposition, RepositoryError>;

    /// List plan decompositions for an issue
    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<PlanDecomposition>, RepositoryError>;

    /// Accept a plan decomposition
    async fn accept(&self, id: Uuid, input: &AcceptPlanDecompositionInput) -> Result<PlanDecomposition, RepositoryError>;

    /// Get a specific plan decomposition
    async fn get_by_id(&self, id: Uuid) -> Result<Option<PlanDecomposition>, RepositoryError>;

    /// Delete a plan decomposition
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
}
