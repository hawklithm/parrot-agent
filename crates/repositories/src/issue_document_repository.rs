use async_trait::async_trait;
use models::{Document, IssueDocument, UpsertDocumentInput, LockDocumentInput};
use uuid::Uuid;
use crate::RepositoryError;

/// Input for creating an Issue document link
#[derive(Debug, Clone)]
pub struct CreateIssueDocumentInput {
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub key: String,
    pub content: String,
    pub content_type: Option<String>,
}

#[async_trait]
pub trait IssueDocumentRepository: Send + Sync {
    /// List all documents for an issue
    async fn list_by_issue(&self, issue_id: Uuid) -> Result<Vec<(IssueDocument, Document)>, RepositoryError>;

    /// Get a document by issue_id and key
    async fn get_by_key(&self, issue_id: Uuid, key: &str) -> Result<Option<(IssueDocument, Document)>, RepositoryError>;

    /// Upsert (create or update) a document for an issue
    /// If a document with the same key exists, update its content
    /// Otherwise, create a new document and link
    async fn upsert(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        input: UpsertDocumentInput,
    ) -> Result<(IssueDocument, Document, bool), RepositoryError>; // Returns (link, document, created)

    /// Lock a document
    async fn lock_document(
        &self,
        document_id: Uuid,
        input: LockDocumentInput,
    ) -> Result<Document, RepositoryError>;

    /// Unlock a document
    async fn unlock_document(&self, document_id: Uuid) -> Result<Document, RepositoryError>;

    /// Delete a document link (and document if not referenced elsewhere)
    async fn delete(&self, issue_id: Uuid, key: &str) -> Result<(), RepositoryError>;
}
