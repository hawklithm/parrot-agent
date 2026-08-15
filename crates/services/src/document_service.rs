use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DocumentServiceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("document not found: {0}")]
    NotFound(Uuid),
    #[error("invalid document: {0}")]
    Invalid(String),
}

pub type DocumentResult<T> = Result<T, DocumentServiceError>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub content: String,
    pub document_type: String,
    pub version: i32,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateDocumentRequest {
    pub workspace_id: Uuid,
    pub title: String,
    pub content: String,
    pub document_type: String,
    pub created_by: Uuid,
}

#[derive(Debug, Clone)]
pub struct UpdateDocumentRequest {
    pub title: Option<String>,
    pub content: Option<String>,
}

#[async_trait]
pub trait DocumentService: Send + Sync {
    async fn create_document(&self, req: CreateDocumentRequest) -> DocumentResult<Document>;
    async fn get_document(&self, document_id: Uuid) -> DocumentResult<Option<Document>>;
    async fn update_document(
        &self,
        document_id: Uuid,
        req: UpdateDocumentRequest,
    ) -> DocumentResult<Document>;
    async fn delete_document(&self, document_id: Uuid) -> DocumentResult<()>;
    async fn list_documents(&self, workspace_id: Uuid) -> DocumentResult<Vec<Document>>;
    async fn search_documents(&self, workspace_id: Uuid, query: &str) -> DocumentResult<Vec<Document>>;
}

pub struct DocumentServiceImpl {
    pool: PgPool,
}

impl DocumentServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DocumentService for DocumentServiceImpl {
    async fn create_document(&self, req: CreateDocumentRequest) -> DocumentResult<Document> {
        let document_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        
        sqlx::query(
            r#"
            INSERT INTO documents (id, workspace_id, title, content, document_type, version, created_by, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#
        )
        .bind(document_id)
        .bind(req.workspace_id)
        .bind(&req.title)
        .bind(&req.content)
        .bind(&req.document_type)
        .bind(1)
        .bind(req.created_by)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        
        Ok(Document {
            id: document_id,
            workspace_id: req.workspace_id,
            title: req.title,
            content: req.content,
            document_type: req.document_type,
            version: 1,
            created_by: req.created_by,
            created_at: now,
            updated_at: now,
        })
    }
    
    async fn get_document(&self, document_id: Uuid) -> DocumentResult<Option<Document>> {
        let row = sqlx::query_as::<_, Document>(
            r#"
            SELECT id, workspace_id, title, content, document_type, version, created_by, created_at, updated_at
            FROM documents
            WHERE id = $1
            "#
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(row)
    }
    
    async fn update_document(
        &self,
        document_id: Uuid,
        req: UpdateDocumentRequest,
    ) -> DocumentResult<Document> {
        let now = chrono::Utc::now();
        
        if let Some(title) = req.title {
            sqlx::query("UPDATE documents SET title = $1, updated_at = $2, version = version + 1 WHERE id = $3")
                .bind(&title)
                .bind(now)
                .bind(document_id)
                .execute(&self.pool)
                .await?;
        }
        
        if let Some(content) = req.content {
            sqlx::query("UPDATE documents SET content = $1, updated_at = $2, version = version + 1 WHERE id = $3")
                .bind(&content)
                .bind(now)
                .bind(document_id)
                .execute(&self.pool)
                .await?;
        }
        
        self.get_document(document_id)
            .await?
            .ok_or_else(|| DocumentServiceError::NotFound(document_id))
    }
    
    async fn delete_document(&self, document_id: Uuid) -> DocumentResult<()> {
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(document_id)
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    async fn list_documents(&self, workspace_id: Uuid) -> DocumentResult<Vec<Document>> {
        let rows = sqlx::query_as::<_, Document>(
            r#"
            SELECT id, workspace_id, title, content, document_type, version, created_by, created_at, updated_at
            FROM documents
            WHERE workspace_id = $1
            ORDER BY updated_at DESC
            "#
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
    
    async fn search_documents(&self, workspace_id: Uuid, query: &str) -> DocumentResult<Vec<Document>> {
        let rows = sqlx::query_as::<_, Document>(
            r#"
            SELECT id, workspace_id, title, content, document_type, version, created_by, created_at, updated_at
            FROM documents
            WHERE workspace_id = $1 AND (title ILIKE $2 OR content ILIKE $2)
            ORDER BY updated_at DESC
            "#
        )
        .bind(workspace_id)
        .bind(format!("%{}%", query))
        .fetch_all(&self.pool)
        .await?;
        
        Ok(rows)
    }
}
