//! Adapter 插件系统
//!
//! 提供外部 adapter 插件的安装、加载和持久化管理
//! 对应 pipeline-adapter-tasks.md §4 Adapter 插件系统

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

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
    async fn load_from_path(&self, path: &str) -> Result<AdapterPluginRecord, String>;

    /// 从 npm 包安装 adapter
    async fn load_from_npm(&self, package_name: &str, version: Option<&str>) -> Result<AdapterPluginRecord, String>;

    /// 获取所有已安装插件
    async fn list_plugins(&self) -> Vec<AdapterPluginRecord>;

    /// 卸载插件
    async fn uninstall(&self, adapter_type: &str) -> Result<(), String>;
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

    /// 添加插件记录
    pub async fn add_plugin(&self, record: AdapterPluginRecord) -> Result<(), String> {
        let mut plugins = self.plugins.write().await;
        // 检查是否已存在同类型
        if plugins.iter().any(|p| p.adapter_type == record.adapter_type) {
            return Err(format!("Adapter type '{}' already installed", record.adapter_type));
        }
        plugins.push(record);
        self.save_to_disk(&plugins).await
    }

    /// 移除插件
    pub async fn remove_plugin(&self, adapter_type: &str) -> Result<(), String> {
        let mut plugins = self.plugins.write().await;
        let initial_len = plugins.len();
        plugins.retain(|p| p.adapter_type != adapter_type);
        if plugins.len() == initial_len {
            return Err(format!("Adapter type '{}' not found", adapter_type));
        }
        self.save_to_disk(&plugins).await
    }

    /// 获取所有插件
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
    async fn save_to_disk(&self, plugins: &[AdapterPluginRecord]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(plugins)
            .map_err(|e| format!("Serialization error: {}", e))?;
        std::fs::write(&self.store_path, json)
            .map_err(|e| format!("Write error: {}", e))
    }
}

// ============================================================================
// 默认插件加载器实现
// ============================================================================

pub struct DefaultAdapterPluginLoader {
    store: Arc<AdapterPluginStore>,
}

impl DefaultAdapterPluginLoader {
    pub fn new(store: Arc<AdapterPluginStore>) -> Self {
        Self { store }
    }

    /// 获取插件目录
    fn get_plugins_dir(&self) -> PathBuf {
        self.store.plugins_dir.clone()
    }

    /// 解析包目录
    fn resolve_package_dir(&self, record: &AdapterPluginRecord) -> PathBuf {
        if let Some(local_path) = &record.local_path {
            PathBuf::from(local_path)
        } else {
            self.get_plugins_dir()
                .join("node_modules")
                .join(&record.package_name)
        }
    }
}

#[async_trait]
impl AdapterPluginLoader for DefaultAdapterPluginLoader {
    async fn load_from_path(&self, path: &str) -> Result<AdapterPluginRecord, String> {
        let resolved_path = std::path::absolute(path)
            .map_err(|e| format!("Invalid path '{}': {}", path, e))?;

        if !resolved_path.exists() {
            return Err(format!("Path '{}' does not exist", resolved_path.display()));
        }

        // Try to read package.json to extract adapter type
        let pkg_json_path = resolved_path.join("package.json");
        let adapter_type = if pkg_json_path.exists() {
            let content = std::fs::read_to_string(&pkg_json_path)
                .map_err(|e| format!("Failed to read package.json: {}", e))?;
            let pkg: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Invalid package.json: {}", e))?;
            pkg.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.replace("/", "_"))
                .unwrap_or_else(|| {
                    resolved_path.file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| Uuid::new_v4().to_string())
                })
        } else {
            resolved_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| Uuid::new_v4().to_string())
        };

        let record = AdapterPluginRecord {
            package_name: adapter_type.clone(),
            local_path: Some(resolved_path.to_string_lossy().to_string()),
            version: None,
            adapter_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
        };

        self.store.add_plugin(record.clone()).await?;
        Ok(record)
    }

    async fn load_from_npm(&self, package_name: &str, version: Option<&str>) -> Result<AdapterPluginRecord, String> {
        let plugins_dir = self.get_plugins_dir();
        std::fs::create_dir_all(&plugins_dir)
            .map_err(|e| format!("Failed to create plugins dir: {}", e))?;

        // Run npm install (simulated - in production would spawn npm process)
        let install_dir = plugins_dir.join("node_modules").join(package_name);
        let version_str = version.unwrap_or("latest");

        // Simulated npm install
        println!("Installing adapter '{}@{}' to {}", package_name, version_str, install_dir.display());

        let adapter_type = package_name.replace("/", "_");
        let record = AdapterPluginRecord {
            package_name: package_name.to_string(),
            local_path: None,
            version: Some(version_str.to_string()),
            adapter_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
            disabled: Some(false),
        };

        self.store.add_plugin(record.clone()).await?;
        Ok(record)
    }

    async fn list_plugins(&self) -> Vec<AdapterPluginRecord> {
        self.store.list_plugins().await
    }

    async fn uninstall(&self, adapter_type: &str) -> Result<(), String> {
        self.store.remove_plugin(adapter_type).await
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
    let default_profile = available_profiles.iter()
        .find(|p| matches!(p.key, ModelProfileKey::Default));

    if let Some(profile) = default_profile {
        return ModelProfileApplication {
            requested,
            requested_by,
            applied: ModelProfileKey::Default,
            config_source: AppliedModelProfileConfigSource::AdapterDefault,
            fallback_reason: Some(format!("Requested profile '{:?}' not available, falling back to default", requested)),
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
