use async_trait::async_trait;
use models::{
    IssueTreeHold, IssueTreeHoldMember, IssueTreeControlMode, IssueTreeHoldStatus,
    IssueTreeControlPreview, IssueTreePreviewIssue,
    CreateIssueTreeHoldInput,
    ActiveIssueTreePauseHoldGate, Issue, IssueStatus,
};
use uuid::Uuid;
use std::sync::Arc;
use std::collections::HashSet;
use repositories::{
    IssueTreeHoldRepository, IssueRepository, CreateTreeHoldInput,
    ReleaseTreeHoldInput, RepositoryError,
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

    /// Calculate issue depth by counting parent chain
    async fn calculate_issue_depth(&self, issue: &Issue) -> TreeControlServiceResult<i32> {
        let mut depth = 0;
        let mut current_parent = issue.parent_id;
        
        while let Some(parent_id) = current_parent {
            depth += 1;
            if depth > self.max_tree_depth {
                // Prevent infinite loop
                break;
            }
            
            match self.issue_repository.get_by_id(parent_id).await? {
                Some(parent) => current_parent = parent.parent_id,
                None => break, // Parent not found, stop
            }
        }
        
        Ok(depth)
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
        mode: &IssueTreeControlMode,
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
                // Can restore Cancelled back to Backlog
                match current_status {
                    IssueStatus::Cancelled => Ok(Some(IssueStatus::Backlog)),
                    _ => Err(TreeControlServiceError::InvalidOperation(
                        format!("Cannot restore issue with status {:?}", current_status),
                    )),
                }
            }
            IssueTreeControlMode::Pause => {
                // A pause hold suppresses execution; the issue keeps its own
                // status so the release path can restore it exactly.
                match current_status {
                    IssueStatus::Done | IssueStatus::Cancelled => Ok(None),
                    _ => Ok(None),
                }
            }
            IssueTreeControlMode::Resume => {
                // Resume clears a pause hold; it never changes issue status.
                Ok(None)
            }
        }
    }

    /// Apply the tree-wide status transition for a freshly created hold.
    ///
    /// Returns the ids of the issues whose status actually changed. Per-issue
    /// rejections (for example cancelling a `done` issue) are skipped rather
    /// than aborting the rest of the subtree, matching Paperclip's preview
    /// semantics where such members are recorded as skipped.
    async fn apply_hold_transition(
        &self,
        hold: &IssueTreeHold,
        tree_issues: &[Issue],
    ) -> TreeControlServiceResult<Vec<Uuid>> {
        let mut updated = Vec::new();
        for issue in tree_issues {
            let target = match self.validate_mode_transition(&hold.mode, &issue.status) {
                Ok(Some(target)) => target,
                Ok(None) => continue,
                Err(_) => continue,
            };
            self.issue_repository
                .update(
                    issue.id,
                    models::UpdateIssueInput {
                        status: Some(target),
                        ..Default::default()
                    },
                )
                .await?;
            updated.push(issue.id);
        }
        Ok(updated)
    }

    /// Restore every non-skipped member to the status it held before the hold
    /// was applied.
    ///
    /// Members without a recorded `previous_status` fall back to the status
    /// captured in the member snapshot. Unparseable statuses are left alone
    /// instead of being coerced to a guessed value.
    async fn restore_hold_members(
        &self,
        hold_id: Uuid,
        members: &[IssueTreeHoldMember],
    ) -> TreeControlServiceResult<Vec<Uuid>> {
        let mut restored = Vec::new();
        for member in members {
            if member.skipped {
                continue;
            }
            let target_text = member
                .previous_status
                .clone()
                .unwrap_or_else(|| member.issue_status.clone());
            let Some(target) = parse_issue_status(&target_text) else {
                continue;
            };
            self.issue_repository
                .update(
                    member.issue_id,
                    models::UpdateIssueInput {
                        status: Some(target),
                        ..Default::default()
                    },
                )
                .await?;
            let _ = self
                .tree_hold_repository
                .mark_member_restored(hold_id, member.issue_id)
                .await;
            restored.push(member.issue_id);
        }
        Ok(restored)
    }
}

