use async_trait::async_trait;
use models::issue_auxiliary::{Attachment, UploadAttachmentInput};
use uuid::Uuid;

use crate::errors::ServiceResult;

/// Attachment service trait
#[async_trait]
pub trait AttachmentService: Send + Sync {
    /// List attachments for a parent (issue or case)
    async fn list_attachments(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<Attachment>>;

    /// Upload an attachment to a parent (issue or case)
    async fn upload_attachment(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
        input: UploadAttachmentInput,
    ) -> ServiceResult<Attachment>;

    /// Get the raw content of an attachment by id
    async fn get_attachment_content(
        &self,
        attachment_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<u8>>;

    /// Delete an attachment by id
    async fn delete_attachment(
        &self,
        attachment_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<()>;
}

/// Mock implementation
pub struct MockAttachmentService;

impl MockAttachmentService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AttachmentService for MockAttachmentService {
    async fn list_attachments(
        &self,
        _parent_type: &str,
        _parent_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<Vec<Attachment>> {
        Ok(Vec::new())
    }

    async fn upload_attachment(
        &self,
        _parent_type: &str,
        _parent_id: Uuid,
        _company_id: Uuid,
        _input: UploadAttachmentInput,
    ) -> ServiceResult<Attachment> {
        Err(crate::errors::ServiceError::NotImplemented(
            "AttachmentService::upload_attachment not implemented".to_string(),
        ))
    }

    async fn get_attachment_content(
        &self,
        _attachment_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<Vec<u8>> {
        Err(crate::errors::ServiceError::NotImplemented(
            "AttachmentService::get_attachment_content not implemented".to_string(),
        ))
    }

    async fn delete_attachment(
        &self,
        _attachment_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<()> {
        Err(crate::errors::ServiceError::NotImplemented(
            "AttachmentService::delete_attachment not implemented".to_string(),
        ))
    }
}
