use async_trait::async_trait;
use models::issue_auxiliary::{Attachment, UploadAttachmentInput};
use uuid::Uuid;

use crate::asset_storage::{LocalStorageService, PutFileRequest, StorageService};
use crate::attachment_types::{
    is_allowed_content_type, max_attachment_bytes, normalize_upload_attachment_content_type,
};
use crate::errors::{ServiceError, ServiceResult};
use sqlx::PgPool;
use std::sync::Arc;

/// 附件对象（内容 + 回放所需的元信息），供 content 端点构造响应头。
#[derive(Debug, Clone)]
pub struct AttachmentObject {
    pub data: Vec<u8>,
    pub content_type: String,
    pub filename: String,
    pub byte_size: i64,
}

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

/// 上传前的输入校验（纯函数，便于单测）。
///
/// 返回归一化后的 content type。校验顺序与 Paperclip 保持一致：
/// 1. filename 必填
/// 2. size 与 content 长度一致（客户端声明与实际不符属于 400）
/// 3. 空文件 → 422
/// 4. 超过上限 → 422
/// 5. 类型不在白名单 → 422
pub fn validate_attachment_upload(
    filename: &str,
    declared_content_type: Option<&str>,
    declared_size: i64,
    actual_len: usize,
    max_bytes: i64,
) -> ServiceResult<String> {
    if filename.trim().is_empty() {
        return Err(ServiceError::Validation("filename is required".into()));
    }
    if declared_size != actual_len as i64 {
        return Err(ServiceError::Validation("size does not match content".into()));
    }
    if actual_len == 0 {
        return Err(ServiceError::Unprocessable("Attachment is empty".into()));
    }
    if actual_len as i64 > max_bytes {
        return Err(ServiceError::Unprocessable(format!(
            "Attachment exceeds {} bytes",
            max_bytes
        )));
    }
    let content_type = normalize_upload_attachment_content_type(declared_content_type, Some(filename));
    if !is_allowed_content_type(&content_type) {
        return Err(ServiceError::Unprocessable(format!(
            "Unsupported file type: {}",
            content_type
        )));
    }
    Ok(content_type)
}

/// Mock implementation
pub struct MockAttachmentService;

/// Filesystem-backed implementation matching Paperclip's asset + attachment
/// split: metadata lives in Postgres and bytes live under object_key.
pub struct LocalAttachmentService {
    pool: PgPool,
    storage: Arc<dyn StorageService>,
}

impl LocalAttachmentService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            storage: Arc::new(LocalStorageService::from_env()),
        }
    }

    /// 允许注入自定义 storage（测试或后续接入对象存储时使用）。
    pub fn with_storage(pool: PgPool, storage: Arc<dyn StorageService>) -> Self {
        Self { pool, storage }
    }

    /// 元信息读取，供 content 端点构造 Content-Type / Content-Disposition。
    pub async fn get_attachment_object(
        &self,
        attachment_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<AttachmentObject> {
        let row: Option<(String, String, String, i64)> = sqlx::query_as(
            "SELECT a.object_key, x.filename, x.content_type, x.size_bytes
             FROM assets a JOIN attachments x ON x.asset_id = a.id
             WHERE x.id = $1 AND x.company_id = $2",
        )
        .bind(attachment_id)
        .bind(company_id)
        .fetch_optional(&self.pool)
        .await?;
        let (key, filename, content_type, byte_size) =
            row.ok_or_else(|| ServiceError::NotFound("attachment not found".into()))?;
        let data = self.storage.get_object(company_id, &key).await?;
        Ok(AttachmentObject { data, content_type, filename, byte_size })
    }
}