/// Parse an `IssueStatus` from its canonical snake_case text.
///
/// Hold members are persisted with `Display` text, so the release path has to
/// map that text back to the enum. Unknown text yields `None` and the member is
/// left untouched rather than being coerced to a guessed status.
fn parse_issue_status(raw: &str) -> Option<IssueStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "backlog" => Some(IssueStatus::Backlog),
        "todo" => Some(IssueStatus::Todo),
        "in_progress" | "inprogress" => Some(IssueStatus::InProgress),
        "in_review" | "inreview" => Some(IssueStatus::InReview),
        "blocked" => Some(IssueStatus::Blocked),
        "done" => Some(IssueStatus::Done),
        "cancelled" | "canceled" => Some(IssueStatus::Cancelled),
        _ => None,
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
            let transition_result = self.validate_mode_transition(&mode, &issue.status);

            match transition_result {
                Ok(target_status) => {
                    affected_issues.push(IssueTreePreviewIssue {
                        issue_id: issue.id,
                        current_status: issue.status.to_string(),
                        target_status: target_status.map(|s| s.to_string()).unwrap_or_else(|| "no_change".to_string()),
                    });
                }
                Err(_e) => {
                    status_changes.push(IssueTreePreviewIssue {
                        issue_id: issue.id,
                        current_status: issue.status.to_string(),
                        target_status: "error".to_string(),
                    });
                }
            }
        }

        // Note: Active run tracking requires Run repository integration.
        // Currently no runs are reported. Future implementation should query:
        // SELECT id, status FROM runs WHERE issue_id IN (...) AND status IN ('running', 'paused')
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
        let release_policy = input.release_policy.clone();
        let release_policy_json = serde_json::to_value(&release_policy)
            .map_err(|e| TreeControlServiceError::Validation(format!("Invalid release policy: {}", e)))?;

        // Create tree hold
        let hold_mode = input.mode.clone();
        let (created_by_agent_id, created_by_user_id) = match actor_type.as_deref() {
            Some("agent") => (actor_id, None),
            Some("user") => (None, actor_id.map(|id| id.to_string())),
            _ => (None, None),
        };
        let create_input = CreateTreeHoldInput {
            company_id,
            root_issue_id,
            mode: hold_mode.clone(),
            reason: input.reason,
            release_policy: release_policy_json,
            metadata: input.metadata,
            created_by_actor_type: actor_type,
            created_by_agent_id,
            created_by_user_id,
            created_by_run_id: None,
        };

        let hold = self.tree_hold_repository.create(create_input).await?;

        let tree_issues = self.collect_tree_issues(root_issue_id).await?;
        let mut members = Vec::new();
        for issue in &tree_issues {
            let transition = self.validate_mode_transition(&input.mode, &issue.status);
            let (skipped, skip_reason) = match &transition {
                Ok(None) => (true, Some("Already in target state".to_string())),
                Ok(Some(_)) => (false, None),
                Err(e) => (true, Some(e.to_string())),
            };
            let previous_status = issue.status.to_string();
            let issue_status = previous_status.clone();

            members.push(IssueTreeHoldMember {
                id: Uuid::new_v4(),
                company_id,
                hold_id: hold.id,
                issue_id: issue.id,
                parent_issue_id: issue.parent_id,
                previous_status: Some(previous_status),
                depth: self.calculate_issue_depth(&issue).await.unwrap_or(0),
                issue_identifier: issue.identifier.clone(),
                issue_title: issue.title.clone(),
                issue_status,
                assignee_agent_id: issue.assignee_agent_id,
                assignee_user_id: issue.assignee_user_id,
                active_run_id: None,
                active_run_status: None,
                skipped,
                skip_reason: skip_reason.clone(),
                restored_at: None,
                created_at: chrono::Utc::now(),
            });
        }

        // Create members
        self.tree_hold_repository.create_members(members).await?;

        // Apply the tree-wide status transition. A failure leaves the hold
        // active with `apply_error` set so an operator can retry instead of
        // silently leaving the subtree half-applied.
        if let Err(error) = self.apply_hold_transition(&hold, &tree_issues).await {
            tracing::warn!(
                hold_id = %hold.id,
                mode = ?hold.mode,
                error = %error,
                "Failed to apply tree control transition"
            );
            let _ = self
                .tree_hold_repository
                .set_apply_error(hold.id, Some(error.to_string()))
                .await;
        }

        self.tree_hold_repository
            .get_by_id(hold.id)
            .await?
            .ok_or(TreeControlServiceError::HoldNotFound(hold.id))
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

        // Restore each member to the status it held before the hold was
        // applied. Only cancel/restore holds change issue status; pause holds
        // suppress execution without mutating status, so their members are
        // restored to the same value and the restore is a no-op.
        let members = self.tree_hold_repository.get_members(hold_id).await?;
        let restored = self
            .restore_hold_members(hold_id, &members)
            .await?;

        let (released_agent_id, released_user_id) = match released_by_type.as_deref() {
            Some("agent") => (released_by_id, None),
            Some(_) => (None, released_by_id.map(|id| id.to_string())),
            None => (None, None),
        };

        // Release hold with full attribution and a restore summary so the
        // release is auditable without re-reading every member row.
        let released_hold = self
            .tree_hold_repository
            .release_with_actor(
                hold_id,
                ReleaseTreeHoldInput {
                    released_by_actor_type: released_by_type,
                    released_by_agent_id: released_agent_id,
                    released_by_user_id: released_user_id,
                    released_by_run_id: None,
                    release_reason: None,
                    release_metadata: Some(serde_json::json!({
                        "restoredIssueIds": restored,
                        "restoredCount": restored.len(),
                    })),
                },
            )
            .await?;

        Ok(released_hold)
    }

    async fn get_pause_state(&self, issue_id: Uuid) -> TreeControlServiceResult<Option<ActiveIssueTreePauseHoldGate>> {
        // Get active holds for this issue
        let active_holds = self.tree_hold_repository.list_active_for_issue(issue_id).await?;

        // Find pause holds. Paperclip also reports whether the gated issue is
        // the hold root, so callers can distinguish a directly-held issue from
        // one paused by ancestor propagation.
        for hold in active_holds {
            if hold.mode == IssueTreeControlMode::Pause {
                return Ok(Some(ActiveIssueTreePauseHoldGate {
                    hold_id: hold.id,
                    root_issue_id: hold.root_issue_id,
                    issue_id,
                    is_root: hold.root_issue_id == issue_id,
                    mode: hold.mode,
                    reason: hold.reason,
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
