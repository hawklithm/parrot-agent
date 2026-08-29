use async_trait::async_trait;
use uuid::Uuid;
use models::{
    IssueTreeHold, CreateIssueTreeHoldInput,
    IssueTreeControlPreview, AffectedIssue, PreviewActiveRun,
};

/// Tree control service trait for issue tree operations
#[async_trait]
pub trait TreeControlService: Send + Sync {
    /// Preview tree control impact
    async fn preview(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: &CreateIssueTreeHoldInput,
    ) -> Result<IssueTreeControlPreview, String>;

    /// Create tree hold
    async fn create_hold(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateIssueTreeHoldInput,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, String>;

    /// Get tree hold state
    async fn get_hold_state(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<IssueTreeHold>, String>;

    /// List tree holds for an issue
    async fn list_holds(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueTreeHold>, String>;

    /// Release tree hold
    async fn release_hold(
        &self,
        issue_id: Uuid,
        hold_id: Uuid,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, String>;
}

/// Mock implementation of TreeControlService
pub struct MockTreeControlService;

impl MockTreeControlService {
    pub fn new() -> Self {
        Self
    }

    /// Build a hold row with every Paperclip attribution field populated, so
    /// the mock exercises the same shape the PostgreSQL repository returns.
    fn mock_hold(
        id: Uuid,
        company_id: Uuid,
        root_issue_id: Uuid,
        mode: models::IssueTreeControlMode,
        status: models::IssueTreeHoldStatus,
        reason: Option<String>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
        released_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> IssueTreeHold {
        let now = chrono::Utc::now();
        IssueTreeHold {
            id,
            company_id,
            root_issue_id,
            mode,
            status,
            reason,
            release_policy: sqlx::types::Json(models::IssueTreeHoldReleasePolicy {
                strategy: models::IssueTreeHoldReleasePolicyStrategy::Manual,
                note: None,
            }),
            metadata: None,
            created_by_actor_type: if created_by_agent_id.is_some() {
                "agent".to_string()
            } else {
                "user".to_string()
            },
            created_by_agent_id,
            created_by_user_id: created_by_user_id.map(|id| id.to_string()),
            created_by_run_id: None,
            created_at: now,
            updated_at: now,
            released_at,
            released_by_actor_type: None,
            released_by_agent_id: None,
            released_by_user_id: None,
            released_by_run_id: None,
            release_reason: None,
            release_metadata: None,
            apply_error: None,
        }
    }
}

impl Default for MockTreeControlService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TreeControlService for MockTreeControlService {
    async fn preview(
        &self,
        issue_id: Uuid,
        _company_id: Uuid,
        _input: &CreateIssueTreeHoldInput,
    ) -> Result<IssueTreeControlPreview, String> {
        Ok(IssueTreeControlPreview {
            affected_issues: vec![AffectedIssue {
                issue_id,
                current_status: "in_progress".to_string(),
                target_status: "paused".to_string(),
            }],
            active_runs: vec![PreviewActiveRun {
                run_id: Uuid::new_v4(),
                agent_id: Some(Uuid::new_v4()),
                issue_id,
            }],
            status_changes: vec![AffectedIssue {
                issue_id,
                current_status: "in_progress".to_string(),
                target_status: "paused".to_string(),
            }],
        })
    }

    async fn create_hold(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateIssueTreeHoldInput,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, String> {
        Ok(Self::mock_hold(
            Uuid::new_v4(),
            company_id,
            issue_id,
            input.mode,
            models::IssueTreeHoldStatus::Active,
            input.reason,
            agent_id,
            user_id,
            None,
        ))
    }

    async fn get_hold_state(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<IssueTreeHold>, String> {
        Ok(Some(Self::mock_hold(
            Uuid::new_v4(),
            company_id,
            issue_id,
            models::IssueTreeControlMode::Pause,
            models::IssueTreeHoldStatus::Active,
            Some("Mock hold".to_string()),
            None,
            None,
            None,
        )))
    }

    async fn list_holds(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueTreeHold>, String> {
        Ok(vec![Self::mock_hold(
            Uuid::new_v4(),
            company_id,
            issue_id,
            models::IssueTreeControlMode::Pause,
            models::IssueTreeHoldStatus::Active,
            Some("Mock hold 1".to_string()),
            None,
            None,
            None,
        )])
    }

    async fn release_hold(
        &self,
        issue_id: Uuid,
        hold_id: Uuid,
        company_id: Uuid,
        _agent_id: Option<Uuid>,
        _user_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, String> {
        Ok(Self::mock_hold(
            hold_id,
            company_id,
            issue_id,
            models::IssueTreeControlMode::Resume,
            models::IssueTreeHoldStatus::Released,
            Some("Released".to_string()),
            None,
            None,
            Some(chrono::Utc::now()),
        ))
    }
}
