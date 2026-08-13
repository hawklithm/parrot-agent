//! 对象存储抽象 —— 对齐 Paperclip `server/src/storage/types.ts` 的 `StorageService`。
//!
//! Parrot 目前只有本地文件系统实现，但接口按 Paperclip 的 `putFile / getObject /
//! deleteObject` 三件套定义，后续接 S3/COS 时不需要改调用方。
//!
//! 统一约束（与 Paperclip `storage/service.ts` 对齐）：
//! - object_key 布局 `<company_id>/<namespace>/<YYYY>/<MM>/<DD>/<uuid>-<stem><ext>`，
//!   公司 id 作为第一段，等价 Paperclip 的 `ensureCompanyPrefix`。
//! - 文件名/命名空间分段做安全化处理，杜绝 `..`、路径分隔符与空字节导致的目录穿越。
//! - 读取时若对象缺失返回 `NotFound`（→404）而不是 500。

use async_trait::async_trait;
use chrono::{Datelike, Utc};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::errors::{ServiceError, ServiceResult};

/// 落盘结果，字段与 `assets` 表一一对应。
#[derive(Debug, Clone)]
pub struct StoredObject {
    pub provider: String,
    pub object_key: String,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub original_filename: Option<String>,
}

/// 落盘请求。
#[derive(Debug, Clone)]
pub struct PutFileRequest {
    pub company_id: Uuid,
    pub namespace: String,
    pub original_filename: Option<String>,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[async_trait]
pub trait StorageService: Send + Sync {
    async fn put_file(&self, req: PutFileRequest) -> ServiceResult<StoredObject>;
    async fn get_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<Vec<u8>>;
    async fn delete_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<()>;
}

/// 存储根目录：`PARROT_ASSET_STORAGE_DIR`，默认 `data/assets`。
pub fn storage_root() -> PathBuf {
    std::env::var_os("PARROT_ASSET_STORAGE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/assets"))
}

/// 单个路径分段最大长度，与 Paperclip `MAX_SEGMENT_LENGTH` 保持一致。
pub const MAX_SEGMENT_LENGTH: usize = 120;

/// 分段安全化：等价 Paperclip `sanitizeSegment`
/// （`[^a-zA-Z0-9._-]+` → `_`，压缩连续 `_`，去掉首尾 `_`，空值回落 `file`）。
///
/// 额外收敛（Paperclip 靠 `path.basename` 兜底，这里直接在分段层做掉）：
/// 连续 `.` 压缩为一个，并同时去掉首尾的 `.`，杜绝 `..` 出现在任何 key 分段里。
pub fn sanitize_segment(value: &str) -> String {
    let mut collapsed = String::new();
    let mut prev_underscore = false;
    let mut prev_dot = false;
    for ch in value.trim().chars() {
        let allowed = ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-';
        if allowed && ch == '.' {
            if !prev_dot {
                collapsed.push('.');
            }
            prev_dot = true;
            prev_underscore = false;
        } else if allowed && ch != '_' {
            collapsed.push(ch);
            prev_underscore = false;
            prev_dot = false;
        } else if !prev_underscore {
            collapsed.push('_');
            prev_underscore = true;
            prev_dot = false;
        }
    }
    let cleaned = collapsed.trim_matches(|c| c == '_' || c == '.');
    if cleaned.is_empty() {
        return "file".to_string();
    }
    cleaned.chars().take(MAX_SEGMENT_LENGTH).collect()
}

/// namespace 安全化：等价 Paperclip `normalizeNamespace`，空值回落 `misc`。
/// 额外收敛：纯点号分段（`.`、`..`）直接丢弃，避免 key 里出现穿越记号。
pub fn sanitize_namespace(namespace: &str) -> String {
    let segments: Vec<String> = namespace
        .split('/')
        .map(|seg| seg.trim())
        .filter(|seg| !seg.is_empty() && !seg.chars().all(|c| c == '.'))
        .map(sanitize_segment)
        .collect();
    if segments.is_empty() {
        "misc".to_string()
    } else {
        segments.join("/")
    }
}

