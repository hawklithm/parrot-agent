//! Task watchdog service.
//!
//! Ports paperclip's `task-watchdogs.ts`: a watchdog evaluates a watched
//! issue's subtree for liveness, stopping, and review state, then maintains a
//! dedicated "watchdog issue" (origin_kind = `task_watchdog`) that is reopened
//! when the subtree stops so an agent can verify the disposition.

use async_trait::async_trait;
use chrono::Utc;
use repositories::{
    AgentWakeupRequestRepository, HeartbeatRunRepository, IssueThreadInteractionRepository,
    IssueWatchdogRepository, IssueRepository,
};
use models::{CreateIssueInput, UpdateIssueInput};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

use models::issue::{IssueStatus};
use models::task_watchdog::{IssueThreadInteraction, TaskWatchdogClassifierIssue};
use repositories::RepositoryResult;

/// Origin kind for the synthetic "watchdog review" issue paperclip creates
/// under a stopped subtree.
pub const TASK_WATCHDOG_ORIGIN_KIND: &str = "task_watchdog";

/// First-run grace window (ms): suppress a stopped verdict for issues created
/// within this window that have never completed a run.
pub const TASK_WATCHDOG_FIRST_RUN_GRACE_MS: i64 = 30_000;

// ============================================================================
// Classifier (ports classifyTaskWatchdogSubtree)
// ============================================================================

#[derive(Debug, Clone)]
pub struct StoppedLeaf {
    pub issue_id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub status: String,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
    pub blocker_issue_ids: Vec<Uuid>,
    pub pending_interaction_ids: Vec<Uuid>,
    pub pending_approval_ids: Vec<Uuid>,
    pub updated_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum ClassifierState {
    NotApplicable { reason: String },
    Live { reason: String, live_issue_ids: Vec<Uuid> },
    PendingFirstRun { reason: String, pending_issue_ids: Vec<Uuid> },
    AlreadyReviewed { reason: String, stop_fingerprint: String, stopped_leaves: Vec<StoppedLeaf> },
    Stopped { reason: String, stop_fingerprint: String, stopped_leaves: Vec<StoppedLeaf> },
}

#[derive(Debug, Clone)]
pub struct ClassifierInput {
    pub watchdog_issue_id: Uuid,
    pub company_id: Uuid,
    pub last_reviewed_fingerprint: Option<String>,
    pub issues: Vec<TaskWatchdogClassifierIssue>,
    /// Live run issue/task ids (from heartbeat_runs context).
    pub live_run_issue_ids: Vec<Uuid>,
    /// Live wake request issue/task ids (from agent_wakeup_requests payload).
    pub live_wake_issue_ids: Vec<Uuid>,
    /// blocker relations within the subtree: (blocked_issue, blocker_issue).
    pub blockers: Vec<(Uuid, Uuid)>,
    /// pending thread interaction issue ids.
    pub pending_interaction_issue_ids: Vec<Uuid>,
    /// pending approval issue ids.
    pub pending_approval_issue_ids: Vec<Uuid>,
    /// issue ids that have a terminal run.
    pub completed_run_issue_ids: Vec<Uuid>,
    pub evaluated_at: chrono::DateTime<Utc>,
}

fn to_epoch_ms(dt: &chrono::DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

/// Build a stable fingerprint for a stopped subtree from its leaves.
/// Mirrors paperclip's `stableStopFingerprint`.
fn stable_stop_fingerprint(
    company_id: Uuid,
    watched_issue_id: Uuid,
    leaves: &[StoppedLeaf],
) -> String {
    let mut leaf_parts: Vec<String> = leaves
        .iter()
        .map(|leaf| {
            let blockers = leaf
                .blocker_issue_ids
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let interactions = leaf
                .pending_interaction_ids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let approvals = leaf
                .pending_approval_ids
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}:{}:{}:{}:{}:{}",
                leaf.issue_id, leaf.status, blockers, interactions, approvals, leaf.title
            )
        })
        .collect();
    leaf_parts.sort();
    format!(
        "stop:{}:{}:{}",
        company_id,
        watched_issue_id,
        leaf_parts.join("|")
    )
}

