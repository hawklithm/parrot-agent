use async_trait::async_trait;
use uuid::Uuid;

/// Work product service trait
#[async_trait]
pub trait WorkProductService: Send + Sync {
    // Placeholder for work product operations
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
}
