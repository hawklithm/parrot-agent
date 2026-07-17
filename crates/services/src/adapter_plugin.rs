//! Adapter 插件系统
//!
//! 提供外部 adapter 插件的安装、加载和持久化管理。
//! 对应 pipeline-adapter-tasks.md §4 Adapter 插件系统。
//!
//! 插件通过 npm 包分发（`load_from_npm`），也支持从本地路径加载
//! （`load_from_path`）。已安装的插件记录持久化到 JSON 文件。
//!
//! # 与 Paperclip 对齐
//!
//! 本模块的 `resolve_package_dir` 对应 Paperclip 的插件目录解析逻辑：
//! - 如果插件有 `local_path`（本地加载），直接返回该路径
//! - 否则按 `{plugins_dir}/node_modules/{package_name}` 拼接
//!
//! `load_from_npm` 通过真实 npm 进程安装包，而非模拟。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

// ============================================================================
// 错误类型
// ============================================================================

/// Adapter 插件系统错误
#[derive(Debug, Error)]
pub enum AdapterPluginError {
    #[error("Npm install failed: {0}")]
    NpmInstallFailed(String),

    #[error("Package not found at {0}")]
    PackageNotFound(PathBuf),

    #[error("Invalid package.json: {0}")]
    InvalidPackageJson(String),

    #[error("Plugin already installed: {0}")]
    AlreadyInstalled(String),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Adapter 插件系统结果类型
pub type AdapterPluginResult<T> = Result<T, AdapterPluginError>;

// ============================================================================
// 核心类型
// ============================================================================

/// Adapter 插件记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPluginRecord {
    /// npm 包名
    pub package_name: String,
    /// 本地文件系统路径（本地链接的 adapter）
    pub local_path: Option<String>,
    /// 已安装版本
    pub version: Option<String>,
    /// Adapter 类型标识
    pub adapter_type: String,
    /// 安装时间
    pub installed_at: String,
    /// 是否禁用
    pub disabled: Option<bool>,
    /// 插件入口文件（从 package.json 的 main 字段读取）
    pub entry_point: Option<String>,
}

/// Adapter 安装请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInstallRequest {
    pub package_name: String,
    pub is_local_path: bool,
    pub version: Option<String>,
}

/// 技能条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSkillEntry {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub content: String,
}

/// 模型 Profile 键
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProfileKey {
    Default,
    Fast,
    Balanced,
    Deep,
}

/// 模型 Profile 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterModelProfileDefinition {
    pub key: ModelProfileKey,
    pub label: String,
    pub config_overrides: serde_json::Value,
}

/// 模型 Profile 应用结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfileApplication {
    pub requested: ModelProfileKey,
    pub requested_by: ModelProfileRequestSource,
    pub applied: ModelProfileKey,
    pub config_source: AppliedModelProfileConfigSource,
    pub fallback_reason: Option<String>,
    pub adapter_config: serde_json::Value,
}

/// Profile 请求来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProfileRequestSource {
    IssueOverride,
    WakeContext,
}

/// 应用配置来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliedModelProfileConfigSource {
    AgentRuntime,
    AdapterDefault,
}

// ============================================================================
// 插件加载器 trait
// ============================================================================

/// Adapter 插件加载器
#[async_trait]
pub trait AdapterPluginLoader: Send + Sync {
    /// 从本地路径加载 adapter
    async fn load_from_path(&self, path: &str) -> AdapterPluginResult<AdapterPluginRecord>;

