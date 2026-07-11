use async_trait::async_trait;
use models::{Document, IssueDocument, UpsertDocumentInput, LockDocumentInput};
use uuid::Uuid;
use std::sync::Arc;
use repositories::{IssueDocumentRepository, IssueRepository, RepositoryError};

/// Service-level errors for Document operations
#[derive(Debug, thiserror::Error)]
pub enum DocumentServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Document not found: {0}")]
    NotFound(String),

    #[error("Issue not found: {0}")]
    IssueNotFound(Uuid),

    #[error("Document is locked by {locked_by_type} {locked_by_id}")]
    DocumentLocked {
        locked_by_type: String,
        locked_by_id: Uuid,
    },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

pub type DocumentServiceResult<T> = Result<T, DocumentServiceError>;

/// Issue Document Service trait
#[async_trait]
pub trait IssueDocumentService: Send + Sync {
    /// Create or update a document for an issue
    async fn upsert_document(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: UpsertDocumentInput,
    ) -> DocumentServiceResult<(IssueDocument, Document, bool)>;

    /// Get a document by issue_id and key
    async fn get_document(
        &self,
        issue_id: Uuid,
        key: &str,
    ) -> DocumentServiceResult<(IssueDocument, Document)>;

    /// List all documents for an issue
    async fn list_documents(
        &self,
        issue_id: Uuid,
    ) -> DocumentServiceResult<Vec<(IssueDocument, Document)>>;

    /// Delete a document
    async fn delete_document(
        &self,
        issue_id: Uuid,
        key: &str,
    ) -> DocumentServiceResult<()>;

    /// Lock a document for editing
    async fn lock_document(
        &self,
        issue_id: Uuid,
        key: &str,
        locked_by_type: String,
        locked_by_id: Uuid,
        run_id: Option<Uuid>,
    ) -> DocumentServiceResult<Document>;

    /// Unlock a document
    async fn unlock_document(
        &self,
        issue_id: Uuid,
        key: &str,
        actor_id: Uuid,
    ) -> DocumentServiceResult<Document>;
}

/// Issue Document Service implementation
pub struct IssueDocumentServiceImpl<DR, IR>
where
    DR: IssueDocumentRepository,
    IR: IssueRepository,
{
    document_repository: Arc<DR>,
    issue_repository: Arc<IR>,
}

impl<DR, IR> IssueDocumentServiceImpl<DR, IR>
where
    DR: IssueDocumentRepository,
    IR: IssueRepository,
{
    pub fn new(document_repository: Arc<DR>, issue_repository: Arc<IR>) -> Self {
        Self {
            document_repository,
            issue_repository,
        }
    }

    /// Verify that an issue exists
    async fn verify_issue_exists(&self, issue_id: Uuid) -> DocumentServiceResult<()> {
        let issue = self.issue_repository.get_by_id(issue_id).await?;
        if issue.is_none() {
            return Err(DocumentServiceError::IssueNotFound(issue_id));
        }
        Ok(())
    }

    /// Check if a document is locked and verify permission
    fn check_lock_permission(
        &self,
        document: &Document,
        actor_id: Uuid,
    ) -> DocumentServiceResult<()> {
        if let Some(locked_by_id) = document.locked_by_id {
            if locked_by_id != actor_id {
                return Err(DocumentServiceError::DocumentLocked {
                    locked_by_type: document.locked_by_type.clone().unwrap_or_default(),
                    locked_by_id,
                });
            }
        }
        Ok(())
    }
}

#[async_trait]
impl<DR, IR> IssueDocumentService for IssueDocumentServiceImpl<DR, IR>
where
    DR: IssueDocumentRepository,
    IR: IssueRepository,
{
    async fn upsert_document(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: UpsertDocumentInput,
    ) -> DocumentServiceResult<(IssueDocument, Document, bool)> {
        // Verify issue exists
        self.verify_issue_exists(issue_id).await?;

        // Validate input
        if input.key.is_empty() {
            return Err(DocumentServiceError::Validation(
                "Document key cannot be empty".to_string(),
            ));
        }

        if input.content.is_empty() {
            return Err(DocumentServiceError::Validation(
                "Document content cannot be empty".to_string(),
            ));
        }

        // Check if document exists and is locked
        if let Some((_, existing_doc)) = self.document_repository.get_by_key(issue_id, &input.key).await? {
            if existing_doc.is_locked() {
                return Err(DocumentServiceError::DocumentLocked {
                    locked_by_type: existing_doc.locked_by_type.unwrap_or_default(),
                    locked_by_id: existing_doc.locked_by_id.unwrap(),
                });
            }
        }

        // Upsert document
        let result = self.document_repository.upsert(issue_id, company_id, input).await?;

        Ok(result)
    }

    async fn get_document(
        &self,
        issue_id: Uuid,
        key: &str,
    ) -> DocumentServiceResult<(IssueDocument, Document)> {
        let result = self.document_repository.get_by_key(issue_id, key).await?;

        match result {
            Some(doc) => Ok(doc),
            None => Err(DocumentServiceError::NotFound(format!(
                "Document '{}' not found for issue {}",
                key, issue_id
            ))),
        }
    }

    async fn list_documents(
        &self,
        issue_id: Uuid,
    ) -> DocumentServiceResult<Vec<(IssueDocument, Document)>> {
        // Verify issue exists
        self.verify_issue_exists(issue_id).await?;

        let documents = self.document_repository.list_by_issue(issue_id).await?;

        Ok(documents)
    }

    async fn delete_document(
        &self,
        issue_id: Uuid,
        key: &str,
    ) -> DocumentServiceResult<()> {
        // Check if document exists
        let doc_result = self.document_repository.get_by_key(issue_id, key).await?;

        if doc_result.is_none() {
            return Err(DocumentServiceError::NotFound(format!(
                "Document '{}' not found for issue {}",
                key, issue_id
            )));
        }

        let (_, document) = doc_result.unwrap();

        // Check if document is locked
        if document.is_locked() {
            return Err(DocumentServiceError::DocumentLocked {
                locked_by_type: document.locked_by_type.unwrap_or_default(),
                locked_by_id: document.locked_by_id.unwrap(),
            });
        }

        // Delete document
        self.document_repository.delete(issue_id, key).await?;

        Ok(())
    }

    async fn lock_document(
        &self,
        issue_id: Uuid,
        key: &str,
        locked_by_type: String,
        locked_by_id: Uuid,
        run_id: Option<Uuid>,
    ) -> DocumentServiceResult<Document> {
        // Get document
        let (_, document) = self.get_document(issue_id, key).await?;

        // Check if already locked
        if document.is_locked() {
            return Err(DocumentServiceError::DocumentLocked {
                locked_by_type: document.locked_by_type.unwrap_or_default(),
                locked_by_id: document.locked_by_id.unwrap(),
            });
        }

        // Lock document
        let input = LockDocumentInput {
            locked_by_type,
            locked_by_id,
            run_id,
        };

        let locked_doc = self.document_repository.lock_document(document.id, input).await?;

        Ok(locked_doc)
    }

    async fn unlock_document(
        &self,
        issue_id: Uuid,
        key: &str,
        actor_id: Uuid,
    ) -> DocumentServiceResult<Document> {
        // Get document
        let (_, document) = self.get_document(issue_id, key).await?;

        // Check if locked and verify permission
        self.check_lock_permission(&document, actor_id)?;

        // Unlock document
        let unlocked_doc = self.document_repository.unlock_document(document.id).await?;

        Ok(unlocked_doc)
    }
}