pub fn classify_subtree(input: ClassifierInput) -> ClassifierState {
    let issues_by_id: HashMap<Uuid, &TaskWatchdogClassifierIssue> =
        input.issues.iter().map(|i| (i.id, i)).collect();
    let root = match issues_by_id.get(&input.watchdog_issue_id) {
        Some(r) => r,
        None => {
            return ClassifierState::NotApplicable {
                reason: "Watched issue is missing from subtree.".to_string(),
            }
        }
    };
    if root.origin_kind.as_deref() == Some(TASK_WATCHDOG_ORIGIN_KIND) {
        return ClassifierState::NotApplicable {
            reason: "Task watchdog origin issues cannot themselves be watched.".to_string(),
        };
    }

    // Build children map and DFS-collect included (non-watchdog) issues.
    let mut children_by_parent: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for issue in &input.issues {
        if let Some(pid) = issue.parent_id {
            children_by_parent.entry(pid).or_default().push(issue.id);
        }
    }

    let mut included: Vec<Uuid> = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = vec![input.watchdog_issue_id];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(issue) = issues_by_id.get(&id) {
            if issue.origin_kind.as_deref() == Some(TASK_WATCHDOG_ORIGIN_KIND) {
                continue;
            }
            included.push(id);
            if let Some(children) = children_by_parent.get(&id) {
                for c in children {
                    stack.push(*c);
                }
            }
        }
    }

    if included.is_empty() {
        return ClassifierState::NotApplicable {
            reason: "Watched subtree has no non-watchdog issues.".to_string(),
        };
    }

    let included_set: HashSet<Uuid> = included.iter().copied().collect();

    // Live detection: any included issue with a live run or queued wake.
    let mut live_issue_ids: Vec<Uuid> = input
        .live_run_issue_ids
        .iter()
        .chain(input.live_wake_issue_ids.iter())
        .copied()
        .filter(|id| included_set.contains(id))
        .collect();
    live_issue_ids.sort();
    live_issue_ids.dedup();
    if !live_issue_ids.is_empty() {
        return ClassifierState::Live {
            reason: "At least one issue in the watched subtree has a live run, queued wake, or scheduled retry.".to_string(),
            live_issue_ids,
        };
    }

    // Pending-first-run guard.
    let completed_set: HashSet<Uuid> = input.completed_run_issue_ids.iter().copied().collect();
    let mut pending_issue_ids: Vec<Uuid> = included
        .iter()
        .copied()
        .filter(|id| {
            let issue = &issues_by_id[id];
            let status = issue.status.as_str();
            if matches!(
                status,
                "done" | "cancelled" | "in_review"
            ) {
                return false;
            }
            if completed_set.contains(id) {
                return false;
            }
            if let Some(created) = issue.created_at {
                let age_ms = to_epoch_ms(&input.evaluated_at) - to_epoch_ms(&created);
                return age_ms < TASK_WATCHDOG_FIRST_RUN_GRACE_MS;
            }
            false
        })
        .collect();
    pending_issue_ids.sort();
    pending_issue_ids.dedup();
    if !pending_issue_ids.is_empty() {
        return ClassifierState::PendingFirstRun {
            reason: "A watched issue was created within the first-run grace window and has not yet completed a run; deferring evaluation.".to_string(),
            pending_issue_ids,
        };
    }

    // Build leaves (issues with no included children).
    let mut included_children_by_parent: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for id in &included {
        if let Some(issue) = issues_by_id.get(id) {
            if let Some(pid) = issue.parent_id {
                if included_set.contains(&pid) {
                    included_children_by_parent.entry(pid).or_default().push(*id);
                }
            }
        }
    }
    let mut blockers_by_issue: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (blocked, blocker) in &input.blockers {
        if included_set.contains(blocked) {
            blockers_by_issue.entry(*blocked).or_default().push(*blocker);
        }
    }
    let pending_interactions: HashSet<Uuid> =
        input.pending_interaction_issue_ids.iter().copied().collect();
    let pending_approvals: HashSet<Uuid> =
        input.pending_approval_issue_ids.iter().copied().collect();

    let mut leaves: Vec<StoppedLeaf> = included
        .iter()
        .copied()
        .filter(|id| included_children_by_parent.get(id).map_or(true, |c| c.is_empty()))
        .map(|id| {
            let issue = &issues_by_id[&id];
            StoppedLeaf {
                issue_id: issue.id,
                identifier: issue.identifier.clone(),
                title: issue.title.clone(),
                status: issue.status.clone(),
                assignee_agent_id: issue.assignee_agent_id,
                assignee_user_id: issue.assignee_user_id,
                blocker_issue_ids: blockers_by_issue.get(&id).cloned().unwrap_or_default(),
                pending_interaction_ids: if pending_interactions.contains(&id) {
                    vec![id]
                } else {
                    vec![]
                },
                pending_approval_ids: if pending_approvals.contains(&id) {
                    vec![id]
                } else {
                    vec![]
                },
                updated_at: issue.updated_at,
            }
        })
        .collect();
    leaves.sort_by_key(|l| l.issue_id);

    let stop_fingerprint =
        stable_stop_fingerprint(input.company_id, input.watchdog_issue_id, &leaves);

    if input.last_reviewed_fingerprint.as_deref() == Some(stop_fingerprint.as_str()) {
        return ClassifierState::AlreadyReviewed {
            reason: "The current stopped subtree fingerprint was already reviewed by the watchdog.".to_string(),
            stop_fingerprint,
            stopped_leaves: leaves,
        };
    }

    ClassifierState::Stopped {
        reason: "No issue in the watched subtree has a live execution path.".to_string(),
        stop_fingerprint,
        stopped_leaves: leaves,
    }
}