    /// 从 npm 包安装 adapter
    async fn load_from_npm(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> AdapterPluginResult<AdapterPluginRecord>;

    /// 获取所有已安装插件
    async fn list_plugins(&self) -> Vec<AdapterPluginRecord>;

    /// 卸载插件
    async fn uninstall(&self, adapter_type: &str) -> AdapterPluginResult<()>;

    /// 解析插件包在文件系统中的目录路径。
    ///
    /// 如果记录包含 `local_path`，直接返回该路径；
    /// 否则按 `{plugins_dir}/node_modules/{package_name}` 拼接。
    fn resolve_package_dir(&self, record: &AdapterPluginRecord) -> PathBuf;
}

// ============================================================================
// 插件存储
// ============================================================================

/// Adapter 插件存储（JSON 文件持久化）
pub struct AdapterPluginStore {
    plugins: RwLock<Vec<AdapterPluginRecord>>,
    store_path: PathBuf,
    plugins_dir: PathBuf,
}

impl AdapterPluginStore {
    /// 创建插件存储，从 `paperclip_home` 目录加载已有记录。
    ///
    /// 数据文件位于 `{paperclip_home}/adapter-plugins.json`，
    /// 插件安装目录为 `{paperclip_home}/adapter-plugins`。
    pub fn new(paperclip_home: &Path) -> Self {
        let store_path = paperclip_home.join("adapter-plugins.json");
        let plugins_dir = paperclip_home.join("adapter-plugins");

        let records = if store_path.exists() {
            std::fs::read_to_string(&store_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            plugins: RwLock::new(records),
            store_path,
            plugins_dir,
        }
    }

    /// 获取插件安装根目录
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// 添加插件记录。如果已存在同类型的插件，返回 `AlreadyInstalled` 错误。
    pub async fn add_plugin(
        &self,
        record: AdapterPluginRecord,
    ) -> AdapterPluginResult<()> {
        let mut plugins = self.plugins.write().await;
        if plugins.iter().any(|p| p.adapter_type == record.adapter_type) {
            return Err(AdapterPluginError::AlreadyInstalled(record.adapter_type));
        }
        plugins.push(record);
        self.save_to_disk(&plugins).await
    }

    /// 移除插件。如果找不到对应类型，返回 `NotFound` 错误。
    pub async fn remove_plugin(&self, adapter_type: &str) -> AdapterPluginResult<()> {
        let mut plugins = self.plugins.write().await;
        let initial_len = plugins.len();
        plugins.retain(|p| p.adapter_type != adapter_type);
        if plugins.len() == initial_len {
            return Err(AdapterPluginError::NotFound(adapter_type.to_string()));
        }
        self.save_to_disk(&plugins).await
    }

    /// 获取所有已安装插件
    pub async fn list_plugins(&self) -> Vec<AdapterPluginRecord> {
        self.plugins.read().await.clone()
    }

    /// 按类型获取插件
    pub async fn get_by_type(&self, adapter_type: &str) -> Option<AdapterPluginRecord> {
        self.plugins.read().await.iter()
            .find(|p| p.adapter_type == adapter_type)
            .cloned()
    }

    /// 保存到磁盘
    async fn save_to_disk(&self, plugins: &[AdapterPluginRecord]) -> AdapterPluginResult<()> {
        let json = serde_json::to_string_pretty(plugins)
            .map_err(|e| AdapterPluginError::InvalidPackageJson(e.to_string()))?;
        std::fs::write(&self.store_path, json)?;
        Ok(())
    }
}

// ============================================================================
// 默认插件加载器实现
// ============================================================================

/// 默认的 Adapter 插件加载器，支持本地路径和 npm 安装。
pub struct DefaultAdapterPluginLoader {
    store: Arc<AdapterPluginStore>,
}

impl DefaultAdapterPluginLoader {
    pub fn new(store: Arc<AdapterPluginStore>) -> Self {
        Self { store }
    }

    /// 获取插件目录
    fn get_plugins_dir(&self) -> PathBuf {
        self.store.plugins_dir().to_path_buf()
    }
}

#[async_trait]
impl AdapterPluginLoader for DefaultAdapterPluginLoader {
    fn resolve_package_dir(&self, record: &AdapterPluginRecord) -> PathBuf {
        if let Some(local_path) = &record.local_path {
            PathBuf::from(local_path)
        } else {
            self.get_plugins_dir()
                .join("node_modules")
                .join(&record.package_name)
        }
    }

    async fn load_from_path(&self, path: &str) -> AdapterPluginResult<AdapterPluginRecord> {
        let resolved_path = std::path::absolute(path)
            .map_err(|_| AdapterPluginError::PackageNotFound(PathBuf::from(path)))?;

        if !resolved_path.exists() {
            return Err(AdapterPluginError::PackageNotFound(resolved_path));
        }

        // 读取 package.json 获取包名和入口文件
        let pkg_json_path = resolved_path.join("package.json");
        let (package_name, entry_point) = if pkg_json_path.exists() {
            let content = std::fs::read_to_string(&pkg_json_path)?;
            let pkg: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| AdapterPluginError::InvalidPackageJson(e.to_string()))?;

            let name = pkg
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    resolved_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| Uuid::new_v4().to_string())
                });

