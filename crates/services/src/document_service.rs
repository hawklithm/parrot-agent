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
    #[error("version not found: ({0}, {1})")]
    VersionNotFound(Uuid, i32),
    #[error("invalid document: {0}")]
    Invalid(String),
}

pub type DocumentResult<T> = Result<T, DocumentServiceError>;

/// Matches `documents` table after migrations/12_create_documents_and_versions.sql
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub company_id: Uuid,
    pub title: String,
    pub content: String,
    pub content_type: String,
    pub category: Option<String>,
    pub tags: serde_json::Value,
    pub created_by_agent_id: Option<Uuid>,
    pub updated_by_agent_id: Option<Uuid>,
    pub version: i32,
    pub status: String,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Matches `document_versions` table after migration 12
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentVersion {
    pub id: Uuid,
    pub document_id: Uuid,
    pub version: i32,
    pub title: String,
    pub content: String,
    pub updated_by_agent_id: Option<Uuid>,
    pub change_summary: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateDocumentRequest {
    pub company_id: Uuid,
    pub title: String,
    pub content: String,
    pub content_type: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub created_by_agent_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct UpdateDocumentRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub content_type: Option<String>,
    pub tags: Option<Vec<String>>,
    pub updated_by_agent_id: Option<Uuid>,
    pub change_summary: Option<String>,
}

#[async_trait]
pub trait DocumentService: Send + Sync {
    async fn create_document(&self, req: CreateDocumentRequest) -> DocumentResult<Document>;
    async fn get_document(&self, document_id: Uuid) -> DocumentResult<Option<Document>>;
    async fn update_document(&self, document_id: Uuid, req: UpdateDocumentRequest) -> DocumentResult<Document>;
    async fn delete_document(&self, document_id: Uuid) -> DocumentResult<()>;
    /// Snapshot current content as a version. Call before content update.
    async fn create_version(&self, document_id: Uuid, change_summary: Option<String>) -> DocumentResult<DocumentVersion>;
    async fn list_versions(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentVersion>>;
    async fn get_version(&self, document_id: Uuid, version: i32) -> DocumentResult<DocumentVersion>;
    /// Restore document content to a previous version (creates a new version)
    async fn restore_version(&self, document_id: Uuid, version: i32) -> DocumentResult<Document>;
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
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let tags = req.tags.map(|t| serde_json::json!(t)).unwrap_or(serde_json::json!([]));

        sqlx::query(
            r#"INSERT INTO documents
               (id, company_id, title, content, content_type, category, tags, created_by_agent_id, version, status, metadata, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 1, 'draft', '{}'::jsonb, $9, $10)"#,
        )
        .bind(id)
        .bind(req.company_id)
        .bind(&req.title)
        .bind(&req.content)
        .bind(&req.content_type)
        .bind(&req.category)
        .bind(&tags)
        .bind(req.created_by_agent_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(Document {
            id,
            company_id: req.company_id,
            title: req.title,
            content: req.content,
            content_type: req.content_type,
            category: req.category,
            tags,
            created_by_agent_id: req.created_by_agent_id,
            updated_by_agent_id: None,
            version: 1,
            status: "draft".into(),
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_document(&self, document_id: Uuid) -> DocumentResult<Option<Document>> {
        Ok(sqlx::query_as::<_, Document>(
            "SELECT id, company_id, title, content, content_type, category, tags, \
             created_by_agent_id, updated_by_agent_id, version, status, metadata, \
             created_at, updated_at FROM documents WHERE id = $1",
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn update_document(&self, document_id: Uuid, req: UpdateDocumentRequest) -> DocumentResult<Document> {
        let now = chrono::Utc::now();
        if let Some(title) = &req.title {
            sqlx::query("UPDATE documents SET title = $1, updated_at = $2, version = version + 1 WHERE id = $3")
                .bind(title).bind(now).bind(document_id)
                .execute(&self.pool).await?;
        }
        if let Some(content) = &req.content {
            sqlx::query("UPDATE documents SET content = $1, updated_at = $2, version = version + 1 WHERE id = $3")
                .bind(content).bind(now).bind(document_id)
                .execute(&self.pool).await?;
        }
        if let Some(content_type) = &req.content_type {
            sqlx::query("UPDATE documents SET content_type = $1, updated_at = $2 WHERE id = $3")
                .bind(content_type).bind(now).bind(document_id)
                .execute(&self.pool).await?;
        }
        if let Some(tags) = &req.tags {
            let j = serde_json::json!(tags);
            sqlx::query("UPDATE documents SET tags = $1, updated_at = $2 WHERE id = $3")
                .bind(&j).bind(now).bind(document_id)
                .execute(&self.pool).await?;
        }
        if let Some(agent_id) = req.updated_by_agent_id {
            sqlx::query("UPDATE documents SET updated_by_agent_id = $1 WHERE id = $2")
                .bind(agent_id).bind(document_id)
                .execute(&self.pool).await?;
        }
        self.get_document(document_id).await?.ok_or_else(|| DocumentServiceError::NotFound(document_id))
    }

    async fn delete_document(&self, document_id: Uuid) -> DocumentResult<()> {
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(document_id).execute(&self.pool).await?;
        Ok(())
    }

    // ── Version (revision) support via document_versions table ───────

    async fn create_version(&self, document_id: Uuid, change_summary: Option<String>) -> DocumentResult<DocumentVersion> {
        let doc = self.get_document(document_id).await?.ok_or_else(|| DocumentServiceError::NotFound(document_id))?;

        let version = sqlx::query_as::<_, DocumentVersion>(
            r#"
            INSERT INTO document_versions (document_id, version, title, content, updated_by_agent_id, change_summary)
            VALUES ($1, (SELECT COALESCE(MAX(version), 0) + 1 FROM document_versions WHERE document_id = $1),
                    $2, $3, $4, $5)
            RETURNING id, document_id, version, title, content, updated_by_agent_id, change_summary, metadata, created_at
            "#,
        )
        .bind(document_id)
        .bind(&doc.title)
        .bind(&doc.content)
        .bind(doc.updated_by_agent_id)
        .bind(change_summary)
        .fetch_one(&self.pool)
        .await?;

        Ok(version)
    }

    async fn list_versions(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentVersion>> {
        Ok(sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version, title, content, updated_by_agent_id, change_summary, metadata, created_at \
             FROM document_versions WHERE document_id = $1 ORDER BY version DESC",
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn get_version(&self, document_id: Uuid, version: i32) -> DocumentResult<DocumentVersion> {
        sqlx::query_as::<_, DocumentVersion>(
            "SELECT id, document_id, version, title, content, updated_by_agent_id, change_summary, metadata, created_at \
             FROM document_versions WHERE document_id = $1 AND version = $2",
        )
        .bind(document_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DocumentServiceError::VersionNotFound(document_id, version))
    }

    async fn restore_version(&self, document_id: Uuid, version: i32) -> DocumentResult<Document> {
        let ver = self.get_version(document_id, version).await?;
        let now = chrono::Utc::now();

        sqlx::query(
            "UPDATE documents SET title = $1, content = $2, updated_at = $3, version = version + 1 WHERE id = $4",
        )
        .bind(&ver.title)
        .bind(&ver.content)
        .bind(now)
        .bind(document_id)
        .execute(&self.pool)
        .await?;

        self.get_document(document_id).await?.ok_or_else(|| DocumentServiceError::NotFound(document_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run_migrations(pool: &PgPool) {
        sqlx::migrate!("../../migrations")
            .run(pool)
            .await
            .expect("migrations should apply");
    }

    /// Create a minimal company so FK constraints are satisfied
    async fn seed_company(pool: &PgPool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO companies (id, name, issue_prefix, created_at, updated_at) \
             VALUES ($1, 'test', 'TST', NOW(), NOW())"
        )
            .bind(id)
            .execute(pool)
            .await
            .expect("seed company");
        id
    }

    #[sqlx::test]
    async fn create_and_get_document(pool: PgPool) {
        run_migrations(&pool).await;
        let company_id = seed_company(&pool).await;
        let svc = DocumentServiceImpl::new(pool);
        let doc = svc.create_document(CreateDocumentRequest {
            company_id,
            title: "Test".into(),
            content: "hello".into(),
            content_type: "markdown".into(),
            category: None,
            tags: Some(vec!["tag1".into()]),
            created_by_agent_id: None,
        }).await.unwrap();
        assert_eq!(doc.title, "Test");
        assert_eq!(doc.content, "hello");
        assert_eq!(doc.tags.as_array().unwrap().len(), 1);
        let got = svc.get_document(doc.id).await.unwrap().unwrap();
        assert_eq!(got.title, "Test");
    }

    #[sqlx::test]
    async fn version_lifecycle(pool: PgPool) {
        run_migrations(&pool).await;
        let company_id = seed_company(&pool).await;
        let svc = DocumentServiceImpl::new(pool);
        let doc = svc.create_document(CreateDocumentRequest {
            company_id,
            title: "Doc".into(),
            content: "v1".into(),
            content_type: "markdown".into(),
            category: None,
            tags: None,
            created_by_agent_id: None,
        }).await.unwrap();

        // Snapshot v1
        let v1 = svc.create_version(doc.id, Some("initial".into())).await.unwrap();
        assert_eq!(v1.version, 1);
        assert_eq!(v1.content, "v1");

        // Update to v2
        svc.update_document(doc.id, UpdateDocumentRequest {
            title: Some("Doc v2".into()),
            content: Some("v2".into()),
            content_type: None,
            tags: None,
            updated_by_agent_id: None,
            change_summary: Some("updated".into()),
        }).await.unwrap();

        // Snapshot v2
        let v2 = svc.create_version(doc.id, Some("updated".into())).await.unwrap();
        assert_eq!(v2.version, 2);
        assert_eq!(v2.content, "v2");

        // List versions (descending)
        let versions = svc.list_versions(doc.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 2);

        // Get specific version
        let got = svc.get_version(doc.id, 1).await.unwrap();
        assert_eq!(got.content, "v1");

        // Restore version 1
        let restored = svc.restore_version(doc.id, 1).await.unwrap();
        assert_eq!(restored.title, "Doc");
        assert_eq!(restored.content, "v1");

        // Verify current
        let current = svc.get_document(doc.id).await.unwrap().unwrap();
        assert_eq!(current.content, "v1");
        assert!(current.version >= 2);
    }
}