// ============================================================================
// Service (ports taskWatchdogService + ensureReusableWatchdogIssue)
// ============================================================================

#[async_trait]
pub trait WatchdogService: Send + Sync {
    /// Evaluate every active watchdog for a company and apply decisions.
    async fn evaluate_all(&self, company_id: Uuid) -> RepositoryResult<usize>;
    /// Evaluate a single watched issue.
    async fn evaluate(&self, watchdog: models::task_watchdog::IssueWatchdog) -> RepositoryResult<()>;
    /// Evaluate the watchdog(s) associated with a specific issue (if any).
    /// Returns the number of watchdogs evaluated.
    async fn evaluate_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> RepositoryResult<usize>;
    /// Create or update a watchdog for a watched issue.
    async fn upsert_watchdog(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        watchdog_agent_id: Uuid,
        instructions: Option<String>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<String>,
        created_by_run_id: Option<Uuid>,
    ) -> RepositoryResult<models::task_watchdog::IssueWatchdog>;
    /// Get the watchdog for a specific issue.
    async fn get_watchdog(&self, company_id: Uuid, issue_id: Uuid) -> RepositoryResult<Option<models::task_watchdog::IssueWatchdog>>;
    /// Update the status of a watchdog.
    async fn update_watchdog_status(&self, id: Uuid, status: models::task_watchdog::IssueWatchdogStatus) -> RepositoryResult<models::task_watchdog::IssueWatchdog>;
}

pub struct DefaultWatchdogService {
    issue_repo: Arc<dyn IssueRepository>,
    watchdog_repo: Arc<dyn IssueWatchdogRepository>,
    heartbeat_repo: Arc<dyn HeartbeatRunRepository>,
    wakeup_repo: Arc<dyn AgentWakeupRequestRepository>,
    interaction_repo: Arc<dyn IssueThreadInteractionRepository>,
}

