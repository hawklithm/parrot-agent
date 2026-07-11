use async_trait::async_trait;
use uuid::Uuid;
use crate::models::{WorkProduct, CreateWorkProductInput, UpdateWorkProductInput};

/// Work product service trait
#[async_trait]
pub trait WorkProductService: Send + Sync {
    /// List work products for an issue
    async fn list_work_products(&self, issue_id: Uuid, company_id: Uuid) -> Result<Vec<WorkProduct>, String>;
    
    /// Create work product
    async fn create_work_product(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateWorkProductInput,
    ) -> Result<WorkProduct, String>;
    
    /// Update work product
    async fn update_work_product(
        &self,
        id: Uuid,
        company_id: Uuid,
        input: UpdateWorkProductInput,
    ) -> Result<WorkProduct, String>;
    
    /// Delete work product
    async fn delete_work_prodself, id: Uuid, company_id: Uuid) -> Result<bool, String>;
}

/// Mock implementation of WorkProductService
pub struct MockWorkProductService;

impl MockWorkProductService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_work_product(id: Uuid, issue_id: Uuid, company_id: Uuid, name: String) -> WorkProduct {
        WorkProduct {
            id,
            issue_id,
            company_id,
            name,
            description: Some("Mock work product".to_string()),
            artifact: Some(serde_json::json!({"type": "code", "url": "https://example.com/artifact"})),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl WorkProductService for MockWorkProductService {
    async fn list_work_products(&self, issue_id: Uuid, company_id: Uuid) -> Result<Vec<WorkProduct>, String> {
        Ok(vec![
            Self::create_mock_work_product(Uuid::new_v4(), issue_id, company_id, "Design Doc".to_string()),
            Self::create_mock_work_product(Uuid::new_v4(), issue_id, company_id, "Implementation".to_string()),
        ])
    }
    
    async fn create_work_product(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateWorkProductInput,
    ) -> Result<WorkProduct, String> {
        let mut wp = Self::create_mock_work_product(Uuid::new_v4(), issue_id, company_id, input.name);
        wp.description = input.description;
        wp.artifact = input.artifact;
        Ok(wp)
    }
    
    async fn update_work_product(
        &self,
        id: Uuid,
        company_id: Uuid,
        input: UpdateWorkProductInput,
    ) -> Result<WorkProduct, String> {
        let mut wp = Self::create_mock_work_product(id, Uuid::new_v4(), company_id, input.name.unwrap_or_else(|| "Updated".to_string()));
        wp.description = input.description.or(wp.description);
        wp.artifact = input.artifact.or(wp.artifact);
        Ok(wp)
    }
    
    async fn delete_work_product(&self, _id: Uuid, _company_id: Uuid) -> Result<bool, String> {
        Ok(true)
    }
}
