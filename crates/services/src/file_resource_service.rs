use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum FileResourceError {
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Path traversal attack detected: {0}")]
    PathTraversalDetected(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(Uuid),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Workspace kind enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceKind {
    ExecutionWorkspace,
    ProjectWorkspace,
}

/// File resource provider enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileResourceProvider {
    LocalFs,
    GitWorktree,
}

/// Workspace candidate for file access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCandidate {
    pub workspace_kind: WorkspaceKind,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub provider: FileResourceProvider,
    pub root_path: String,
    pub remote: Option<String>,
}

/// File entry information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// File preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePreview {
    pub content: String,
    pub language: Option<String>,
    pub truncated: bool,
    pub byte_size: usize,
}

/// Workspace file resources service trait
#[async_trait]
pub trait WorkspaceFileResourcesService: Send + Sync {
    /// List files in a workspace directory
    async fn list_files(
        &self,
        workspace: &WorkspaceCandidate,
        sub_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, FileResourceError>;

    /// Preview file content
    async fn preview_file(
        &self,
        workspace: &WorkspaceCandidate,
        file_path: &str,
        max_bytes: Option<usize>,
    ) -> Result<FilePreview, FileResourceError>;

    /// Download file content
    async fn download_file(
        &self,
        workspace: &WorkspaceCandidate,
        file_path: &str,
    ) -> Result<Vec<u8>, FileResourceError>;
}

/// Default implementation of file resource service
pub struct DefaultFileResourceService;

impl DefaultFileResourceService {
    pub fn new() -> Self {
        Self
    }

    /// Validate and resolve file path within workspace root
    fn resolve_path(
        root_path: &str,
        sub_path: Option<&str>,
    ) -> Result<PathBuf, FileResourceError> {
        let root = PathBuf::from(root_path);

        let resolved = if let Some(sub) = sub_path {
            // Normalize the sub path
            let normalized = sub.replace("\\", "/");

            // Check for path traversal attempts
            if normalized.contains("..") || normalized.starts_with('/') {
                return Err(FileResourceError::PathTraversalDetected(
                    normalized.to_string(),
                ));
            }

            root.join(normalized)
    } else {
            root.clone()
        };

        // Ensure resolved path is within root
        let canonical_root = root.canonicalize().map_err(|e| {
            FileResourceError::InvalidPath(format!("Invalid root path: {}", e))
        })?;

        let canonical_resolved = resolved.canonicalize().unwrap_or(resolved.clone());

        if !canonical_resolved.starts_with(&canonical_root) {
            return Err(FileResourceError::PathTraversalDetected(
                format!("Path escapes workspace root: {:?}", canonical_resolved),
            ));
        }

        Ok(canonical_resolved)
    }

    /// Check if path is sensitive (should not be accessed)
    fn is_sensitive_path(path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // Block .git directory
        if path_str.contains("/.git/") || path_str.ends_with("/.git") {
            return true;
        }

        // Block other sensitive directories
        let sensitive_dirs = [".env", ".ssh", ".aws", ".config"];
        for dir in &sensitive_dirs {
            if path_str.contains(&format!("/{}/", dir)) || path_str.ends_with(&format!("/{}", dir)) {
                return true;
            }
        }

        false
    }

    /// Detect language from file extension
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "rs" => "rust",
                "ts" | "tsx" => "typescript",
                "js" | "jsx" => "javascript",
                "py" => "python",
                "go" => "go",
                "java" => "java",
                "cpp" | "cc" | "cxx" => "cpp",
                "c" | "h" => "c",
                "md" => "markdown",
                "json" => "json",
                "yaml" | "yml" => "yaml",
                "toml" => "toml",
                "sh" | "bash" => "shell",
                "sql" => "sql",
                _ => "plaintext",
            })
            .map(String::from)
    }
}

