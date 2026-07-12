use async_trait::async_trait;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeControlMode, IssueTreeHoldStatus,
    IssueTreeControlPreview, IssueTreePreviewIssue, IssueTreePreviewRun, IssueTreePreviewWarning,
    CreateIssueTreeHoldInput, IssueTreeHoldReleasePolicy, HoldReleasePolicyStrategy,
    ActiveIssueTreePauseHoldGate, Issue, IssueStatus,
};
use uuid::Uuid;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use repositories::{
    IssueTreeHoldRepository, IssueRepository, CreateTreeHoldInput,
    RepositoryError,
};

/// Service-level errors for Tree Control operations
#[derive(Debug, thiserror::Error)]
pub enum TreeControlServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Hold not found: {0}")]
    HoldNotFound(Uuid),

    #[error("Issue not found: {0}")]
    IssueNotFound(Uuid),

    #[error("Hold already released")]
    HoldAlreadyReleased,

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type TreeControlServiceResult<T> = Result<T, TreeControlServiceError>;

/// Issue Tree Control Service trait
#[async_trait]
pub trait IssueTreeControlService: Send + Sync {
    /// Preview tree control effect before applying
    async fn preview_tree_hold(
        &self,
        root_issue_id: Uuid,
        mode: IssueTreeControlMode,
    ) -> TreeControlServiceResult<IssueTreeControlPreview>;

    /// Create a tree hold
    async fn create_tree_hold(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        input: CreateIssueTreeHoldInput,
        actor_type: Option<String>,
        actor_id: Option<Uuid>,
    ) -> TreeControlServiceResult<IssueTreeHold>;

    /// Get a tree hold by ID
    async fn get_tree_hold(&self, hold_id: Uuid) -> TreeControlServiceResult<IssueTreeHold>;

    /// List tree holds for a root issue
    async fn list_tree_holds(&self, root_issue_id: Uuid) -> TreeControlServiceResult<Vec<IssueTreeHold>>;

    /// Release a tree hold
    async fn release_tree_hold(
        &self,
        hold_id: Uuid,
        released_by_type: Option<String>,
        released_by_id: Option<Uuid>,
    ) -> TreeControlServiceResult<IssueTreeHold>;

    /// Get current pause state for an issue
    async fn get_pause_state(&self, issue_id: Uuid) -> TreeControlServiceResult<Option<ActiveIssueTreePauseHoldGate>>;

    /// Get hold members
    async fn get_hold_members(&self, hold_id: Uuid) -> TreeControlServiceResult<Vec<IssueTreeHoldMember>>;
}

/// Issue Tree Control Service implementation
pub struct IssueTreeControlServiceImpl<THR, IR>
where
    THR: IssueTreeHoldRepository,
    IR: IssueRepository,
{
    tree_hold_repository: Arc<THR>,
    issue_repository: Arc<IR>,
    max_tree_depth: i32,
}

impl<THR, IR> IssueTreeControlServiceImpl<THR, IR>
where
    THR: IssueTreeHoldRepository,
    IR: IssueRepository,
{
    pub fn new(tree_hold_repository: Arc<THR>, issue_repository: Arc<IR>) -> Self {
        Self {
            tree_hold_repository,
            issue_repository,
            max_tree_depth: 10,
        }
    }

    pub fn with_max_depth(mut self, max_depth: i32) -> Self {
        self.max_tree_depth = max_depth;
        self
    }

    /// Recursively collect all descendant issues (tree traversal)
    async fn collect_tree_issues(
        &self,
        root_id: Uuid,
    ) -> TreeControlServiceResult<Vec<Issue>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = vec![(root_id, 0)]; // (issue_id, depth)

        while let Some((current_id, depth)) = queue.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id);

            if depth > self.max_tree_depth {
                continue;
            }

            let issue = self.issue_repository.get_by_id(current_id).await?;
            if let Some(issue) = issue {
                result.push(issue.clone());

                // Get children
                let children = self.issue_repository.list_children(current_id).await?;
                for child in children {
                    queue.push((child.id, depth + 1));
                }
            }
        }

        Ok(result)
    }

    /// Validate if a tree control mode can be applied
    fn validate_mode_transition(
        &self,
        mode: IssueTreeControlMode,
        current_status: &IssueStatus,
    ) -> TreeControlServiceResult<Option<IssueStatus>> {
        match mode {
            IssueTreeControlMode::Cancel => {
                // Can cancel anything except Done
                match current_status {
                    IssueStatus::Done => Err(TreeControlServiceError::InvalidOperation(
                        "Cannot cancel completed issue".to_string(),
                    )),
                    IssueStatus::Cancelled => Ok(None), // Already canceled
                    _ => Ok(Some(IssueStatus::Cancelled)),
                }
            }
            IssueTreeControlMode::Restore => {
                // Can restore Cancelled
                match current_status {
                    IssueStatus::Cancelled => Ok(Some(IssueStatus::Backlog)),
                    _ => Err(TreeControlServiceError::InvalidOperation(
                        format!("Cannot restore issue with status {:?}", current_status),
                    )),
                }
            }
            _ => Err(TreeControlServiceError::InvalidOperation(
                format!("Unsupported control mode: {:?}", mode),
            )),
        }
    }
}

