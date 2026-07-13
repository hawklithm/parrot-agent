use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::{Issue, IssueStatus};
use repositories::IssueRepository;

/// Blocker diagnostics for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockersDiagnostics {
    /// List of issues that are blocking this issue
    pub blockers: Vec<Issue>,
    /// Whether the issue itself is blocked
    pub is_blocked: bool,
    /// The blocker chain (ancestors that are blocked)
    pub blocker_chain: Vec<Issue>,
}

/// Wakes diagnostics for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakesDiagnostics {
    /// Number of wake requests pending
    pub pending_wakes: i64,
    /// Last wake time
    pub last_wake_at: Option<String>,
    /// Active wake requests
    pub active_wakes: Vec<WakeRequestInfo>,
}

/// Wake request info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeRequestInfo {
    pub wake_id: Uuid,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub created_at: String,
}

/// Subtree diagnostics for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtreeDiagnostics {
    pub issue_id: Uuid,
    pub total_descendants: i64,
    pub status_breakdown: Vec<StatusCount>,
    pub max_depth: i64,
    pub has_loops: bool,
}

/// Status count breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

/// Issue diagnostics service for analyzing issue health
#[async_trait]
pub trait IssueDiagnosticsService: Send + Sync {
    /// Get blockers diagnostics for an issue
    async fn get_blockers_diagnostics(&self, company_id: Uuid, issue_id: Uuid) -> Result<BlockersDiagnostics, String>;

    /// Get wakes diagnostics for an issue
    async fn get_wakes_diagnostics(&self, company_id: Uuid, issue_id: Uuid) -> Result<WakesDiagnostics, String>;

    /// Get subtree diagnostics for an issue
    async fn get_subtree_diagnostics(&self, company_id: Uuid, issue_id: Uuid) -> Result<SubtreeDiagnostics, String>;
}

/// Default implementation of IssueDiagnosticsService
pub struct DefaultIssueDiagnosticsService {
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultIssueDiagnosticsService {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }
}

#[async_trait]
impl IssueDiagnosticsService for DefaultIssueDiagnosticsService {
    async fn get_blockers_diagnostics(&self, _company_id: Uuid, issue_id: Uuid) -> Result<BlockersDiagnostics, String> {
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| format!("Failed to get issue: {}", e))?
            .ok_or_else(|| format!("Issue {} not found", issue_id))?;

        let is_blocked = issue.status == IssueStatus::Blocked;

        // Find issues that are blocking this one
        // In practice: query the blockers via issue_tree_holds or linked dependencies
        // For now: find parent chain that might be blocked
        let mut blocker_chain = Vec::new();
        let mut current_id = issue.parent_id;

        while let Some(parent_id) = current_id {
            if let Ok(Some(parent)) = self.issue_repo.get_by_id(parent_id).await {
                if parent.status == IssueStatus::Blocked {
                    blocker_chain.push(parent.clone());
                }
                current_id = parent.parent_id;
            } else {
                break;
            }
        }

        // Find direct blockers (children with blocked status that might be blocking)
        let blockers = self.issue_repo
            .list_children(issue_id)
            .await
            .map_err(|e| format!("Failed to list children: {}", e))?
            .into_iter()
            .filter(|child| child.status == IssueStatus::Blocked)
            .collect();

        Ok(BlockersDiagnostics {
            blockers,
            is_blocked,
            blocker_chain,
        })
    }

    async fn get_wakes_diagnostics(&self, _company_id: Uuid, _issue_id: Uuid) -> Result<WakesDiagnostics, String> {
        // Wake diagnostics would query the heartbeat/wake system
        // For now, return a placeholder
        Ok(WakesDiagnostics {
            pending_wakes: 0,
            last_wake_at: None,
            active_wakes: vec![],
        })
    }

    async fn get_subtree_diagnostics(&self, _company_id: Uuid, issue_id: Uuid) -> Result<SubtreeDiagnostics, String> {
        let children = self.issue_repo
            .list_children(issue_id)
            .await
            .map_err(|e| format!("Failed to list children: {}", e))?;

        let total_descendants = children.len() as i64;

        // Status breakdown
        let mut status_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for child in &children {
            let status_str = format!("{}", child.status);
            *status_counts.entry(status_str).or_insert(0) += 1;
        }

        let status_breakdown: Vec<StatusCount> = status_counts
            .into_iter()
            .map(|(status, count)| StatusCount { status, count })
            .collect();

        // Calculate max depth by traversing
        let max_depth = self.calculate_max_depth(issue_id, 0).await;

        Ok(SubtreeDiagnostics {
            issue_id,
            total_descendants,
            status_breakdown,
            max_depth,
            has_loops: false,
        })
    }
}

impl DefaultIssueDiagnosticsService {
    /// Calculate max depth of the issue tree (non-recursive to avoid async recursion issues)
    async fn calculate_max_depth(&self, issue_id: Uuid, _current_depth: i64) -> i64 {
        // Use iterative BFS to calculate max depth
        use std::collections::VecDeque;

        let mut queue = VecDeque::new();
        queue.push_back((issue_id, 0i64));
        let mut max_depth = 0i64;

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth > max_depth {
                max_depth = depth;
            }

            if let Ok(children) = self.issue_repo.list_children(current_id).await {
                for child in &children {
                    queue.push_back((child.id, depth + 1));
                }
            }
        }

        max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use repositories::RepositoryError;

    struct MockIssueRepo;

    impl MockIssueRepo {
        fn new() -> Self { Self }
    }

    #[async_trait]
    impl IssueRepository for MockIssueRepo {
        async fn get_by_id(&self, _id: Uuid) -> Result<Option<Issue>, RepositoryError> {
            Ok(None)
        }
        async fn list_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
        async fn count_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter) -> Result<i64, RepositoryError> {
            Ok(0)
        }
        async fn create(&self, _input: models::CreateIssueInput) -> Result<Issue, RepositoryError> {
            unimplemented!()
        }
        async fn update(&self, _id: Uuid, _input: models::UpdateIssueInput) -> Result<Issue, RepositoryError> {
            unimplemented!()
        }
        async fn delete(&self, _id: Uuid) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn search(&self, _company_id: Uuid, _query: &str, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
        async fn get_by_identifier(&self, _identifier: &str) -> Result<Option<Issue>, RepositoryError> {
            Ok(None)
        }
        async fn list_by_parent(&self, _parent_id: Uuid, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
        async fn get_by_ids(&self, _ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
        async fn list_ancestors(&self, _issue_id: Uuid) -> Result<Vec<Issue>, RepositoryError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_blockers_diagnostics_empty() {
        let service = DefaultIssueDiagnosticsService::new(Arc::new(MockIssueRepo::new()));
        let result = service.get_blockers_diagnostics(Uuid::nil(), Uuid::new_v4()).await;
        assert!(result.is_err()); // Issue not found
    }

    #[tokio::test]
    async fn test_subtree_diagnostics_empty() {
        let service = DefaultIssueDiagnosticsService::new(Arc::new(MockIssueRepo::new()));
        let result = service.get_subtree_diagnostics(Uuid::nil(), Uuid::new_v4()).await;
        assert!(result.is_ok());
        let diagnostics = result.unwrap();
        assert_eq!(diagnostics.total_descendants, 0);
        assert_eq!(diagnostics.max_depth, 0);
    }
}