impl DefaultWatchdogService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issue_repo: Arc<dyn IssueRepository>,
        watchdog_repo: Arc<dyn IssueWatchdogRepository>,
        heartbeat_repo: Arc<dyn HeartbeatRunRepository>,
        wakeup_repo: Arc<dyn AgentWakeupRequestRepository>,
        interaction_repo: Arc<dyn IssueThreadInteractionRepository>,
    ) -> Self {
        Self {
            issue_repo,
            watchdog_repo,
            heartbeat_repo,
            wakeup_repo,
            interaction_repo,
        }
    }

    /// Collect classifier input for a watchdog (subtree + runs + wakes +
    /// blockers + pending interactions/approvals).
    async fn collect_classifier_input(
        &self,
        company_id: Uuid,
        watchdog: &models::task_watchdog::IssueWatchdog,
    ) -> RepositoryResult<ClassifierInput> {
        let subtree = self
            .watchdog_repo
            .load_subtree_issues(company_id, watchdog.issue_id)
            .await?;
        let subtree_ids: Vec<Uuid> = subtree.iter().map(|i| i.id).collect();

        let live_runs = self
            .heartbeat_repo
            .list_live_by_issue_ids(company_id, &subtree_ids)
            .await?;
        let terminal_runs = self
            .heartbeat_repo
            .list_terminal_by_issue_ids(company_id, &subtree_ids)
            .await?;
        let live_wakes = self
            .wakeup_repo
            .list_live_by_issue_ids(company_id, &subtree_ids)
            .await?;
        let pending_interactions = self
            .interaction_repo
            .list_pending_by_issue_ids(company_id, &subtree_ids)
            .await?;

        let live_run_issue_ids = extract_context_issue_ids(&live_runs);
        let live_wake_issue_ids = extract_payload_issue_ids(&live_wakes);
        let completed_run_issue_ids: Vec<Uuid> = terminal_runs
            .iter()
            .filter_map(|r| extract_context_issue_ids_single(r))
            .collect();

        // Blockers: issues in subtree that have a blocker (parent dependency).
        // paperclip reads relation rows; parrot Issue has no explicit blocker
        // relation table, so derive from parent_id where parent is also in subtree.
        let subtree_set: HashSet<Uuid> = subtree_ids.iter().copied().collect();
        let blockers: Vec<(Uuid, Uuid)> = subtree
            .iter()
            .filter_map(|i| i.parent_id.filter(|p| subtree_set.contains(p)).map(|p| (i.id, p)))
            .collect();

        let pending_interaction_issue_ids: Vec<Uuid> =
            pending_interactions.iter().map(|i: &IssueThreadInteraction| i.issue_id).collect();
        // paperclip also includes pending approvals; parrot reuses the same
        // interactions table for approval-style kinds.
        let pending_approval_issue_ids: Vec<Uuid> = pending_interactions
            .iter()
            .filter(|i| i.kind == "approval" || i.kind == "review")
            .map(|i| i.issue_id)
            .collect();

        Ok(ClassifierInput {
            watchdog_issue_id: watchdog.issue_id,
            company_id,
            last_reviewed_fingerprint: watchdog.last_reviewed_fingerprint.clone(),
            issues: subtree,
            live_run_issue_ids,
            live_wake_issue_ids,
            blockers,
            pending_interaction_issue_ids,
            pending_approval_issue_ids,
            completed_run_issue_ids,
            evaluated_at: Utc::now(),
        })
    }

    /// Handle a stopped subtree: reuse/create the watchdog issue and reopen it
    /// if it is terminal/backlog. Mirrors paperclip `ensureReusableWatchdogIssue`.
    async fn handle_stopped(
        &self,
        watchdog: &models::task_watchdog::IssueWatchdog,
        classification: &ClassifierState,
    ) -> RepositoryResult<()> {
        let stop_fingerprint = match classification {
            ClassifierState::Stopped {
                stop_fingerprint,
                ..
            } => stop_fingerprint.clone(),
            _ => return Ok(()),
        };

        // Source issue (the watched issue) for parent/context.
        let source_issue = match self.issue_repo.get_by_id(watchdog.issue_id).await? {
            Some(i) => i,
            None => return Ok(()),
        };

        // Reuse existing watchdog issue if referenced or already created.
        let existing = if let Some(wid) = watchdog.watchdog_issue_id {
            self.issue_repo.get_by_id(wid).await?
        } else {
            self.watchdog_repo
                .find_watchdog_issue(watchdog.company_id, watchdog.issue_id)
                .await?
        };

        match existing {
            Some(mut wd_issue) => {
                // Ensure watchdog record references this issue
                if watchdog.watchdog_issue_id != Some(wd_issue.id) {
                    self.watchdog_repo.update_watchdog_issue_id(watchdog.id, wd_issue.id).await?;
                }

                let is_terminal = matches!(
                    wd_issue.status,
                    IssueStatus::Done | IssueStatus::Cancelled
                );
                let is_backlog = wd_issue.status == IssueStatus::Backlog;
                let needs_fresh_wake = wd_issue.status == IssueStatus::InReview;
                let should_reopen = is_terminal || is_backlog || needs_fresh_wake;

                if should_reopen {
                    let update = UpdateIssueInput {
                        status: Some(IssueStatus::Todo),
                        assignee_agent_id: Some(watchdog.watchdog_agent_id),
                        ..Default::default()
                    };
                    self.issue_repo.update(wd_issue.id, update).await?;
                    wd_issue.status = IssueStatus::Todo;
                }
                if wd_issue.origin_fingerprint.as_deref() != Some(stop_fingerprint.as_str()) {
                    let update = UpdateIssueInput {
                        description: Some(format!(
                            "Watchdog stopped fingerprint: {}",
                            stop_fingerprint
                        )),
                        ..Default::default()
                    };
                    self.issue_repo.update(wd_issue.id, update).await?;
                }
                Ok(())
            }
            None => {
                let create = CreateIssueInput {
                    company_id: watchdog.company_id,
                    project_id: source_issue.project_id,
                    goal_id: source_issue.goal_id,
                    title: format!(
                        "Watchdog review for {}",
                        source_issue
                            .identifier
                            .clone()
                            .unwrap_or_else(|| source_issue.title.clone())
                    ),
                    description: Some(format!(
                        "Task watchdog review issue.\n\nWatched issue: {}\nStopped fingerprint: {}\n\nThe watchdog agent should verify the stopped subtree and either confirm the disposition or restore a valid live path.",
                        source_issue.identifier.clone().unwrap_or_else(|| source_issue.id.to_string()),
                        stop_fingerprint
                    )),
                    status: Some(IssueStatus::Todo),
                    priority: Some(source_issue.priority),
                    parent_id: Some(source_issue.id),
                    assignee_agent_id: Some(watchdog.watchdog_agent_id),
                    origin_kind: Some(TASK_WATCHDOG_ORIGIN_KIND.to_string()),
                    origin_id: Some(watchdog.issue_id.to_string()),
                    ..Default::default()
                };
                let created = self.issue_repo.create(create).await?;
                // Link the watchdog record to the newly created review issue
                self.watchdog_repo.update_watchdog_issue_id(watchdog.id, created.id).await?;
                Ok(())
            }
        }
    }
}