#[async_trait]
impl AttachmentService for LocalAttachmentService {
    async fn list_attachments(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<Attachment>> {
        sqlx::query_as::<_, Attachment>(
            "SELECT id, parent_type, parent_id, company_id, asset_id, filename, content_type, size_bytes AS size, created_at
             FROM attachments WHERE parent_type = $1 AND parent_id = $2 AND company_id = $3 ORDER BY created_at ASC")
            .bind(parent_type).bind(parent_id).bind(company_id).fetch_all(&self.pool).await.map_err(Into::into)
    }

    async fn upload_attachment(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
        input: UploadAttachmentInput,
    ) -> ServiceResult<Attachment> {
        let content_type = validate_attachment_upload(
            &input.filename,
            Some(&input.content_type),
            input.size,
            input.content.len(),
            max_attachment_bytes(),
        )?;

        let stored = self
            .storage
            .put_file(PutFileRequest {
                company_id,
                namespace: format!("{}s/{}", parent_type, parent_id),
                original_filename: Some(input.filename.clone()),
                content_type: content_type.clone(),
                body: input.content.clone(),
            })
            .await?;

        // 元数据写入失败时必须把已落盘的对象删掉，避免产生孤儿文件。
        let result = self
            .insert_attachment_rows(
                parent_type,
                parent_id,
                company_id,
                &stored.object_key,
                &content_type,
                &stored.sha256,
                &input,
            )
            .await;
        if result.is_err() {
            let _ = self.storage.delete_object(company_id, &stored.object_key).await;
        }
        result
    }

    async fn get_attachment_content(
        &self,
        attachment_id: Uuid,
        company_id: Uuid,
    ) -> ServiceResult<Vec<u8>> {
        let key: Option<String> = sqlx::query_scalar(
            "SELECT a.object_key FROM assets a JOIN attachments x ON x.asset_id = a.id WHERE x.id = $1 AND x.company_id = $2")
            .bind(attachment_id).bind(company_id).fetch_optional(&self.pool).await?;
        let key = key.ok_or_else(|| ServiceError::NotFound("attachment not found".into()))?;
        self.storage.get_object(company_id, &key).await
    }

    async fn delete_attachment(&self, attachment_id: Uuid, company_id: Uuid) -> ServiceResult<()> {
        let key: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT a.id, a.object_key FROM assets a JOIN attachments x ON x.asset_id = a.id WHERE x.id = $1 AND x.company_id = $2")
            .bind(attachment_id).bind(company_id).fetch_optional(&self.pool).await?;
        let (asset_id, object_key) =
            key.ok_or_else(|| ServiceError::NotFound("attachment not found".into()))?;
        // 先删元数据（事务内），成功后再删除物理文件；文件删除失败不影响幂等性。
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM attachments WHERE id = $1 AND company_id = $2")
            .bind(attachment_id)
            .bind(company_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM assets WHERE id = $1 AND company_id = $2")
            .bind(asset_id)
            .bind(company_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let _ = self.storage.delete_object(company_id, &object_key).await;
        Ok(())
    }
}

impl LocalAttachmentService {
    #[allow(clippy::too_many_arguments)]
    async fn insert_attachment_rows(
        &self,
        parent_type: &str,
        parent_id: Uuid,
        company_id: Uuid,
        key: &str,
        content_type: &str,
        sha: &str,
        input: &UploadAttachmentInput,
    ) -> ServiceResult<Attachment> {
        let mut tx = self.pool.begin().await?;
        let asset_id: Uuid = sqlx::query_scalar(
            "INSERT INTO assets (company_id, provider, object_key, content_type, byte_size, sha256, original_filename)
             VALUES ($1, 'local', $2, $3, $4, $5, $6) RETURNING id")
            .bind(company_id)
            .bind(key)
            .bind(content_type)
            .bind(input.size)
            .bind(sha)
            .bind(&input.filename)
            .fetch_one(&mut *tx)
            .await?;
        let attachment = sqlx::query_as::<_, Attachment>(
            "INSERT INTO attachments (company_id, parent_type, parent_id, asset_id, filename, content_type, size_bytes)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, parent_type, parent_id, company_id, asset_id, filename, content_type, size_bytes AS size, created_at")
            .bind(company_id)
            .bind(parent_type)
            .bind(parent_id)
            .bind(asset_id)
            .bind(&input.filename)
            .bind(content_type)
            .bind(input.size)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(attachment)
    }
}

impl MockAttachmentService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockAttachmentService {
    fn default() -> Self {
        Self::new()
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
        Err(ServiceError::NotImplemented(
            "AttachmentService::upload_attachment not implemented".to_string(),
        ))
    }

    async fn get_attachment_content(
        &self,
        _attachment_id: Uuid,
        _company_id: Uuid,
    ) -> ServiceResult<Vec<u8>> {
        Err(ServiceError::NotImplemented(
            "AttachmentService::get_attachment_content not implemented".to_string(),
        ))
    }

    async fn delete_attachment(&self, _attachment_id: Uuid, _company_id: Uuid) -> ServiceResult<()> {
        Err(ServiceError::NotImplemented(
            "AttachmentService::delete_attachment not implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_msg(e: ServiceError) -> String {
        e.to_string()
    }

    #[test]
    fn upload_validation_requires_filename() {
        let err = validate_attachment_upload("  ", Some("image/png"), 3, 3, 1024).unwrap_err();
        assert!(matches!(err, ServiceError::Validation(_)));
        assert!(err_msg(err).contains("filename"));
    }

    #[test]
    fn upload_validation_rejects_size_mismatch() {
        let err = validate_attachment_upload("a.png", Some("image/png"), 99, 3, 1024).unwrap_err();
        assert!(matches!(err, ServiceError::Validation(_)));
    }

    #[test]
    fn upload_validation_rejects_empty_file() {
        let err = validate_attachment_upload("a.png", Some("image/png"), 0, 0, 1024).unwrap_err();
        assert!(matches!(err, ServiceError::Unprocessable(_)));
        assert!(err_msg(err).contains("empty"));
    }

    #[test]
    fn upload_validation_rejects_oversized_file() {
        let err = validate_attachment_upload("a.png", Some("image/png"), 10, 10, 4).unwrap_err();
        assert!(matches!(err, ServiceError::Unprocessable(_)));
        assert!(err_msg(err).contains("exceeds"));
    }

    #[test]
    fn upload_validation_rejects_unsupported_type() {
        let err =
            validate_attachment_upload("a.exe", Some("application/x-msdownload"), 4, 4, 1024).unwrap_err();
        assert!(matches!(err, ServiceError::Unprocessable(_)));
        assert!(err_msg(err).contains("Unsupported file type"));
    }

    #[test]
    fn upload_validation_normalizes_office_content_type() {
        let ct = validate_attachment_upload("plan.docx", Some("application/octet-stream"), 4, 4, 1024).unwrap();
        assert_eq!(
            ct,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
    }

    #[test]
    fn upload_validation_accepts_normal_image() {
        let ct = validate_attachment_upload("a.PNG", Some("Image/PNG"), 4, 4, 1024).unwrap();
        assert_eq!(ct, "image/png");
    }

    #[test]
    fn upload_validation_order_prefers_size_mismatch_over_type() {
        // size 与 content 不一致时应先报 400，而不是先做类型判定
        let err = validate_attachment_upload("a.exe", Some("application/x-msdownload"), 9, 3, 1024).unwrap_err();
        assert!(matches!(err, ServiceError::Validation(_)));
    }
}