/// 拆分文件名为 `(stem, ext)`，等价 Paperclip `splitFilename`。
pub fn split_filename(filename: Option<&str>) -> (String, String) {
    let raw = match filename {
        Some(v) => v,
        None => return ("file".to_string(), String::new()),
    };
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if base.is_empty() {
        return ("file".to_string(), String::new());
    }
    let ext_raw = match base.rfind('.') {
        // 与 Node `path.extname` 一致：`.dotfile` 不视为扩展名
        Some(idx) if idx > 0 => &base[idx..],
        _ => "",
    };
    let stem_raw = &base[..base.len() - ext_raw.len()];
    let ext: String = ext_raw
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || *c == '.')
        .take(16)
        .collect();
    (sanitize_segment(stem_raw), ext)
}

/// 生成 object key：`<company_id>/<namespace>/<YYYY>/<MM>/<DD>/<uuid>-<stem><ext>`。
pub fn build_object_key(company_id: Uuid, namespace: &str, filename: Option<&str>) -> String {
    let ns = sanitize_namespace(namespace);
    let now = Utc::now();
    let (stem, ext) = split_filename(filename);
    format!(
        "{}/{}/{:04}/{:02}/{:02}/{}-{}{}",
        company_id,
        ns,
        now.year(),
        now.month(),
        now.day(),
        Uuid::new_v4(),
        stem,
        ext
    )
}

/// 校验 object key 归属指定公司且不含穿越构造，等价 Paperclip `ensureCompanyPrefix`。
pub fn object_key_belongs_to_company(object_key: &str, company_id: Uuid) -> bool {
    if object_key.contains("..") || object_key.starts_with('/') || object_key.contains('\0') {
        return false;
    }
    object_key.starts_with(&format!("{}/", company_id))
}

/// 本地文件系统实现。
pub struct LocalStorageService {
    root: PathBuf,
}

impl LocalStorageService {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn from_env() -> Self {
        Self::new(storage_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, object_key: &str) -> ServiceResult<PathBuf> {
        if object_key.trim().is_empty()
            || object_key.contains("..")
            || object_key.starts_with('/')
            || object_key.contains('\0')
        {
            return Err(ServiceError::Validation(format!(
                "invalid object key: {}",
                object_key
            )));
        }
        Ok(self.root.join(object_key))
    }
}

impl Default for LocalStorageService {
    fn default() -> Self {
        Self::from_env()
    }
}

#[async_trait]
impl StorageService for LocalStorageService {
    async fn put_file(&self, req: PutFileRequest) -> ServiceResult<StoredObject> {
        if req.body.is_empty() {
            return Err(ServiceError::Unprocessable("File is empty".into()));
        }
        if req.content_type.trim().is_empty() {
            return Err(ServiceError::Unprocessable("contentType is required".into()));
        }
        let object_key = build_object_key(
            req.company_id,
            &req.namespace,
            req.original_filename.as_deref(),
        );
        let path = self.resolve(&object_key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ServiceError::Internal(format!("failed to create storage dir: {e}")))?;
        }
        tokio::fs::write(&path, &req.body)
            .await
            .map_err(|e| ServiceError::Internal(format!("failed to write object: {e}")))?;

        let mut hasher = Sha256::new();
        hasher.update(&req.body);
        Ok(StoredObject {
            provider: "local".to_string(),
            object_key,
            content_type: req.content_type,
            byte_size: req.body.len() as i64,
            sha256: format!("{:x}", hasher.finalize()),
            original_filename: req.original_filename,
        })
    }