#[async_trait]
impl WatchdogService for DefaultWatchdogService {
    async fn evaluate_all(&self, company_id: Uuid) -> RepositoryResult<usize> {
        let watchdogs = self.watchdog_repo.list_active_by_company(company_id).await?;
        let mut evaluated = 0;
        for wd in watchdogs {
            self.evaluate(wd).await?;
            evaluated += 1;
        }
        Ok(evaluated)
    }

    async fn evaluate(&self, watchdog: models::task_watchdog::IssueWatchdog) -> RepositoryResult<()> {
        let input = self.collect_classifier_input(watchdog.company_id, &watchdog).await?;
        let classification = classify_subtree(input);

        match &classification {
            ClassifierState::Stopped {
                stop_fingerprint, ..
            } => {
                self.watchdog_repo.record_observed(watchdog.id, stop_fingerprint).await?;
                self.handle_stopped(&watchdog, &classification).await?;
                self.watchdog_repo.record_reviewed(watchdog.id, stop_fingerprint).await?;
            }
            ClassifierState::Live { .. }
            | ClassifierState::PendingFirstRun { .. }
            | ClassifierState::AlreadyReviewed { .. }
            | ClassifierState::NotApplicable { .. } => {
                // No fingerprint review needed for these states.
            }
        }
        Ok(())
    }

    async fn upsert_watchdog(
        &self,
        company_id: Uuid,
        issue_id: Uuid,
        watchdog_agent_id: Uuid,
        instructions: Option<String>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<String>,
        created_by_run_id: Option<Uuid>,
    ) -> RepositoryResult<models::task_watchdog::IssueWatchdog> {
        self.watchdog_repo.upsert(
            company_id,
            issue_id,
            watchdog_agent_id,
            instructions.as_deref(),
            created_by_agent_id,
            created_by_user_id.as_deref(),
            created_by_run_id,
        ).await
    }

