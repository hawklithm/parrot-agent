use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::models::{Case, CreateCaseInput, UpdateCaseInput};
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
    async fn list(&self, company_id: Uuid, filter: &CaseQueryFilter, pagination: &Pagination) -> Result<Vec<Case>, String>;
    async fn update(&self, id: Uuid, company_id: Uuid, input: UpdateCaseInput) -> Result<CaseMutationResult, String>;
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
            status: crate::models::CaseStatus::Draft,
            fields: serde_json::json!({}),
            parent_case_id: None,
            created_by_agent_id: None,
            created_by_user_id: None,
            completed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
}
