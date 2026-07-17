use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;
use models::{Case, CaseDetail, CaseEvent, CreateCaseInput, UpdateCaseInput};
use crate::issue_repository::Pagination;

/// Case query filter
#[derive(Debug, Clone, Default)]
pub struct CaseQueryFilter {
    pub status: Option<Vec<String>>,
    pub case_type: Option<String>,
    pub project_id: Option<Uuid>,
    pub parent_case_id: Option<Uuid>,
}

/// Case mutation result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMutationResult {
    pub changed: bool,
    pub case: Case,
    pub change_kind: String,
}

/// Case service trait for business logic
#[async_trait]
pub trait CaseService: Send + Sync {
    async fn create(&self, input: CreateCaseInput, upsert: bool) -> Result<CaseMutationResult, String>;
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Case>, String>;
    async fn get_detail(&self, id: Uuid, company_id: Uuid) -> Result<Option<CaseDetail>, String>;
    async fn list(&self, company_id: Uuid, filter: &CaseQueryFilter, pagination: &Pagination) -> Result<Vec<Case>, String>;
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String>;
    async fn list_events(&self, case_id: Uuid, company_id: Uuid, pagination: &Pagination) -> Result<Vec<CaseEvent>, String>;

    // --- P1: Case 子资源/状态机动作 (C1-C23) ---

    /// C1: Get child cases
    async fn get_children(&self, id: Uuid, company_id: Uuid) -> Result<Vec<Case>, String>;

