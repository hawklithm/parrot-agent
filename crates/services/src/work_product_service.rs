use async_trait::async_trait;
use models::issue_auxiliary::{CreateWorkProductInput, UpdateWorkProductInput, WorkProduct};
use uuid::Uuid;

use crate::errors::ServiceResult;
use sqlx::PgPool;

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

/// PostgreSQL-backed work-product service.  Paperclip stores these records as
/// issue work products; keep the service scoped by company on every query.
pub struct PgWorkProductService {
    pool: PgPool,
}

impl PgWorkProductService {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl WorkProductService for PgWorkProductService {
    async fn list_work_products(&self, issue_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<WorkProduct>> {
        sqlx::query_as::<_, WorkProduct>(
            "SELECT id, issue_id, company_id, name, description, artifact, created_at, updated_at
             FROM issue_work_products WHERE issue_id = $1 AND company_id = $2 ORDER BY created_at ASC")
            .bind(issue_id).bind(company_id).fetch_all(&self.pool).await.map_err(Into::into)
    }

    async fn create_work_product(&self, issue_id: Uuid, company_id: Uuid, input: CreateWorkProductInput) -> ServiceResult<WorkProduct> {
        sqlx::query_as::<_, WorkProduct>(
            "INSERT INTO issue_work_products (issue_id, company_id, name, description, artifact)
             VALUES ($1, $2, $3, $4, COALESCE($5, '{}'::jsonb))
             RETURNING id, issue_id, company_id, name, description, artifact, created_at, updated_at")
            .bind(issue_id).bind(company_id).bind(input.name).bind(input.description).bind(input.artifact)
            .fetch_one(&self.pool).await.map_err(Into::into)
    }

    async fn update_work_product(&self, product_id: Uuid, company_id: Uuid, input: UpdateWorkProductInput) -> ServiceResult<WorkProduct> {
        sqlx::query_as::<_, WorkProduct>(
            "UPDATE issue_work_products SET name = COALESCE($3, name), description = COALESCE($4, description),
             artifact = COALESCE($5, artifact), updated_at = NOW()
             WHERE id = $1 AND company_id = $2
             RETURNING id, issue_id, company_id, name, description, artifact, created_at, updated_at")
            .bind(product_id).bind(company_id).bind(input.name).bind(input.description).bind(input.artifact)
            .fetch_optional(&self.pool).await.map_err(ServiceErrorFromSql::into_service)?
            .ok_or_else(|| crate::errors::ServiceError::NotFound("work product not found".into()))
    }

    async fn delete_work_product(&self, product_id: Uuid, company_id: Uuid) -> ServiceResult<()> {
        let result = sqlx::query("DELETE FROM issue_work_products WHERE id = $1 AND company_id = $2")
            .bind(product_id).bind(company_id).execute(&self.pool).await?;
        if result.rows_affected() == 0 { return Err(crate::errors::ServiceError::NotFound("work product not found".into())); }
        Ok(())
    }
}

struct ServiceErrorFromSql;
impl ServiceErrorFromSql {
    fn into_service(e: sqlx::Error) -> crate::errors::ServiceError { e.into() }
}

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
