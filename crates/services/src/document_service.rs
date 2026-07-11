use async_trait::async_trait;
use uuid::Uuid;
use models::{IssueDocument, CreateDocumentInput, LockDocumentInput};

/// Document parent type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentParentType {
    Issue,
    Case,
}

/// Document service trait for Issue and Case documents
#[async_trait]
pub trait DocumentService: Send + Sync {
    /// List documents by parent (issue or case)
    async fn list_documents(
        &self,
        parent_type: DocumentParentType,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueDocument>, String>;
    
    /// Get document by key
    async fn get_document(
        &self,
        parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        company_id: Uuid,
    ) -> Result<Option<IssueDocument>, String>;
    
    /// Upsert document (create or update)
    async fn upsert_document(
        &self,
        parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        input: CreateDocumentInput,
        company_id: Uuid,
    ) -> Result<IssueDocument, String>;
    
    /// Lock document for exclusive editing
    async fn lock_document(
        &self,
        parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        input: LockDocumentInput,
        company_id: Uuid,
    ) -> Result<IssueDocument, String>;
    
    /// Unlock document
    async fn unlock_document(
        &self,
        parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        company_id: Uuid,
        agent_id: Option<Uuid>,
        user_id: Option<Uuid>,
    ) -> Result<IssueDocument, String>;
}

/// Mock implementation of DocumentService
pub struct MockDocumentService;

impl MockDocumentService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_document(id: Uuid, parent_id: Uuid, company_id: Uuid, key: String) -> IssueDocument {
        IssueDocument {
            id,
            issue_id: parent_id,
            company_id,
            key,
            content: "# Mock Document\n\nSample content".to_string(),
            locked_by_agent_id: None,
            locked_by_user_id: None,
            locked_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl DocumentService for MockDocumentService {
    async fn list_documents(
        &self,
        _parent_type: DocumentParentType,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> Result<Vec<IssueDocument>, String> {
        Ok(vec![
            Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, "README.md".to_string()),
            Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, "notes.md".to_string()),
        ])
    }
    
    async fn get_document(
        &self,
        _parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        company_id: Uuid,
    ) -> Result<Option<IssueDocument>, String> {
        Ok(Some(Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, key.to_string())))
    }
    
    async fn upsert_document(
        &self,
        _parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        input: CreateDocumentInput,
        company_id: Uuid,
    ) -> Result<IssueDocument, String> {
        let mut doc = Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, key.to_string());
        doc.content = input.content;
        Ok(doc)
    }
    
    async fn lock_document(
        &self,
        _parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        input: LockDocumentInput,
        company_id: Uuid,
    ) -> Result<IssueDocument, String> {
        let mut doc = Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, key.to_string());
        doc.locked_by_agent_id = input.agent_id;
        doc.locked_by_user_id = input.user_id;
        doc.locked_at = Some(chrono::Utc::now());
        Ok(doc)
    }
    
    async fn unlock_document(
        &self,
        _parent_type: DocumentParentType,
        parent_id: Uuid,
        key: &str,
        company_id: Uuid,
        _agent_id: Option<Uuid>,
        _user_id: Option<Uuid>,
    ) -> Result<IssueDocument, String> {
        let mut doc = Self::create_mock_document(Uuid::new_v4(), parent_id, company_id, key.to_string());
        doc.locked_by_agent_id = None;
        doc.locked_by_user_id = None;
        doc.locked_at = None;
        Ok(doc)
    }
}
