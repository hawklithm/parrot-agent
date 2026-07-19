use async_trait::async_trait;
use models::{CreateLabelInput, Label};
use repositories::LabelRepository;
use std::sync::Arc;
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

#[async_trait]
pub trait LabelService: Send + Sync {
    /// List labels by company
    async fn list_by_company(&self, company_id: Uuid) -> ServiceResult<Vec<Label>>;

    /// Create a new label
    async fn create(&self, company_id: Uuid, name: String, color: Option<String>) -> ServiceResult<Label>;

    /// Delete a label
    async fn delete(&self, id: Uuid) -> ServiceResult<()>;
}

pub struct DefaultLabelService<R: LabelRepository> {
    repo: Arc<R>,
}

impl<R: LabelRepository> DefaultLabelService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl<R: LabelRepository + Send + Sync> LabelService for DefaultLabelService<R> {
    async fn list_by_company(&self, company_id: Uuid) -> ServiceResult<Vec<Label>> {
        let labels = self
            .repo
            .list_by_company(company_id)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(labels)
    }

    async fn create(&self, company_id: Uuid, name: String, color: Option<String>) -> ServiceResult<Label> {
        // Check if label already exists
        if let Some(_existing) = self
            .repo
            .get_by_company_and_name(company_id, &name)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?
        {
            return Err(ServiceError::Conflict(format!(
                "Label '{}' already exists in this company",
                name
            )));
        }

        let input = CreateLabelInput {
            company_id,
            name,
            color,
        };

        let label = self
            .repo
            .create(input)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(label)
    }

    async fn delete(&self, id: Uuid) -> ServiceResult<()> {
        // Check if label exists
        let _label = self
            .repo
            .get_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("Label not found: {}", id)))?;

        self.repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;
        Ok(())
    }
}