    async fn get_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<Vec<u8>> {
        if !object_key_belongs_to_company(object_key, company_id) {
            return Err(ServiceError::NotFound("object not found".into()));
        }
        let path = self.resolve(object_key)?;
        match tokio::fs::read(&path).await {
            Ok(data) => Ok(data),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ServiceError::NotFound("object not found in storage".into()))
            }
            Err(e) => Err(ServiceError::Internal(e.to_string())),
        }
    }

    async fn delete_object(&self, company_id: Uuid, object_key: &str) -> ServiceResult<()> {
        if !object_key_belongs_to_company(object_key, company_id) {
            return Ok(());
        }
        let path = self.resolve(object_key)?;
        // 幂等：文件已不存在也视为成功
        let _ = tokio::fs::remove_file(&path).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_sanitization_blocks_traversal() {
        assert_eq!(sanitize_segment("../../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize_segment("a/b/c"), "a_b_c");
        assert_eq!(sanitize_segment("   "), "file");
        assert_eq!(sanitize_segment(""), "file");
        assert_eq!(sanitize_segment("normal-name_1"), "normal-name_1");
        assert_eq!(sanitize_segment("weird  name!!"), "weird_name");
        assert!(sanitize_segment(&"x".repeat(500)).chars().count() <= MAX_SEGMENT_LENGTH);
    }

    #[test]
    fn filename_split_matches_paperclip() {
        assert_eq!(
            split_filename(Some("../../etc/pass wd.PNG")),
            ("pass_wd".to_string(), ".png".to_string())
        );
        assert_eq!(
            split_filename(Some("report.final.docx")),
            ("report.final".to_string(), ".docx".to_string())
        );
        // dotfile 不算扩展名
        assert_eq!(
            split_filename(Some(".gitignore")),
            ("gitignore".to_string(), String::new())
        );
        assert_eq!(split_filename(None), ("file".to_string(), String::new()));
        assert_eq!(split_filename(Some("   ")), ("file".to_string(), String::new()));
    }

    #[test]
    fn namespace_sanitization() {
        assert_eq!(sanitize_namespace("assets/general"), "assets/general");
        assert_eq!(sanitize_namespace("assets/../etc"), "assets/etc");
        assert_eq!(sanitize_namespace(""), "misc");
        assert_eq!(sanitize_namespace("///"), "misc");
        assert_eq!(sanitize_namespace("issues/ABC-123"), "issues/ABC-123");
    }

    #[test]
    fn object_key_is_company_prefixed() {
        let company = Uuid::new_v4();
        let key = build_object_key(company, "assets/general", Some("../x.png"));
        assert!(key.starts_with(&format!("{}/assets/general/", company)));
        assert!(!key.contains(".."));
        assert!(key.ends_with("-x.png"));
        // company / ns(2) / y / m / d / filename
        assert_eq!(key.split('/').count(), 7);
    }

    #[test]
    fn company_scope_check() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let key = build_object_key(a, "assets/general", Some("x.png"));
        assert!(object_key_belongs_to_company(&key, a));
        assert!(!object_key_belongs_to_company(&key, b));
        assert!(!object_key_belongs_to_company("../escape", a));
        assert!(!object_key_belongs_to_company("/abs/path", a));
        assert!(!object_key_belongs_to_company(
            &format!("{}/../{}/x.png", a, b),
            a
        ));
    }

    #[tokio::test]
    async fn put_file_rejects_empty_body() {
        let dir = std::env::temp_dir().join(format!("parrot-storage-{}", Uuid::new_v4()));
        let svc = LocalStorageService::new(dir.clone());
        let err = svc
            .put_file(PutFileRequest {
                company_id: Uuid::new_v4(),
                namespace: "assets/general".into(),
                original_filename: Some("empty.png".into()),
                content_type: "image/png".into(),
                body: Vec::new(),
            })
            .await
            .expect_err("empty body must be rejected");
        assert!(matches!(err, ServiceError::Unprocessable(_)));
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[test]
    fn resolve_rejects_dangerous_keys() {
        let svc = LocalStorageService::new(PathBuf::from("/tmp/parrot-test-root"));
        assert!(svc.resolve("a/b.png").is_ok());
        assert!(svc.resolve("../escape").is_err());
        assert!(svc.resolve("/abs").is_err());
        assert!(svc.resolve("  ").is_err());
    }

    #[tokio::test]
    async fn put_get_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("parrot-storage-{}", Uuid::new_v4()));
        let svc = LocalStorageService::new(dir.clone());
        let company = Uuid::new_v4();

        let stored = svc
            .put_file(PutFileRequest {
                company_id: company,
                namespace: "assets/general".into(),
                original_filename: Some("hello.txt".into()),
                content_type: "text/plain".into(),
                body: b"hello".to_vec(),
            })
            .await
            .expect("put_file");
        assert_eq!(stored.provider, "local");
        assert_eq!(stored.byte_size, 5);
        assert_eq!(
            stored.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );

        let data = svc.get_object(company, &stored.object_key).await.expect("get_object");
        assert_eq!(data, b"hello");

        // 跨公司读取按 404 处理
        assert!(svc.get_object(Uuid::new_v4(), &stored.object_key).await.is_err());

        svc.delete_object(company, &stored.object_key).await.expect("delete");
        assert!(svc.get_object(company, &stored.object_key).await.is_err());
        // 重复删除保持幂等
        svc.delete_object(company, &stored.object_key).await.expect("idempotent delete");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