impl Default for DefaultFileResourceService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WorkspaceFileResourcesService for DefaultFileResourceService {
    async fn list_files(
        &self,
        workspace: &WorkspaceCandidate,
        sub_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, FileResourceError> {
        match workspace.provider {
            FileResourceProvider::LocalFs => {
                let dir_path = Self::resolve_path(&workspace.root_path, sub_path)?;

                if Self::is_sensitive_path(&dir_path) {
                    return Err(FileResourceError::PermissionDenied(
                        "Access to sensitive directory denied".to_string(),
                    ));
                }

                let mut entries = Vec::new();
                let mut read_dir = tokio::fs::read_dir(&dir_path).await?;

                while let Some(entry) = read_dir.next_entry().await? {
                    let metadata = entry.metadata().await?;
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let file_path = entry.path();

                    // Skip sensitive paths
                    if Self::is_sensitive_path(&file_path) {
                        continue;
                    }

                    let relative_path = file_path
                        .strip_prefix(&workspace.root_path)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .to_string();

                    let modified_at = metadata
                        .modified()
                        .ok()
                        .and_then(|t| {
                            t.duration_since(std::time::UNIX_EPOCH)
                                .ok()
                                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                        })
                        .flatten();

                    entries.push(FileEntry {
                        name: file_name,
                        path: relative_path,
                        is_dir: metadata.is_dir(),
                        size: if metadata.is_file() {
                            Some(metadata.len())
                        } else {
                            None
                        },
                        modified_at,
                    });
                }

                // Sort: directories first, then by name
                entries.sort_by(|a, b| {
                    match (a.is_dir, b.is_dir) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        _ => a.name.cmp(&b.name),
                    }
                });

                Ok(entries)
            }
            FileResourceProvider::GitWorktree => {
                // TODO: Implement git worktree provider
                Err(FileResourceError::Internal(
                    "GitWorktree provider not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn preview_file(
        &self,
        workspace: &WorkspaceCandidate,
        file_path: &str,
        max_bytes: Option<usize>,
    ) -> Result<FilePreview, FileResourceError> {
        match workspace.provider {
            FileResourceProvider::LocalFs => {
                let resolved_path = Self::resolve_path(&workspace.root_path, Some(file_path))?;

                if Self::is_sensitive_path(&resolved_path) {
                    return Err(FileResourceError::PermissionDenied(
                        "Access to sensitive file denied".to_string(),
                    ));
                }

                let metadata = tokio::fs::metadata(&resolved_path).await?;
                if !metadata.is_file() {
                    return Err(FileResourceError::InvalidPath(
                        "Path is not a file".to_string(),
                    ));
                }

                let file_size = metadata.len() as usize;
                let max_read = max_bytes.unwrap_or(1024 * 1024); // Default 1MB

                let content_bytes = tokio::fs::read(&resolved_path).await?;
                let truncated = content_bytes.len() > max_read;
                let preview_bytes = if truncated {
                    &content_bytes[..max_read]
                } else {
                    &content_bytes[..]
                };

                let content = String::from_utf8_lossy(preview_bytes).to_string();
                let language = Self::detect_language(&resolved_path);

                Ok(FilePreview {
                    content,
                    language,
                    truncated,
                    byte_size: file_size,
                })
            }
            FileResourceProvider::GitWorktree => {
                Err(FileResourceError::Internal(
                    "GitWorktree provider not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn download_file(
        &self,
        workspace: &WorkspaceCandidate,
        file_path: &str,
    ) -> Result<Vec<u8>, FileResourceError> {
        match workspace.provider {
            FileResourceProvider::LocalFs => {
                let resolved_path = Self::resolve_path(&workspace.root_path, Some(file_path))?;

                if Self::is_sensitive_path(&resolved_path) {
                    return Err(FileResourceError::PermissionDenied(
                        "Access to sensitive file denied".to_string(),
                    ));
                }

                let content = tokio::fs::read(&resolved_path).await?;
                Ok(content)
            }
            FileResourceProvider::GitWorktree => {
                Err(FileResourceError::Internal(
                    "GitWorktree provider not yet implemented".to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_traversal_detection() {
        let result = DefaultFileResourceService::resolve_path("/root", Some("../etc/passwd"));
        assert!(result.is_err());

        let result = DefaultFileResourceService::resolve_path("/root", Some("/etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn test_sensitive_path_detection() {
        assert!(DefaultFileResourceService::is_sensitive_path(Path::new("/root/.git/config")));
        assert!(DefaultFileResourceService::is_sensitive_path(Path::new("/root/.env")));
        assert!(DefaultFileResourceService::is_sensitive_path(Path::new("/root/.ssh/id_rsa")));
        assert!(!DefaultFileResourceService::is_sensitive_path(Path::new("/root/src/main.rs")));
    }

    #[test]
    fn test_language_detection() {
        assert_eq!(
            DefaultFileResourceService::detect_language(Path::new("test.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            DefaultFileResourceService::detect_language(Path::new("app.tsx")),
            Some("typescript".to_string())
        );
        assert_eq!(
            DefaultFileResourceService::detect_language(Path::new("README.md")),
            Some("markdown".to_string())
        );
    }
}