    /// C2: Get child cases tree
    async fn get_children_tree(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C3: Get case rollup status
    async fn get_rollup(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C4: Get case context pack
    async fn get_context_pack(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C5: Get case outputs
    async fn get_outputs(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C6: Get case issue links
    async fn get_issue_links(&self, id: Uuid, company_id: Uuid) -> Result<Vec<serde_json::Value>, String>;

    /// C6: Create case issue link
    async fn create_issue_link(&self, id: Uuid, company_id: Uuid, issue_id: Uuid) -> Result<serde_json::Value, String>;

    /// C6: Delete case issue link
    async fn delete_issue_link(&self, id: Uuid, link_id: Uuid, company_id: Uuid) -> Result<(), String>;

    /// C7: Create generic link
    async fn create_link(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C8: Update blockers
    async fn update_blockers(&self, id: Uuid, company_id: Uuid, blockers: Vec<Uuid>) -> Result<serde_json::Value, String>;

    /// C9: Suggest transition
    async fn suggest_transition(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C10: Resolve suggestion
    async fn resolve_suggestion(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C11: Initiate review
    async fn review_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C12: Acknowledge drift
    async fn acknowledge_drift(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C13: Open conversation
    async fn open_conversation(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C14: Breakdown case
    async fn breakdown_case(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C20: Automation retry
    async fn automation_retry(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C21: Automation retry plan
    async fn automation_retry_plan(&self, id: Uuid, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String>;

    /// C22: Automation current stage rerun
    async fn automation_rerun_stage(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;

    /// C23: Automation single retry
    async fn automation_retry_single(&self, id: Uuid, company_id: Uuid, automation_id: Uuid) -> Result<serde_json::Value, String>;
}

/// Mock implementation of CaseService
pub struct MockCaseService;

impl MockCaseService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_case(id: Uuid, company_id: Uuid, title: String) -> Case {
        Case {
            id,
            company_id,
            project_id: None,
            case_number: 1,
            identifier: "CASE-1".to_string(),
            case_type: "feature".to_string(),
            key: Some("MOCK-KEY".to_string()),
            title,
            summary: Some("Mock case summary".to_string()),
            status: models::CaseStatus::Draft,
            fields: serde_json::json!({}),
            parent_case_id: None,
            created_by_agent_id: None,
            created_by_user_id: None,
            completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
    
    fn create_mock_case_detail(id: Uuid, company_id: Uuid, title: String) -> CaseDetail {
        CaseDetail {
            case: Self::create_mock_case(id, company_id, title),
            labels: vec!["feature".to_string(), "priority-high".to_string()],
            issue_links: vec![],
            documents: vec![],
            attachments: vec![],
            parent_case: None,
        }
    }
    
    fn create_mock_event(id: Uuid, case_id: Uuid, company_id: Uuid) -> CaseEvent {
        use models::{CaseEvent, CaseEventKind};
        CaseEvent {
            id,
            case_id,
            company_id,
            kind: CaseEventKind::Created,
            event_type: "created".to_string(),
            metadata: Some(serde_json::json!({"action": "created"})),
            actor_agent_id: None,
            actor_user_id: Some(Uuid::new_v4()),
            actor_type: None,
            actor_id: None,
            actor_run_id: None,
            payload: None,
            created_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl CaseService for MockCaseService {
    async fn create(&self, input: CreateCaseInput, _upsert: bool) -> Result<CaseMutationResult, String> {
        let case = Self::create_mock_case(Uuid::new_v4(), input.company_id, input.title);
        Ok(CaseMutationResult {
            changed: true,
            case,
            change_kind: "created".to_string(),
        })
    }
    
    async fn get(&self, id: Uuid, company_id: Uuid) -> Result<Option<Case>, String> {
        Ok(Some(Self::create_mock_case(id, company_id, "Mock Case".to_string())))
    }
    
    async fn get_detail(&self, id: Uuid, company_id: Uuid) -> Result<Option<CaseDetail>, String> {
        Ok(Some(Self::create_mock_case_detail(id, company_id, "Mock Case Detail".to_string())))
    }
    
    async fn list(&self, company_id: Uuid, _filter: &CaseQueryFilter, _pagination: &Pagination) -> Result<Vec<Case>, String> {
        Ok(vec![
            Self::create_mock_case(Uuid::new_v4(), company_id, "Case 1".to_string()),
            Self::create_mock_case(Uuid::new_v4(), company_id, "Case 2".to_string()),
        ])
    }
    
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String> {
        let mut case = Self::create_mock_case(id, company_id, input.title.unwrap_or_else(|| "Updated".to_string()));
        if let Some(status) = input.status {
            case.status = status;
        }
        Ok(CaseMutationResult {
            changed: true,
            case,
            change_kind: "updated".to_string(),
        })
    }
    
    async fn list_events(&self, case_id: Uuid, company_id: Uuid, _pagination: &Pagination) -> Result<Vec<CaseEvent>, String> {
        Ok(vec![
            Self::create_mock_event(Uuid::new_v4(), case_id, company_id),
        ])
    }

    // --- P1: Mock implementations for sub-resources ---

    async fn get_children(&self, id: Uuid, company_id: Uuid) -> Result<Vec<Case>, String> {
        Ok(vec![
            Self::create_mock_case(Uuid::new_v4(), company_id, format!("Child 1 of {}", id)),
            Self::create_mock_case(Uuid::new_v4(), company_id, format!("Child 2 of {}", id)),
        ])
    }

    async fn get_children_tree(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "children": [
                {"caseId": Uuid::new_v4(), "title": "Child 1", "children": []},
                {"caseId": Uuid::new_v4(), "title": "Child 2", "children": []}
            ]
        }))
    }

    async fn get_rollup(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "totalChildren": 2,
            "completed": 0,
            "inProgress": 1,
            "blocked": 0,
            "draft": 1,
        }))
    }

    async fn get_context_pack(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "title": "Mock Case",
            "summary": "Context pack for this case",
            "documents": [],
            "recentEvents": [],
            "relatedIssues": [],
        }))
    }

    async fn get_outputs(&self, id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({
            "caseId": id,
            "outputs": [
                {"key": "result", "value": "Completed successfully", "type": "text"},
                {"key": "artifacts", "value": [], "type": "list"},
            ]
        }))
    }

    async fn get_issue_links(&self, _id: Uuid, _company_id: Uuid) -> Result<Vec<serde_json::Value>, String> {
        Ok(vec![
            serde_json::json!({"id": Uuid::new_v4(), "issueId": Uuid::new_v4(), "relationship": "related"}),
        ])
    }

    async fn create_issue_link(&self, _id: Uuid, _company_id: Uuid, issue_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "issueId": issue_id, "relationship": "related", "created": true}))
    }

    async fn delete_issue_link(&self, _id: Uuid, _link_id: Uuid, _company_id: Uuid) -> Result<(), String> {
        Ok(())
    }

    async fn create_link(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"id": Uuid::new_v4(), "link": input, "created": true}))
    }

    async fn update_blockers(&self, _id: Uuid, _company_id: Uuid, blockers: Vec<Uuid>) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "blockers": blockers, "updated": true}))
    }

    async fn suggest_transition(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "suggestion": input, "suggested": true}))
    }

    async fn resolve_suggestion(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "resolution": input, "resolved": true}))
    }

    async fn review_case(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "review": input, "reviewInitiated": true}))
    }

    async fn acknowledge_drift(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "driftAcknowledged": true}))
    }

    async fn open_conversation(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "conversationId": Uuid::new_v4(), "opened": true}))
    }

    async fn breakdown_case(&self, _id: Uuid, _company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "breakdown": input, "children": [Uuid::new_v4(), Uuid::new_v4()]}))
    }

    async fn automation_retry(&self, _id: Uuid, _company_id: Uuid, _input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "retryInitiated": true, "automationRunId": Uuid::new_v4()}))
    }

    async fn automation_retry_plan(&self, _id: Uuid, _company_id: Uuid, _input: serde_json::Value) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "retryPlan": {"stages": ["stage1", "stage2"]}, "created": true}))
    }

    async fn automation_rerun_stage(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "stageRerunInitiated": true, "runId": Uuid::new_v4()}))
    }

    async fn automation_retry_single(&self, _id: Uuid, _company_id: Uuid, automation_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"caseId": _id, "automationId": automation_id, "retryInitiated": true}))
    }
}
