use async_trait::async_trait;
use models::{Asset, CreateAssetInput, AssetContent, StoragePutResult, MAX_ATTACHMENT_BYTES};
use repositories::{AssetRepository, RepositoryError};
use sha2::{Sha256, Digest};
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AssetServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Asset not found: {0}")]
    AssetNotFound(Uuid),

    #[error("File too large: {0} bytes (max: {1} bytes)")]
    FileTooLarge(usize, usize),

    #[error("Invalid content type: {0}")]
    InvalidContentType(String),

    #[error("SVG sanitization failed: {0}")]
    SvgSanitizationFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Storage service trait for file operations
#[async_trait]
pub trait StorageService: Send + Sync {
    /// Store a file and return storage metadata
    async fn put_file(
        &self,
        content: &[u8],
        content_type: &str,
        filename: Option<&str>,
    ) -> Result<StoragePutResult, AssetServiceError>;

    /// Retrieve file content
    async fn get_file(&self, object_key: &str) -> Result<Vec<u8>, AssetServiceError>;

    /// Delete a file
    async fn delete_file(&self, object_key: &str) -> Result<(), AssetServiceError>;

    /// Get storage provider name
    fn provider_name(&self) -> &str;
}

/// Asset service trait
#[async_trait]
pub trait AssetService: Send + Sync {
    /// Create a new asset
    async fn create(
        &self,
        company_id: Uuid,
        content: &[u8],
        content_type: &str,
        original_filename: Option<String>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<Asset, AssetServiceError>;

    /// Get asset by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Asset>, AssetServiceError>;

    /// Get asset content
    async fn get_content(&self, id: Uuid) -> Result<AssetContent, AssetServiceError>;

    /// Delete asset
    async fn delete(&self, id: Uuid) -> Result<(), AssetServiceError>;

    /// List assets by company
    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Asset>, AssetServiceError>;

    /// Check if asset exists by SHA256 (deduplication)
    async fn find_by_hash(&self, company_id: Uuid, sha256: &str) -> Result<Option<Asset>, AssetServiceError>;
}

/// Local filesystem storage provider
pub struct LocalStorageProvider {
    base_path: PathBuf,
}

impl LocalStorageProvider {
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    fn ensure_directory(&self) -> Result<(), AssetServiceError> {
        std::fs::create_dir_all(&self.base_path)
            .map_err(|e| AssetServiceError::Storage(format!("Failed to create storage directory: {}", e)))?;
        Ok(())
    }

    fn object_path(&self, object_key: &str) -> PathBuf {
        self.base_path.join(object_key)
    }
}

#[async_trait]
impl StorageService for LocalStorageProvider {
    async fn put_file(
        &self,
        content: &[u8],
        content_type: &str,
        _filename: Option<&str>,
    ) -> Result<StoragePutResult, AssetServiceError> {
        self.ensure_directory()?;

        // Generate SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(content);
        let sha256 = format!("{:x}", hasher.finalize());

        // Generate object key: first 2 chars of hash as directory, then hash as filename
        let dir = &sha256[0..2];
        let object_key = format!("{}/{}", dir, sha256);

        // Create subdirectory
        let subdir = self.base_path.join(dir);
        std::fs::create_dir_all(&subdir)?;

        // Write file
        let file_path = self.object_path(&object_key);
        tokio::fs::write(&file_path, content).await?;

        Ok(StoragePutResult {
            provider: "local_fs".to_string(),
            object_key,
            content_type: content_type.to_string(),
            byte_size: content.len() as i64,
            sha256,
        })
    }

    async fn get_file(&self, object_key: &str) -> Result<Vec<u8>, AssetServiceError> {
        let file_path = self.object_path(object_key);
        let content = tokio::fs::read(&file_path).await?;
        Ok(content)
    }

    async fn delete_file(&self, object_key: &str) -> Result<(), AssetServiceError> {
        let file_path = self.object_path(object_key);
        tokio::fs::remove_file(&file_path).await?;
        Ok(())
    }

