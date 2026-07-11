use async_trait::async_trait;
use uuid::Uuid;
use crate::models::{Attachment, UploadAttachmentInput};

/// Attachment service trait
#[async_trait]
pub trait AttachmentService: Send + Sync {
    /// List attachments for a parent (issue or case)
    async fn list_attachments(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<Attachment>, String>;
    
    /// Upload attachment
    async fn upload_attachment(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
        input: UploadAttachmentInput,
    ) -> Result<Attachment, String>;
    
    /// Get attachment content
    async fn get_attachment_content(
        &self,
        id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<u8>, String>;
    
    /// Delete attachment
    async fn delete_attachment(&self, id: Uuid, company_id: Uuid) -> Result<bool, String>;
}

/// Mock implementation of AttachmentService
pub struct MockAttachmentService;

impl MockAttachmentService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_attachment(id: Uuid, parent_id: Uuid, company_id: Uuid, filename: String) -> Attachment {
        Attachment {
            id,
            parent_type: "issue".to_string(),
            parent_id,
            company_id,
            asset_id: Some(Uuid::new_v4()),
            filename,
            content_type: "application/pdf".to_string(),
            size: 1024,
            created_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl AttachmentService for MockAttachmentService {
    async fn list_attachments(
        &self,
        _parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<Attachment>, String> {
        Ok(vec![
            Self::create_mock_attachment(Uuid::new_v4(), parent_id, company_id, "document.pdf".to_string()),
            Self::create_mock_attachment(Uuid::new_v4(), parent_id, company_id, "image.png".to_string()),
        ])
    }
    
    async fn upload_attachment(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
        input: UploadAttachmentInput,
    ) -> Result<Attachment, String> {
        Ok(Attachment {
            id: Uuid::new_v4(),
            parent_type: parent_type.to_string(),
            parent_id,
            company_id,
            asset_id: Some(Uuid::new_v4()),
            filename: input.filename,
            content_type: input.content_type,
            size: input.size,
            created_at: chrono::Utc::now(),
        })
    }
    
    async fn get_attachment_content(
        &self,
        _id: Uuid,
        _company_id: Uuid,
    ) -> Result<Vec<u8>, String> {
        Ok(b"Mock attachment content".to_vec())
    }
    
    async fn delete_attachment(&self, _id: Uuid, _company_id: Uuid) -> Result<bool, String> {
        Ok(true)
    }
}