#[async_trait]
impl<THR, IR> IssueTreeControlService for IssueTreeControlServiceImpl<THR, IR>
where
    THR: IssueTreeHoldRepository,
    IR: IssueRepository,
{
    async fn preview_tree_hold(
        &self,
        root_issue_id: Uuid,
        mode: IssueTreeControlMode,
    ) -> TreeControlServiceResult<IssueTreeControlPreview> {
        // Collect all issues in the tree
        let tree_issues = self.collect_tree_issues(root_issue_id).await?;

        let mut affected_issues = Vec::new();
        let mut status_changes = Vec::new();

        for issue in tree_issues {
            let transition_result = self.validate_mode_transition(mode, &issue.status);

            match transition_result {
                Ok(target_status) => {
                    affected_issues.push(IssueTreePreviewIssue {
                        issue_id: issue.id,
                        current_status: issue.status.to_string(),
                        target_status: target_status.map(|s| s.to_string()).unwrap_or_else(|| "no_change".to_string()),
                    });
                }
                Err(e) => {
                    status_changes.push(IssueTreePreviewIssue {
                        issue_id: issue.id,
                        current_status: issue.status.to_string(),
                        target_status: "error".to_string(),
                    });
                }
            }
        }

        // TODO: Get active runs for affected issues
        let active_runs = Vec::new();

        Ok(IssueTreeControlPreview {
            affected_issues,
            active_runs,
            status_changes,
        })
    }

    async fn create_tree_hold(
        &self,
        company_id: Uuid,
        root_issue_id: Uuid,
        input: CreateIssueTreeHoldInput,
        actor_type: Option<String>,
        actor_id: Option<Uuid>,
    ) -> TreeControlServiceResult<IssueTreeHold> {
        // Verify root issue exists
        let root_issue = self.issue_repository.get_by_id(root_issue_id).await?;
        if root_issue.is_none() {
            return Err(TreeControlServiceError::IssueNotFound(root_issue_id));
        }

        // Default release policy
        let release_policy = input.release_policy;

        let release_policy_json = serde_json::to_value(&release_policy)
            .map_err(|e| TreeControlServiceError::Validation(format!("Invalid release policy: {}", e)))?;

        // Create tree hold
        let create_input = CreateTreeHoldInput {
            company_id,
            root_issue_id,
            mode: input.mode,
            reason: input.reason,
            release_policy: release_policy_json,
            metadata: input.metadata,
            actor_type,
            actor_id,
        };

        let hold = self.tree_hold_repository.create(create_input).await?;

        // Collect tree members
        let tree_issues = self.collect_tree_issues(root_issue_id).await?;
        let mut members = Vec::new();

        for issue in tree_issues {
            let transition_result = self.validate_mode_transition(input.mode, &issue.status);
            let (skipped, skip_reason) = match transition_result {
                Ok(None) => (true, Some("Already in target state".to_string())),
                Ok(Some(_)) => (false, None),
                Err(e) => (true, Some(e.to_string())),
            };

            members.push(IssueTreeHoldMember {
                id: Uuid::new_v4(),
                company_id,
                hold_id: hold.id,
                issue_id: issue.id,
                parent_issue_id: issue.parent_id,
                previous_status: format!("{:?}", issue.status),
                depth: 0, // TODO: calculate actual depth
                issue_identifier: issue.identifier,
                issue_title: issue.title,
                issue_status: format!("{:?}", issue.status),
                assignee_agent_id: issue.assignee_agent_id,
                assignee_user_id: issue.assignee_user_id,
                active_run_id: None, // TODO: get active run
                active_run_status: None,
                skipped,
                skip_reason,
                created_at: chrono::Utc::now(),
            });
        }

        // Create members
        self.tree_hold_repository.create_members(members).await?;

        Ok(hold)
    }

    async fn get_tree_hold(&self, hold_id: Uuid) -> TreeControlServiceResult<IssueTreeHold> {
        let hold = self.tree_hold_repository.get_by_id(hold_id).await?;

        match hold {
            Some(h) => Ok(h),
            None => Err(TreeControlServiceError::HoldNotFound(hold_id)),
        }
    }

    async fn list_tree_holds(&self, root_issue_id: Uuid) -> TreeControlServiceResult<Vec<IssueTreeHold>> {
        let holds = self.tree_hold_repository.list_by_root_issue(root_issue_id).await?;
        Ok(holds)
    }

    async fn release_tree_hold(
        &self,
        hold_id: Uuid,
        released_by_type: Option<String>,
        released_by_id: Option<Uuid>,
    ) -> TreeControlServiceResult<IssueTreeHold> {
        // Get hold
        let hold = self.get_tree_hold(hold_id).await?;

        // Check if already released
        if hold.status == IssueTreeHoldStatus::Released {
            return Err(TreeControlServiceError::HoldAlreadyReleased);
        }

        // Release hold
        let released_hold = self.tree_hold_repository.release(
            hold_id,
            released_by_type,
            released_by_id,
        ).await?;

        Ok(released_hold)
    }

    async fn get_pause_state(&self, issue_id: Uuid) -> TreeControlServiceResult<Option<ActiveIssueTreePauseHoldGate>> {
        // Get active holds for this issue
        let active_holds = self.tree_hold_repository.list_active_for_issue(issue_id).await?;

        // Find pause holds
        for hold in active_holds {
            if hold.mode == IssueTreeControlMode::Pause {
                return Ok(Some(ActiveIssueTreePauseHoldGate {
                    hold_id: hold.id,
                    root_issue_id: hold.root_issue_id,
                    mode: hold.mode,
                    release_policy: hold.release_policy.0,
                    created_at: hold.created_at,
                }));
            }
        }

        Ok(None)
    }

    async fn get_hold_members(&self, hold_id: Uuid) -> TreeControlServiceResult<Vec<IssueTreeHoldMember>> {
        // Verify hold exists
        let _ = self.get_tree_hold(hold_id).await?;

        let members = self.tree_hold_repository.get_members(hold_id).await?;
        Ok(members)
    }
}
