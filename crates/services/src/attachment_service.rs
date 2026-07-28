use async_trait::async_trait;
use models::issue_auxiliary::{Attachment, UploadAttachmentInput};
use uuid::Uuid;

use crate::errors::ServiceResult;
use sqlx::PgPool;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

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

/// Filesystem-backed implementation matching Paperclip's asset + attachment
/// split: metadata lives in Postgres and bytes live under object_key.
pub struct LocalAttachmentService {
    pool: PgPool,
    root: PathBuf,
}

impl LocalAttachmentService {
    pub fn new(pool: PgPool) -> Self {
        let root = std::env::var_os("PARROT_ASSET_STORAGE_DIR")
            .map(PathBuf::from).unwrap_or_else(|| PathBuf::from("data/assets"));
        Self { pool, root }
    }
    fn path(&self, key: &str) -> PathBuf { self.root.join(key) }
}

#[async_trait]
impl AttachmentService for LocalAttachmentService {
    async fn list_attachments(&self, parent_type: &str, parent_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<Attachment>> {
        sqlx::query_as::<_, Attachment>(
            "SELECT id, parent_type, parent_id, company_id, asset_id, filename, content_type, size_bytes AS size, created_at
             FROM attachments WHERE parent_type = $1 AND parent_id = $2 AND company_id = $3 ORDER BY created_at ASC")
            .bind(parent_type).bind(parent_id).bind(company_id).fetch_all(&self.pool).await.map_err(Into::into)
    }

    async fn upload_attachment(&self, parent_type: &str, parent_id: Uuid, company_id: Uuid, input: UploadAttachmentInput) -> ServiceResult<Attachment> {
        if input.filename.trim().is_empty() { return Err(crate::errors::ServiceError::Validation("filename is required".into())); }
        if input.content.len() as i64 != input.size { return Err(crate::errors::ServiceError::Validation("size does not match content".into())); }
        let mut hasher = Sha256::new(); hasher.update(&input.content);
        let sha = format!("{:x}", hasher.finalize());
        let key = format!("{}/{}-{}", company_id, Uuid::new_v4(), input.filename.replace('/', "_"));
        let path = self.path(&key);
        if let Some(parent) = path.parent() { tokio::fs::create_dir_all(parent).await.map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?; }
        tokio::fs::write(&path, &input.content).await.map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?;
        let asset_id: Uuid = sqlx::query_scalar(
            "INSERT INTO assets (company_id, provider, object_key, content_type, byte_size, sha256, original_filename)
             VALUES ($1, 'local', $2, $3, $4, $5, $6) RETURNING id")
            .bind(company_id).bind(&key).bind(&input.content_type).bind(input.size).bind(&sha).bind(&input.filename)
            .fetch_one(&self.pool).await?;
        sqlx::query_as::<_, Attachment>(
            "INSERT INTO attachments (company_id, parent_type, parent_id, asset_id, filename, content_type, size_bytes)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, parent_type, parent_id, company_id, asset_id, filename, content_type, size_bytes AS size, created_at")
            .bind(company_id).bind(parent_type).bind(parent_id).bind(asset_id).bind(input.filename).bind(input.content_type).bind(input.size)
            .fetch_one(&self.pool).await.map_err(Into::into)
    }

    async fn get_attachment_content(&self, attachment_id: Uuid, company_id: Uuid) -> ServiceResult<Vec<u8>> {
        let key: Option<String> = sqlx::query_scalar(
            "SELECT a.object_key FROM assets a JOIN attachments x ON x.asset_id = a.id WHERE x.id = $1 AND x.company_id = $2")
            .bind(attachment_id).bind(company_id).fetch_optional(&self.pool).await?;
        let key = key.ok_or_else(|| crate::errors::ServiceError::NotFound("attachment not found".into()))?;
        Ok(tokio::fs::read(self.path(&key)).await.map_err(|e| crate::errors::ServiceError::Internal(e.to_string()))?)
    }

    async fn delete_attachment(&self, attachment_id: Uuid, company_id: Uuid) -> ServiceResult<()> {
        let key: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT a.id, a.object_key FROM assets a JOIN attachments x ON x.asset_id = a.id WHERE x.id = $1 AND x.company_id = $2")
            .bind(attachment_id).bind(company_id).fetch_optional(&self.pool).await?;
        let (asset_id, object_key) = key.ok_or_else(|| crate::errors::ServiceError::NotFound("attachment not found".into()))?;
        sqlx::query("DELETE FROM attachments WHERE id = $1 AND company_id = $2").bind(attachment_id).bind(company_id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM assets WHERE id = $1 AND company_id = $2").bind(asset_id).bind(company_id).execute(&self.pool).await?;
        let _ = tokio::fs::remove_file(self.path(&object_key)).await;
        Ok(())
    }
}

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
