use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::Issue;
use repositories::IssueRepository;

/// Input for promoting a low-trust issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteLowTrustInput {
    pub promoted_by_user_id: Uuid,
    pub source_trust: String,
    pub note: Option<String>,
}

/// Result of a low-trust promotion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteLowTrustResult {
    pub issue_id: Uuid,
    pub previous_source_trust: Option<String>,
    pub new_source_trust: String,
    pub promoted_by: Uuid,
    pub promoted_at: String,
}

/// Low trust review service for promoting low-trust outputs
#[async_trait]
pub trait LowTrustService: Send + Sync {
    /// Promote a low-trust issue to a higher trust level
    async fn promote_low_trust(&self, company_id: Uuid, issue_id: Uuid, input: PromoteLowTrustInput) -> Result<PromoteLowTrustResult, String>;

    /// Get issues with low trust for review
    async fn list_low_trust_issues(&self, company_id: Uuid, limit: i64) -> Result<Vec<Issue>, String>;
}

/// Default implementation of LowTrustService
pub struct DefaultLowTrustService {
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultLowTrustService {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }
}

#[async_trait]
impl LowTrustService for DefaultLowTrustService {
    async fn promote_low_trust(&self, _company_id: Uuid, issue_id: Uuid, input: PromoteLowTrustInput) -> Result<PromoteLowTrustResult, String> {
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| format!("Failed to get issue: {}", e))?
            .ok_or_else(|| format!("Issue {} not found", issue_id))?;

        let previous_source_trust = None; // source_trust not stored on Issue struct directly

        // Update the issue's source_trust via the update input
        let update = models::UpdateIssueInput {
            source_trust: Some(input.source_trust.clone()),
            ..Default::default()
        };

        self.issue_repo
            .update(issue_id, update)
            .await
            .map_err(|e| format!("Failed to promote low trust issue: {}", e))?;

        Ok(PromoteLowTrustResult {
            issue_id,
            previous_source_trust,
            new_source_trust: input.source_trust,
            promoted_by: input.promoted_by_user_id,
            promoted_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn list_low_trust_issues(&self, company_id: Uuid, _limit: i64) -> Result<Vec<Issue>, String> {
        // Query issues where source_trust indicates low trust
        let filter = models::IssueQueryFilter {
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
            project_id: None,
            goal_id: None,
            parent_id: None,
            work_mode: None,
        };

        let pagination = models::Pagination {
            limit: 100,
            offset: 0,
            cursor: None,
        };

        let all_issues = self.issue_repo
            .list_by_company(company_id, &filter, &pagination)
            .await
            .map_err(|e| format!("Failed to list issues: {}", e))?;

        // Filter to low-trust issues
        // source_trust is stored in UpdateIssueInput, not directly on Issue
        // In production: query a dedicated trust tracking table or use execution_state
        // For now: return issues with specific trust-related metadata
        Ok(all_issues
            .into_iter()
            .filter(|issue| {
                // Low-trust indicators: blocked issues or issues created by untrusted agents
                issue.status == models::IssueStatus::Blocked
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repositories::RepositoryError;

    struct MockIssueRepo;

    impl MockIssueRepo {
        fn new() -> Self { Self }
    }

    #[async_trait]
    impl IssueRepository for MockIssueRepo {
        async fn get_by_id(&self, _id: Uuid) -> Result<Option<Issue>, RepositoryError> { Ok(None) }
        async fn list_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn count_by_company(&self, _company_id: Uuid, _filter: &models::IssueQueryFilter) -> Result<i64, RepositoryError> { Ok(0) }
        async fn create(&self, _input: models::CreateIssueInput) -> Result<Issue, RepositoryError> { unimplemented!() }
        async fn update(&self, _id: Uuid, _input: models::UpdateIssueInput) -> Result<Issue, RepositoryError> { unimplemented!() }
        async fn delete(&self, _id: Uuid) -> Result<(), RepositoryError> { Ok(()) }
        async fn search(&self, _company_id: Uuid, _query: &str, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn get_by_identifier(&self, _identifier: &str) -> Result<Option<Issue>, RepositoryError> { Ok(None) }
        async fn list_by_parent(&self, _parent_id: Uuid, _pagination: &models::Pagination) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
        async fn get_by_ids(&self, _ids: Vec<Uuid>) -> Result<Vec<Issue>, RepositoryError> { Ok(vec![]) }
    }

    #[tokio::test]
    async fn test_promote_issue_not_found() {
        let service = DefaultLowTrustService::new(Arc::new(MockIssueRepo::new()));
        let input = PromoteLowTrustInput {
            promoted_by_user_id: Uuid::new_v4(),
            source_trust: "high".to_string(),
            note: None,
        };
        let result = service.promote_low_trust(Uuid::nil(), Uuid::new_v4(), input).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