            let entry = pkg
                .get("main")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            (name, entry)
        } else {
            let name = resolved_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            (name, None)
        };

        let adapter_type = package_name.clone();

        let record = AdapterPluginRecord {
            package_name,
            local_path: Some(resolved_path.to_string_lossy().to_string()),
            version: None,
            adapter_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point,
        };

        self.store.add_plugin(record.clone()).await?;
        Ok(record)
    }

    async fn load_from_npm(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> AdapterPluginResult<AdapterPluginRecord> {
        let plugins_dir = self.get_plugins_dir();
        std::fs::create_dir_all(&plugins_dir)?;

        let install_dir = plugins_dir.join("node_modules").join(package_name);
        let version_str = version.unwrap_or("latest");

        // 检查是否已安装
        if install_dir.join("package.json").exists() {
            return Err(AdapterPluginError::AlreadyInstalled(
                package_name.to_string(),
            ));
        }

        // 执行真实的 npm install
        let mut cmd = tokio::process::Command::new("npm");
        cmd.arg("install")
            .arg(format!("{}@{}", package_name, version_str))
            .current_dir(&plugins_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            AdapterPluginError::NpmInstallFailed(format!("Failed to spawn npm: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AdapterPluginError::NpmInstallFailed(format!(
                "npm install failed: {}",
                stderr
            )));
        }

        // 安装成功后从 package.json 读取包信息
        let pkg_content = std::fs::read_to_string(install_dir.join("package.json"))?;
        let pkg: serde_json::Value = serde_json::from_str(&pkg_content)
            .map_err(|e| AdapterPluginError::InvalidPackageJson(e.to_string()))?;

        let actual_package_name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(package_name)
            .to_string();

        let entry_point = pkg
            .get("main")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let record = AdapterPluginRecord {
            package_name: actual_package_name,
            local_path: None,
            version: Some(version_str.to_string()),
            adapter_type: package_name.to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point,
        };

        self.store.add_plugin(record.clone()).await?;
        Ok(record)
    }

    async fn list_plugins(&self) -> Vec<AdapterPluginRecord> {
        self.store.list_plugins().await
    }

    async fn uninstall(&self, adapter_type: &str) -> AdapterPluginResult<()> {
        // 获取插件记录以知道包名
        let record = self
            .store
            .get_by_type(adapter_type)
            .await
            .ok_or_else(|| AdapterPluginError::NotFound(adapter_type.to_string()))?;

        // 从存储中移除记录
        self.store.remove_plugin(adapter_type).await?;

        // 如果插件是通过 npm 安装的（没有 local_path），尝试删除安装目录
        if record.local_path.is_none() {
            let install_dir = self.resolve_package_dir(&record);
            if install_dir.exists() {
                std::fs::remove_dir_all(&install_dir).ok();
            }
        }

        Ok(())
    }
}

// ============================================================================
// 模型 Profile 解析
// ============================================================================

