use async_trait::async_trait;
use uuid::Uuid;

/// Attachment service trait
#[async_trait]
pub trait AttachmentService: Send + Sync {
    // Placeholder for attachment operations
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
}