    fn provider_name(&self) -> &str {
        "local_fs"
    }
}

/// Sanitize SVG content by removing dangerous elements
pub fn sanitize_svg_buffer(input: &[u8]) -> Result<Vec<u8>, AssetServiceError> {
    let content = String::from_utf8(input.to_vec())
        .map_err(|e| AssetServiceError::SvgSanitizationFailed(format!("Invalid UTF-8: {}", e)))?;

    // Simple sanitization: remove script tags, event handlers, and foreignObject
    let mut sanitized = content;

    // Remove script tags
    sanitized = remove_tags(&sanitized, "script");

    // Remove foreignObject tags
    sanitized = remove_tags(&sanitized, "foreignObject");

    // Remove event attributes (onclick, onload, etc.)
    let event_handlers = [
        "onclick", "onload", "onerror", "onmouseover", "onmouseout",
        "onmousemove", "onmousedown", "onmouseup", "onfocus", "onblur",
        "onchange", "onsubmit", "onkeydown", "onkeyup", "onkeypress",
    ];

    for handler in &event_handlers {
        // Remove with single quotes
        sanitized = sanitized.replace(&format!(r#"{}='"#, handler), "");
        sanitized = sanitized.replace(&format!(r#"{}=""#, handler), "");
        // Remove standalone
        let pattern = format!(r#"{}="#, handler);
        while let Some(start) = sanitized.find(&pattern) {
            if let Some(end) = sanitized[start..].find('"').and_then(|i| sanitized[start + i + 1..].find('"')) {
                sanitized.replace_range(start..start + pattern.len() + end + 2, "");
            } else {
                break;
            }
        }
    }

    Ok(sanitized.into_bytes())
}

fn remove_tags(content: &str, tag: &str) -> String {
    let mut result = content.to_string();
    let open_tag = format!("<{}", tag);
    let close_tag = format!("</{}>", tag);

    while let Some(start) = result.find(&open_tag) {
        if let Some(end) = result[start..].find(&close_tag) {
            result.replace_range(start..start + end + close_tag.len(), "");
        } else {
            break;
        }
    }

    result
}

/// Default implementation of AssetService
pub struct DefaultAssetService<R, S>
where
    R: AssetRepository,
    S: StorageService,
{
    asset_repo: Arc<R>,
    storage: Arc<S>,
}

impl<R, S> DefaultAssetService<R, S>
where
    R: AssetRepository,
    S: StorageService,
{
    pub fn new(asset_repo: Arc<R>, storage: Arc<S>) -> Self {
        Self {
            asset_repo,
            storage,
        }
    }
}

#[async_trait]
impl<R, S> AssetService for DefaultAssetService<R, S>
where
    R: AssetRepository + 'static,
    S: StorageService + 'static,
{
    async fn create(
        &self,
        company_id: Uuid,
        content: &[u8],
        content_type: &str,
        original_filename: Option<String>,
        created_by_agent_id: Option<Uuid>,
        created_by_user_id: Option<Uuid>,
    ) -> Result<Asset, AssetServiceError> {
        // Check file size
        if content.len() > MAX_ATTACHMENT_BYTES {
            return Err(AssetServiceError::FileTooLarge(
                content.len(),
                MAX_ATTACHMENT_BYTES,
            ));
        }

        // Sanitize SVG if applicable
        let processed_content = if content_type == "image/svg+xml" {
            sanitize_svg_buffer(content)?
        } else {
            content.to_vec()
        };

        // Store file
        let storage_result = self
            .storage
            .put_file(&processed_content, content_type, original_filename.as_deref())
            .await?;

        // Check for existing asset with same hash (deduplication)
        if let Some(existing) = self
            .asset_repo
            .get_by_sha256(company_id, &storage_result.sha256)
            .await?
        {
            // Return existing asset instead of creating duplicate
            return Ok(existing);
        }

        // Create asset record
        let input = CreateAssetInput {
            company_id,
            provider: storage_result.provider,
            object_key: storage_result.object_key,
            content_type: storage_result.content_type,
            byte_size: storage_result.byte_size,
            sha256: storage_result.sha256,
            original_filename,
            created_by_agent_id,
            created_by_user_id,
        };

        let asset = self.asset_repo.create(input).await?;

        Ok(asset)
    }

    async fn get_by_id(&self, id: Uuid) -> Result<Option<Asset>, AssetServiceError> {
        let asset = self.asset_repo.get_by_id(id).await?;
        Ok(asset)
    }

    async fn get_content(&self, id: Uuid) -> Result<AssetContent, AssetServiceError> {
        let asset = self
            .asset_repo
            .get_by_id(id)
            .await?
            .ok_or(AssetServiceError::AssetNotFound(id))?;

        let body = self.storage.get_file(&asset.object_key).await?;

        Ok(AssetContent {
            content_type: asset.content_type,
            body,
            sha256: asset.sha256,
        })
    }

    async fn delete(&self, id: Uuid) -> Result<(), AssetServiceError> {
        let asset = self
            .asset_repo
            .get_by_id(id)
            .await?
            .ok_or(AssetServiceError::AssetNotFound(id))?;

        // Delete from storage
        self.storage.delete_file(&asset.object_key).await?;

        // Delete asset record
        self.asset_repo.delete(id).await?;

        Ok(())
    }

    async fn list_by_company(&self, company_id: Uuid, limit: i64, offset: i64) -> Result<Vec<Asset>, AssetServiceError> {
        let assets = self.asset_repo.list_by_company(company_id, limit, offset).await?;
        Ok(assets)
    }

    async fn find_by_hash(&self, company_id: Uuid, sha256: &str) -> Result<Option<Asset>, AssetServiceError> {
        let asset = self.asset_repo.get_by_sha256(company_id, sha256).await?;
        Ok(asset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_svg_removes_script() {
        let input = br#"<svg><script>alert('xss')</script><circle r="10"/></svg>"#;
        let result = sanitize_svg_buffer(input).unwrap();
        let output = String::from_utf8(result).unwrap();

        assert!(!output.contains("script"));
        assert!(output.contains("circle"));
    }

    #[test]
    fn test_sanitize_svg_removes_event_handlers() {
        let input = br#"<svg onclick="alert('xss')"><circle r="10"/></svg>"#;
        let result = sanitize_svg_buffer(input).unwrap();
        let output = String::from_utf8(result).unwrap();

        assert!(!output.contains("onclick"));
        assert!(output.contains("circle"));
    }

    #[test]
    fn test_file_size_limit() {
        assert_eq!(MAX_ATTACHMENT_BYTES, 10 * 1024 * 1024);
    }
}
