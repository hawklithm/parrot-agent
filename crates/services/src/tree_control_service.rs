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
            affected_issues: vec![
                AffectedIssue {
                    issue_id,
                    current_status: "in_progress".to_string(),
                    target_status: "paused".to_string(),
                },
            ],
            active_runs: vec![
                PreviewActiveRun {
                    run_id: Uuid::new_v4(),
                    agent_id: Some(Uuid::new_v4()),
                    issue_id,
                },
            ],
            status_changes: vec![
                AffectedIssue {
                    issue_id,
                    current_status: "in_progress".to_string(),
                    target_status: "paused".to_string(),
                },
            ],
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
        Ok(IssueTreeHold {
            id: Uuid::new_v4(),
            company_id,
            root_issue_id: issue_id,
            mode: input.mode,
            status: models::IssueTreeHoldStatus::Active,
            reason: input.reason,
            release_policy: sqlx::types::Json(input.release_policy),
            metadata: input.metadata,
            actor_agent_id: agent_id,
            actor_user_id: user_id,
            created_at: chrono::Utc::now(),
            released_at: None,
        })
    }
    
    async fn get_hold_state(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<IssueTreeHold>, String> {
        Ok(Some(IssueTreeHold {
            id: Uuid::new_v4(),
            company_id,
            root_issue_id: issue_id,
            mode: models::IssueTreeControlMode::Pause,
            status: models::IssueTreeHoldStatus::Active,
            reason: Some("Mock hold".to_string()),
            release_policy: sqlx::types::Json(models::IssueTreeHoldReleasePolicy {
                strategy: models::IssueTreeHoldReleasePolicyStrategy::Manual,
                note: None,
            }),
            metadata: None,
            actor_agent_id: None,
            actor_user_id: None,
            created_at: chrono::Utc::now(),
            released_at: None,
        }))
    }
    
    async fn list_holds(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueTreeHold>, String> {
        Ok(vec![
            IssueTreeHold {
                id: Uuid::new_v4(),
                company_id,
                root_issue_id: issue_id,
                mode: models::IssueTreeControlMode::Pause,
                status: models::IssueTreeHoldStatus::Active,
                reason: Some("Mock hold 1".to_string()),
                release_policy: sqlx::types::Json(models::IssueTreeHoldReleasePolicy {
                    strategy: models::IssueTreeHoldReleasePolicyStrategy::Manual,
                    note: None,
                }),
                metadata: None,
                actor_agent_id: None,
                actor_user_id: None,
                created_at: chrono::Utc::now(),
                released_at: None,
            },
        ])
    }
    
    async fn release_hold(
        &self,
        issue_id: Uuid,
        hold_id: Uuid,
        company_id: Uuid,
        _agent_id: Option<Uuid>,
        _user_id: Option<Uuid>,
    ) -> Result<IssueTreeHold, String> {
        Ok(IssueTreeHold {
            id: hold_id,
            company_id,
            root_issue_id: issue_id,
            mode: models::IssueTreeControlMode::Resume,
            status: models::IssueTreeHoldStatus::Released,
            reason: Some("Released".to_string()),
                release_policy: sqlx::types::Json(models::IssueTreeHoldReleasePolicy {
                    strategy: models::IssueTreeHoldReleasePolicyStrategy::Manual,
                    note: None,
                }),
            metadata: None,
            actor_agent_id: None,
            actor_user_id: None,
            created_at: chrono::Utc::now(),
            released_at: Some(chrono::Utc::now()),
        })
    }
}