/// 解析模型 Profile 应用
pub fn resolve_model_profile_application(
    requested: ModelProfileKey,
    requested_by: ModelProfileRequestSource,
    available_profiles: &[AdapterModelProfileDefinition],
) -> ModelProfileApplication {
    // 查找请求的 profile
    if let Some(profile) = available_profiles.iter().find(|p| p.key == requested) {
        return ModelProfileApplication {
            requested,
            requested_by,
            applied: requested,
            config_source: AppliedModelProfileConfigSource::AgentRuntime,
            fallback_reason: None,
            adapter_config: profile.config_overrides.clone(),
        };
    }

    // Fallback 到 default
    let default_profile = available_profiles
        .iter()
        .find(|p| matches!(p.key, ModelProfileKey::Default));

    if let Some(profile) = default_profile {
        return ModelProfileApplication {
            requested,
            requested_by,
            applied: ModelProfileKey::Default,
            config_source: AppliedModelProfileConfigSource::AdapterDefault,
            fallback_reason: Some(format!(
                "Requested profile '{:?}' not available, falling back to default",
                requested
            )),
            adapter_config: profile.config_overrides.clone(),
        };
    }

    // 没有可用 profile
    ModelProfileApplication {
        requested,
        requested_by,
        applied: ModelProfileKey::Default,
        config_source: AppliedModelProfileConfigSource::AdapterDefault,
        fallback_reason: Some("No profiles available".to_string()),
        adapter_config: serde_json::Value::Object(serde_json::Map::new()),
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== AdapterPluginStore ====================

    #[test]
    fn test_store_new_creates_empty() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = AdapterPluginStore::new(&dir);
        let plugins = store.plugins.try_read().unwrap();
        assert!(plugins.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_store_new_loads_existing() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        // 先写一个文件
        let store_path = dir.join("adapter-plugins.json");
        let records = vec![AdapterPluginRecord {
            package_name: "test-pkg".to_string(),
            local_path: None,
            version: Some("1.0.0".to_string()),
            adapter_type: "test".to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point: None,
        }];
        std::fs::write(&store_path, serde_json::to_string_pretty(&records).unwrap()).unwrap();

        // 加载应能读取已有记录
        let store = AdapterPluginStore::new(&dir);
        let plugins = store.plugins.try_read().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].package_name, "test-pkg");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_store_add_plugin() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = AdapterPluginStore::new(&dir);
        let record = AdapterPluginRecord {
            package_name: "pkg-a".to_string(),
            local_path: None,
            version: None,
            adapter_type: "type-a".to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point: None,
        };

        assert!(store.add_plugin(record.clone()).await.is_ok());

        // 重复添加应拒绝
        let result = store.add_plugin(record).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AdapterPluginError::AlreadyInstalled(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_store_remove_plugin() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = AdapterPluginStore::new(&dir);
        let record = AdapterPluginRecord {
            package_name: "pkg-b".to_string(),
            local_path: None,
            version: None,
            adapter_type: "type-b".to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point: None,
        };

        store.add_plugin(record).await.unwrap();
        assert_eq!(store.list_plugins().await.len(), 1);

        store.remove_plugin("type-b").await.unwrap();
        assert!(store.list_plugins().await.is_empty());

        // 移除不存在的应报错
        let result = store.remove_plugin("nonexistent").await;
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_store_get_by_type() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = AdapterPluginStore::new(&dir);
        let record = AdapterPluginRecord {
            package_name: "pkg-c".to_string(),
            local_path: None,
            version: Some("2.0.0".to_string()),
            adapter_type: "type-c".to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
            entry_point: None,
        };

        store.add_plugin(record).await.unwrap();

        let found = store.get_by_type("type-c").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().version.unwrap(), "2.0.0");

        assert!(store.get_by_type("nonexistent").await.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_store_persists_to_disk() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        // 添加插件到 store
        {
            let store = AdapterPluginStore::new(&dir);
            let record = AdapterPluginRecord {
                package_name: "persist-pkg".to_string(),
                local_path: None,
                version: Some("1.0.0".to_string()),
                adapter_type: "persist".to_string(),
                installed_at: "2024-01-01T00:00:00Z".to_string(),
                disabled: Some(false),
                entry_point: None,
            };
            store.add_plugin(record).await.unwrap();
        } // store 被 drop，文件应已写入

        // 新 store 实例应能读取
        let store2 = AdapterPluginStore::new(&dir);
        let plugins = store2.list_plugins().await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].package_name, "persist-pkg");

        std::fs::remove_dir_all(&dir).ok();
    }

    // ==================== resolve_package_dir ====================

    #[test]
    fn test_resolve_package_dir_local_path() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = Arc::new(AdapterPluginStore::new(&dir));
        let loader = DefaultAdapterPluginLoader::new(store);

        let record = AdapterPluginRecord {
            package_name: "test-pkg".to_string(),
            local_path: Some("/custom/path".to_string()),
            version: None,
            adapter_type: "test".to_string(),
            installed_at: String::new(),
            disabled: Some(false),
            entry_point: None,
        };

        let resolved = loader.resolve_package_dir(&record);
        assert_eq!(resolved, PathBuf::from("/custom/path"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_resolve_package_dir_npm_path() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).unwrap();

        let store = Arc::new(AdapterPluginStore::new(&dir));
        let loader = DefaultAdapterPluginLoader::new(store);

        let record = AdapterPluginRecord {
            package_name: "@scope/my-adapter".to_string(),
            local_path: None,
            version: Some("1.0.0".to_string()),
            adapter_type: "my-adapter".to_string(),
            installed_at: String::new(),
            disabled: Some(false),
            entry_point: None,
        };

        let resolved = loader.resolve_package_dir(&record);
        let expected = dir.join("adapter-plugins").join("node_modules").join("@scope/my-adapter");
        assert_eq!(resolved, expected);

        std::fs::remove_dir_all(&dir).ok();
    }

    // ==================== load_from_path ====================

    #[tokio::test]
    async fn test_load_from_path_with_package_json() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let plugin_dir = dir.join("my-adapter");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // 创建 package.json
        let pkg = serde_json::json!({
            "name": "my-adapter",
            "version": "1.0.0",
            "main": "dist/index.js"
        });
        std::fs::write(
            plugin_dir.join("package.json"),
            serde_json::to_string_pretty(&pkg).unwrap(),
        )
        .unwrap();

        let store_path = dir.join("store");
        std::fs::create_dir_all(&store_path).unwrap();

        let store = Arc::new(AdapterPluginStore::new(&store_path));
        let loader = DefaultAdapterPluginLoader::new(store.clone());

        let result = loader
            .load_from_path(plugin_dir.to_str().unwrap())
            .await;
        assert!(result.is_ok());

        let record = result.unwrap();
        assert_eq!(record.package_name, "my-adapter");
        assert_eq!(record.entry_point.as_deref(), Some("dist/index.js"));
        assert!(record.local_path.is_some());

        // 验证记录被持久化
        let stored = store.get_by_type("my-adapter").await;
        assert!(stored.is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_load_from_path_without_package_json() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let plugin_dir = dir.join("bare-adapter");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let store_path = dir.join("store");
        std::fs::create_dir_all(&store_path).unwrap();

        let store = Arc::new(AdapterPluginStore::new(&store_path));
        let loader = DefaultAdapterPluginLoader::new(store);

        // 没有 package.json，应该用目录名作为包名
        let result = loader
            .load_from_path(plugin_dir.to_str().unwrap())
            .await;
        assert!(result.is_ok());

        let record = result.unwrap();
        assert_eq!(record.package_name, "bare-adapter");
        assert!(record.entry_point.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_load_from_path_nonexistent() {
        let dir = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let store_path = dir.join("store");
        std::fs::create_dir_all(&store_path).unwrap();

        let store = Arc::new(AdapterPluginStore::new(&store_path));
        let loader = DefaultAdapterPluginLoader::new(store);

        let result = loader.load_from_path("/nonexistent/path").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AdapterPluginError::PackageNotFound(_)
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    // ==================== resolve_model_profile_application ====================

    #[test]
    fn test_resolve_model_profile_exact_match() {
        let profiles = vec![
            AdapterModelProfileDefinition {
                key: ModelProfileKey::Default,
                label: "Default".to_string(),
                config_overrides: serde_json::json!({}),
            },
            AdapterModelProfileDefinition {
                key: ModelProfileKey::Fast,
                label: "Fast".to_string(),
                config_overrides: serde_json::json!({ "model": "fast-model" }),
            },
        ];

        let result = resolve_model_profile_application(
            ModelProfileKey::Fast,
            ModelProfileRequestSource::IssueOverride,
            &profiles,
        );

        assert_eq!(result.applied, ModelProfileKey::Fast);
        assert!(result.fallback_reason.is_none());
        assert_eq!(
            result.adapter_config.get("model").and_then(|v| v.as_str()),
            Some("fast-model")
        );
    }

    #[test]
    fn test_resolve_model_profile_fallback_to_default() {
        let profiles = vec![AdapterModelProfileDefinition {
            key: ModelProfileKey::Default,
            label: "Default".to_string(),
            config_overrides: serde_json::json!({ "model": "default-model" }),
        }];

        let result = resolve_model_profile_application(
            ModelProfileKey::Deep,
            ModelProfileRequestSource::WakeContext,
            &profiles,
        );

        assert_eq!(result.applied, ModelProfileKey::Default);
        assert!(result.fallback_reason.is_some());
    }

    #[test]
    fn test_resolve_model_profile_no_profiles() {
        let profiles: Vec<AdapterModelProfileDefinition> = vec![];

        let result = resolve_model_profile_application(
            ModelProfileKey::Balanced,
            ModelProfileRequestSource::IssueOverride,
            &profiles,
        );

        assert_eq!(result.applied, ModelProfileKey::Default);
        assert!(result.fallback_reason.is_some());
        assert!(result
            .fallback_reason
            .as_ref()
            .unwrap()
            .contains("No profiles"));
    }
}