    async fn get_watchdog(&self, company_id: Uuid, issue_id: Uuid) -> RepositoryResult<Option<models::task_watchdog::IssueWatchdog>> {
        self.watchdog_repo.get_by_issue(company_id, issue_id).await
    }

    async fn update_watchdog_status(&self, id: Uuid, status: models::task_watchdog::IssueWatchdogStatus) -> RepositoryResult<models::task_watchdog::IssueWatchdog> {
        self.watchdog_repo.update_status(id, status).await
    }

    async fn evaluate_for_issue(&self, company_id: Uuid, issue_id: Uuid) -> RepositoryResult<usize> {
        // Find the watchdog for this specific issue
        if let Some(watchdog) = self.watchdog_repo.get_by_issue(company_id, issue_id).await? {
            if watchdog.status == models::task_watchdog::IssueWatchdogStatus::Active {
                self.evaluate(watchdog).await?;
                return Ok(1);
            }
        }

        // Also evaluate watchdogs for ancestor issues (walk up the parent chain)
        // by checking if any ancestor has an active watchdog
        let ancestors = self.issue_repo.list_ancestors(issue_id).await?;
        let mut evaluated = 0;
        for ancestor in ancestors {
            if let Some(watchdog) = self.watchdog_repo.get_by_issue(company_id, ancestor.id).await? {
                if watchdog.status == models::task_watchdog::IssueWatchdogStatus::Active {
                    self.evaluate(watchdog).await?;
                    evaluated += 1;
                }
            }
        }

        Ok(evaluated)
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn extract_context_issue_ids(runs: &[models::task_watchdog::HeartbeatRun]) -> Vec<Uuid> {
    runs.iter().filter_map(extract_context_issue_ids_single).collect()
}

fn extract_context_issue_ids_single(run: &models::task_watchdog::HeartbeatRun) -> Option<Uuid> {
    let snap = run.context_snapshot.as_ref()?;
    let issue_id = snap.get("issueId").and_then(|v| v.as_str());
    let task_id = snap.get("taskId").and_then(|v| v.as_str());
    issue_id
        .or(task_id)
        .and_then(|s| Uuid::parse_str(s).ok())
}

fn extract_payload_issue_ids(wakes: &[models::task_watchdog::AgentWakeupRequest]) -> Vec<Uuid> {
    let mut ids = Vec::new();
    for w in wakes {
        for key in ["issueId", "taskId"] {
            if let Some(s) = w.payload.get(key).and_then(|v| v.as_str()) {
                if let Ok(id) = Uuid::parse_str(s) {
                    ids.push(id);
                }
            }
        }
        if let Some(ctx) = w.payload.get("_paperclipWakeContext") {
            for key in ["issueId", "taskId"] {
                if let Some(s) = ctx.get(key).and_then(|v| v.as_str()) {
                    if let Ok(id) = Uuid::parse_str(s) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use models::task_watchdog::TaskWatchdogClassifierIssue;

    fn make_issue(id: Uuid, parent_id: Option<Uuid>, status: &str, origin_kind: Option<&str>) -> TaskWatchdogClassifierIssue {
        TaskWatchdogClassifierIssue {
            id,
            company_id: Uuid::nil(),
            identifier: None,
            title: format!("Issue {}", id),
            status: status.to_string(),
            parent_id,
            assignee_agent_id: None,
            assignee_user_id: None,
            origin_kind: origin_kind.map(|s| s.to_string()),
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        }
    }

    // ==================== classify_subtree ====================

    #[test]
    fn test_classify_not_applicable_missing_root() {
        let input = ClassifierInput {
            watchdog_issue_id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            last_reviewed_fingerprint: None,
            issues: vec![],
            live_run_issue_ids: vec![],
            live_wake_issue_ids: vec![],
            blockers: vec![],
            pending_interaction_issue_ids: vec![],
            pending_approval_issue_ids: vec![],
            completed_run_issue_ids: vec![],
            evaluated_at: Utc::now(),
        };

        let result = classify_subtree(input);
        assert!(matches!(result, ClassifierState::NotApplicable { .. }));
    }

    #[test]
    fn test_classify_not_applicable_watchdog_origin() {
        let root_id = Uuid::new_v4();
        let input = ClassifierInput {
            watchdog_issue_id: root_id,
            company_id: Uuid::nil(),
            last_reviewed_fingerprint: None,
            issues: vec![make_issue(root_id, None, "todo", Some("task_watchdog"))],
            live_run_issue_ids: vec![],
            live_wake_issue_ids: vec![],
            blockers: vec![],
            pending_interaction_issue_ids: vec![],
            pending_approval_issue_ids: vec![],
            completed_run_issue_ids: vec![],
            evaluated_at: Utc::now(),
        };

        let result = classify_subtree(input);
        assert!(matches!(result, ClassifierState::NotApplicable { .. }));
    }

    #[test]
    fn test_classify_live_with_running_issue() {
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let input = ClassifierInput {
            watchdog_issue_id: root_id,
            company_id: Uuid::nil(),
            last_reviewed_fingerprint: None,
            issues: vec![
                make_issue(root_id, None, "todo", None),
                make_issue(child_id, Some(root_id), "in_progress", None),
            ],
            live_run_issue_ids: vec![child_id],
            live_wake_issue_ids: vec![],
            blockers: vec![],
            pending_interaction_issue_ids: vec![],
            pending_approval_issue_ids: vec![],
            completed_run_issue_ids: vec![],
            evaluated_at: Utc::now(),
        };

        let result = classify_subtree(input);
        match result {
            ClassifierState::Live { live_issue_ids, .. } => {
                assert_eq!(live_issue_ids, vec![child_id]);
            }
            _ => panic!("Expected Live state, got {:?}", result),
        }
    }

    #[test]
    fn test_classify_stopped_no_live_path() {
        let root_id = Uuid::new_v4();
        let leaf_id = Uuid::new_v4();
        let input = ClassifierInput {
            watchdog_issue_id: root_id,
            company_id: Uuid::nil(),
            last_reviewed_fingerprint: None,
            issues: vec![
                make_issue(root_id, None, "todo", None),
                make_issue(leaf_id, Some(root_id), "done", None),
            ],
            live_run_issue_ids: vec![],
            live_wake_issue_ids: vec![],
            blockers: vec![],
            pending_interaction_issue_ids: vec![],
            pending_approval_issue_ids: vec![],
            completed_run_issue_ids: vec![leaf_id],
            evaluated_at: Utc::now(),
        };

        let result = classify_subtree(input);
        assert!(matches!(result, ClassifierState::Stopped { .. }));
    }

    #[test]
    fn test_classify_already_reviewed() {
        let root_id = Uuid::new_v4();
        let leaf_id = Uuid::new_v4();
        let input = ClassifierInput {
            watchdog_issue_id: root_id,
            company_id: Uuid::nil(),
            // Set a matching fingerprint to trigger AlreadyReviewed
            last_reviewed_fingerprint: Some(format!(
                "stop:{}:{}:{}",
                Uuid::nil(),
                root_id,
                format!("{}:done:::{}", leaf_id, format!("Issue {}", leaf_id))
            )),
            issues: vec![
                make_issue(root_id, None, "todo", None),
                make_issue(leaf_id, Some(root_id), "done", None),
            ],
            live_run_issue_ids: vec![],
            live_wake_issue_ids: vec![],
            blockers: vec![],
            pending_interaction_issue_ids: vec![],
            pending_approval_issue_ids: vec![],
            completed_run_issue_ids: vec![leaf_id],
            evaluated_at: Utc::now(),
        };

        let result = classify_subtree(input);
        assert!(matches!(result, ClassifierState::AlreadyReviewed { .. }));
    }

    // ==================== stable_stop_fingerprint ====================

    #[test]
    fn test_stable_stop_fingerprint_consistency() {
        let company_id = Uuid::new_v4();
        let watched_id = Uuid::new_v4();
        let leaf_id = Uuid::new_v4();

        let leaves = vec![StoppedLeaf {
            issue_id: leaf_id,
            identifier: None,
            title: "Test".to_string(),
            status: "done".to_string(),
            assignee_agent_id: None,
            assignee_user_id: None,
            blocker_issue_ids: vec![],
            pending_interaction_ids: vec![],
            pending_approval_ids: vec![],
            updated_at: None,
        }];

        let fp1 = stable_stop_fingerprint(company_id, watched_id, &leaves);
        let fp2 = stable_stop_fingerprint(company_id, watched_id, &leaves);
        assert_eq!(fp1, fp2, "Fingerprint must be deterministic");
    }

    #[test]
    fn test_stable_stop_fingerprint_changes_with_leaves() {
        let company_id = Uuid::new_v4();
        let watched_id = Uuid::new_v4();

        let leaves1 = vec![StoppedLeaf {
            issue_id: Uuid::new_v4(),
            identifier: None,
            title: "A".to_string(),
            status: "done".to_string(),
            assignee_agent_id: None,
            assignee_user_id: None,
            blocker_issue_ids: vec![],
            pending_interaction_ids: vec![],
            pending_approval_ids: vec![],
            updated_at: None,
        }];

        let leaves2 = vec![StoppedLeaf {
            issue_id: Uuid::new_v4(),
            identifier: None,
            title: "B".to_string(),
            status: "blocked".to_string(),
            assignee_agent_id: None,
            assignee_user_id: None,
            blocker_issue_ids: vec![Uuid::new_v4()],
            pending_interaction_ids: vec![],
            pending_approval_ids: vec![],
            updated_at: None,
        }];

        let fp1 = stable_stop_fingerprint(company_id, watched_id, &leaves1);
        let fp2 = stable_stop_fingerprint(company_id, watched_id, &leaves2);
        assert_ne!(fp1, fp2, "Different leaves must produce different fingerprints");
    }

    // ==================== extract_context_issue_ids ====================

    #[test]
    fn test_extract_context_issue_ids_from_snapshot() {
        let run = models::task_watchdog::HeartbeatRun {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            invocation_source: "test".to_string(),
            status: models::task_watchdog::HeartbeatRunStatus::Running,
            responsible_user_id: None,
            started_at: None,
            finished_at: None,
            error: None,
            exit_code: None,
            context_snapshot: Some(serde_json::json!({"issueId": "550e8400-e29b-41d4-a716-446655440000"})),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let ids = extract_context_issue_ids(&[run]);
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap());
    }

    #[test]
    fn test_extract_context_issue_ids_empty_when_no_snapshot() {
        let run = models::task_watchdog::HeartbeatRun {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            invocation_source: "test".to_string(),
            status: models::task_watchdog::HeartbeatRunStatus::Queued,
            responsible_user_id: None,
            started_at: None,
            finished_at: None,
            error: None,
            exit_code: None,
            context_snapshot: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let ids = extract_context_issue_ids(&[run]);
        assert!(ids.is_empty());
    }

    // ==================== extract_payload_issue_ids ====================

    #[test]
    fn test_extract_payload_issue_ids_from_wake() {
        let wake = models::task_watchdog::AgentWakeupRequest {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            status: models::task_watchdog::AgentWakeupRequestStatus::Queued,
            payload: serde_json::json!({"issueId": "550e8400-e29b-41d4-a716-446655440000"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let ids = extract_payload_issue_ids(&[wake]);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_extract_payload_issue_ids_from_wake_context() {
        let wake = models::task_watchdog::AgentWakeupRequest {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            status: models::task_watchdog::AgentWakeupRequestStatus::Queued,
            payload: serde_json::json!({
                "_paperclipWakeContext": {
                    "issueId": "550e8400-e29b-41d4-a716-446655440000"
                }
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let ids = extract_payload_issue_ids(&[wake]);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn test_extract_payload_issue_ids_empty() {
        let wake = models::task_watchdog::AgentWakeupRequest {
            id: Uuid::new_v4(),
            company_id: Uuid::nil(),
            agent_id: Uuid::nil(),
            status: models::task_watchdog::AgentWakeupRequestStatus::Queued,
            payload: serde_json::json!({"other": "data"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let ids = extract_payload_issue_ids(&[wake]);
        assert!(ids.is_empty());
    }
}
