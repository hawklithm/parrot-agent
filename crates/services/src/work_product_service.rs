use async_trait::async_trait;
use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use uuid::Uuid;

use crate::errors::ServiceResult;

/// Work product service trait
#[async_trait]
pub trait WorkProductService: Send + Sync {
    /// List work products for an issue
    async fn list_work_products(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<WorkProduct>>;

    /// Create a work product for an issue
    async fn create_work_product(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: CreateWorkProductInput,
    ) -> ServiceResult<WorkProduct>;

    /// Update a work product by id
    async fn update_work_product(
        &self,
        product_id: Uuid,
        company_id: Uuid,
        input: UpdateWorkProductInput,
    ) -> ServiceResult<WorkProduct>;

    /// Delete a work product by id
    async fn delete_work_product(
        &self,
        product_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<()>;
}

/// Mock implementation
pub struct MockWorkProductService;

impl MockWorkProductService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WorkProductService for MockWorkProductService {
    async fn list_work_products(
        &self,
        _issue_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<Vec<WorkProduct>> {
        Ok(Vec::new())
    }

    async fn create_work_product(
        &self,
        _issue_id: Uuid,
        _company_id: Uuid,
        _input: CreateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::create_work_product not implemented".to_string(),
        ))
    }

    async fn update_work_product(
        &self,
        _product_id: Uuid,
        _company_id: Uuid,
        _input: UpdateWorkProductInput,
    ) -> ServiceResult<WorkProduct> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::update_work_product not implemented".to_string(),
        ))
    }

    async fn delete_work_product(
        &self,
        _product_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<()> {
        Err(crate::errors::ServiceError::NotImplemented(
            "WorkProductService::delete_work_product not implemented".to_string(),
        ))
    }
}
