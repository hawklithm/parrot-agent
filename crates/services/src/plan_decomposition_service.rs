use async_trait::async_trait;
use uuid::Uuid;
use std::sync::Arc;

use models::{
    PlanDecomposition, CreatePlanDecompositionInput, AcceptPlanDecompositionInput,
};
use repositories::{PlanDecompositionRepository, IssueRepository};

/// Plan decomposition service for managing issue plan decompositions
#[async_trait]
pub trait PlanDecompositionService: Send + Sync {
    /// Create a plan decomposition for an issue
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreatePlanDecompositionInput) -> Result<PlanDecomposition, String>;

    /// List plan decompositions for an issue
    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<PlanDecomposition>, String>;

    /// Accept a plan decomposition (approve the plan)
    async fn accept(&self, id: Uuid, input: &AcceptPlanDecompositionInput) -> Result<PlanDecomposition, String>;

    /// Get a specific plan decomposition
    async fn get_by_id(&self, id: Uuid) -> Result<Option<PlanDecomposition>, String>;

    /// Delete a plan decomposition
    async fn delete(&self, id: Uuid) -> Result<(), String>;
}

/// Default implementation of PlanDecompositionService
pub struct DefaultPlanDecompositionService {
    plan_repo: Arc<dyn PlanDecompositionRepository>,
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultPlanDecompositionService {
    pub fn new(
        plan_repo: Arc<dyn PlanDecompositionRepository>,
        issue_repo: Arc<dyn IssueRepository>,
    ) -> Self {
        Self {
            plan_repo,
            issue_repo,
        }
    }
}

#[async_trait]
impl PlanDecompositionService for DefaultPlanDecompositionService {
    async fn create(&self, company_id: Uuid, issue_id: Uuid, input: &CreatePlanDecompositionInput) -> Result<PlanDecomposition, String> {
        // Verify issue exists
        let _issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| format!("Failed to verify issue: {}", e))?
            .ok_or_else(|| format!("Issue {} not found", issue_id))?;

        self.plan_repo
            .create(company_id, issue_id, input)
            .await
            .map_err(|e| format!("Failed to create plan decomposition: {}", e))
    }

    async fn list_by_issue(&self, company_id: Uuid, issue_id: Uuid) -> Result<Vec<PlanDecomposition>, String> {
        self.plan_repo
            .list_by_issue(company_id, issue_id)
            .await
            .map_err(|e| format!("Failed to list plan decompositions: {}", e))
    }

    async fn accept(&self, id: Uuid, input: &AcceptPlanDecompositionInput) -> Result<PlanDecomposition, String> {
        self.plan_repo
            .accept(id, input)
            .await
            .map_err(|e| format!("Failed to accept plan decomposition: {}", e))
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<PlanDecomposition>, String> {
        self.plan_repo
            .get_by_id(id)
            .await
            .map_err(|e| format!("Failed to get plan decomposition: {}", e))
    }

    async fn delete(&self, id: Uuid) -> Result<(), String> {
        self.plan_repo
            .delete(id)
            .await
            .map_err(|e| format!("Failed to delete plan decomposition: {}", e))
    }
}
