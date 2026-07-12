use async_trait::async_trait;
use uuid::Uuid;
use chrono::Utc;
use models::{Issue, IssueStatus, IssuePriority, IssueWorkMode, CreateIssueInput, UpdateIssueInput};
use crate::issue_service::{IssueService, IssueMutationResult, CheckoutInput, ReleaseInput, ForceReleaseInput};
use crate::issue_repository::{IssueQueryFilter, Pagination};

/// Mock implementation of IssueService
pub struct MockIssueService;

impl MockIssueService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_issue(id: Uuid, company_id: Uuid, title: String) -> Issue {
        Issue {
            id,
            company_id,
            project_id: None,
            project_workspace_id: None,
            goal_id: None,
            parent_id: None,
            title,
            description: Some("Mock issue description".to_string()),
            status: IssueStatus::Todo,
            work_mode: IssueWorkMode::Standard,
            priority: IssuePriority::Medium,
            assignee_agent_id: None,
            assignee_user_id: None,
            assigned_to: None,
            checkout_run_id: None,
            execution_run_id: None,
            execution_agent_name_key: None,
            execution_locked_at: None,
            created_by_agent_id: None,
            created_by_user_id: None,
            responsible_user_id: None,
            issue_number: Some(1),
            identifier: Some("MOCK-1".to_string()),
            origin_kind: None,
            origin_id: None,
            origin_run_id: None,
            origin_fingerprint: None,
            request_depth: 0,
            billing_code: None,
            execution_policy: None,
            execution_state: None,
            monitor_next_check_at: None,
            monitor_last_triggered_at: None,
            monitor_attempt_count: None,
            monitor_notes: None,
            monitor_scheduled_by: None,
            execution_workspace_id: None,
            execution_workspace_preference: None,
            started_at: None,
            completed_at: None,
            cancelled_at: None,
            hidden_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[async_trait]
impl IssueService for MockIssueService {
    async fn create(&self, input: CreateIssueInput) -> Result<IssueMutationResult, String> {
        let issue = Self::create_mock_issue(Uuid::new_v4(), input.company_id, input.title);
        Ok(IssueMutationResult {
            changed: true,
            issue,
            change_kind: "created".to_string(),
        })
    }
    
    async fn create_child(&self, parent_id: Uuid, input: CreateIssueInput) -> Result<IssueMutationResult, String> {
        let mut issue = Self::create_mock_issue(Uuid::new_v4(), input.company_id, input.title);
        issue.parent_id = Some(parent_id);
        Ok(IssueMutationResult {
            changed: true,
            issue,
            change_kind: "created".to_string(),
        })
    }
    
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Issue>, String> {
        Ok(Some(Self::create_mock_issue(id, company_id, "Mock Issue".to_string())))
    }
    
    async fn list(
        &self,
        company_id: Uuid,
        _filter: &IssueQueryFilter,
        _pagination: &Pagination,
    ) -> Result<Vec<Issue>, String> {
        Ok(vec![
            Self::create_mock_issue(Uuid::new_v4(), company_id, "Issue 1".to_string()),
            Self::create_mock_issue(Uuid::new_v4(), company_id, "Issue 2".to_string()),
        ])
    }
    
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateIssueInput) -> Result<IssueMutationResult, String> {
        let mut issue = Self::create_mock_issue(id, company_id, input.title.unwrap_or_else(|| "Updated".to_string()));
        if let Some(status) = input.status {
            issue.status = status;
        }
        if let Some(priority) = input.priority {
            issue.priority = priority;
        }
        Ok(IssueMutationResult {
            changed: true,
            issue,
            change_kind: "updated".to_string(),
        })
    }
    
    async fn delete(&self, id: Uuid, company_id: Uuid) -> Result<IssueMutationResult, String> {
        let mut issue = Self::create_mock_issue(id, company_id, "Deleted Issue".to_string());
        issue.status = IssueStatus::Cancelled;
        issue.cancelled_at = Some(Utc::now());
        Ok(IssueMutationResult {
            changed: true,
            issue,
            change_kind: "deleted".to_string(),
        })
    }

    async fn checkout(&self, id: Uuid, company_id: Uuid, input: CheckoutInput) -> Result<Issue, String> {
        let mut issue = Self::create_mock_issue(id, company_id, "Checked Out Issue".to_string());
        issue.checkout_run_id = Some(input.checkout_run_id);
        issue.assignee_agent_id = input.agent_id;
        issue.assignee_user_id = input.user_id;
        issue.execution_locked_at = Some(Utc::now());
        Ok(issue)
    }

    async fn release(&self, id: Uuid, company_id: Uuid, input: ReleaseInput) -> Result<Issue, String> {
        let mut issue = Self::create_mock_issue(id, company_id, "Released Issue".to_string());
        issue.execution_run_id = Some(input.release_run_id);
        issue.execution_locked_at = None;
        if let Some(status_str) = input.target_status {
            issue.status = match status_str.as_str() {
                "done" => IssueStatus::Done,
                "in_review" => IssueStatus::InReview,
                _ => IssueStatus::Todo,
            };
        }
        Ok(issue)
    }

    async fn search(
        &self,
        company_id: Uuid,
        query: &str,
        _filter: &IssueQueryFilter,
        _pagination: &Pagination,
    ) -> Result<Vec<Issue>, String> {
        Ok(vec![
            Self::create_mock_issue(Uuid::new_v4(), company_id, format!("Search result: {}", query)),
        ])
    }

    async fn force_release(&self, id: Uuid, company_id: Uuid, _input: ForceReleaseInput) -> Result<Issue, String> {
        let mut issue = Self::create_mock_issue(id, company_id, "Force Released Issue".to_string());
        issue.status = IssueStatus::Todo;
        issue.execution_locked_at = None;
        Ok(issue)
    }

    async fn batch_update(
        &self,
        company_id: Uuid,
        issue_ids: Vec<Uuid>,
        status: Option<String>,
        _priority: Option<String>,
        _assignee_agent_id: Option<Uuid>,
        _assignee_user_id: Option<Uuid>,
    ) -> Result<Vec<Issue>, String> {
        let mut results = Vec::new();
        for id in issue_ids {
            let mut issue = Self::create_mock_issue(id, company_id, format!("Batch Issue {}", id));
            if let Some(ref s) = status {
                issue.status = match s.as_str() {
                    "in_progress" => IssueStatus::InProgress,
                    "done" => IssueStatus::Done,
                    "cancelled" => IssueStatus::Cancelled,
                    "todo" => IssueStatus::Todo,
                    "in_review" => IssueStatus::InReview,
                    "blocked" => IssueStatus::Blocked,
                    _ => IssueStatus::Todo,
                };
            }
            results.push(issue);
        }
        Ok(results)
    }

    async fn get_heartbeat_context(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "issueId": id.to_string(),
            "companyId": company_id.to_string(),
            "status": "todo",
            "activeRuns": [],
            "executionState": null,
        }))
    }
}
