use crate::adapter_plugin_store::AdapterPluginRecord;
use crate::npm_manager::{NpmManager, NpmResult};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterLoadError {
    #[error("npm error: {0}")]
    Npm(#[from] crate::npm_manager::NpmError),
    
    #[error("adapter type not found in package")]
    AdapterTypeNotFound,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
    
    #[error("adapter validation failed: {0}")]
    ValidationFailed(String),
}

pub type LoadResult<T> = Result<T, AdapterLoadError>;

/// 适配器包加载器
/// 
/// 负责：
/// - 安装 npm 包或加载本地路径
/// - 验证适配器包结构
/// - 提取适配器元数据
/// - 创建插件记录
pub struct AdapterPackageLoader {
    npm_manager: NpmManager,
}

impl AdapterPackageLoader {
    /// 创建新的加载器
    pub fn new(npm_manager: NpmManager) -> Self {
        Self { npm_manager }
    }
    
    /// 使用默认配置创建
    pub fn with_default_config() -> Self {
        let npm_manager = NpmManager::new(NpmManager::default_plugins_dir());
        Self::new(npm_manager)
    }
    
    /// 安装 npm 适配器包
    /// 
    /// # Arguments
    /// * `package_name` - npm 包名
    /// * `version` - 可选的版本号
    /// 
    /// # Returns
    /// 安装成功后的插件记录
    pub fn install_npm_adapter(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> LoadResult<AdapterPluginRecord> {
        // 1. 安装 npm 包
        let installed_version = self.npm_manager.install_package(package_name, version)?;
        
        // 2. 读取适配器元数据
        let package_path = self.npm_manager.get_package_path(package_name);
        let adapter_type = self.extract_adapter_type(&package_path)?;
        
        // 3. 创建插件记录
        let record = AdapterPluginRecord {
            package_name: package_name.to_string(),
            local_path: None,
            version: Some(installed_version),
            adapter_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
        };
        
        Ok(record)
    }
    
    /// 加载本地路径的适配器包
    /// 
    /// # Arguments
    /// * `local_path` - 本地文件系统路径
    /// 
    /// # Returns
    /// 加载成功后的插件记录
    pub fn load_local_adapter(&self, local_path: impl AsRef<Path>) -> LoadResult<AdapterPluginRecord> {
        let local_path = local_path.as_ref();
        
        // 1. 验证路径存在
        if !local_path.exists() {
            return Err(AdapterLoadError::ValidationFailed(format!(
                "Local path does not exist: {}",
                local_path.display()
            )));
        }
        
        // 2. 读取 package.json
        let pkg_json_path = local_path.join("package.json");
        if !pkg_json_path.exists() {
            return Err(AdapterLoadError::ValidationFailed(
                "package.json not found in local path".to_string(),
            ));
        }
        
        let content = std::fs::read_to_string(&pkg_json_path)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;
        
        // 3. 提取元数据
        let package_name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AdapterLoadError::ValidationFailed("package name missing".to_string()))?
            .to_string();
        
        let version = NpmManager::read_local_package_version(local_path)?;
        let adapter_type = self.extract_adapter_type(local_path)?;
        
        // 4. 创建插件记录
        let record = AdapterPluginRecord {
            package_name,
            local_path: Some(local_path.to_string_lossy().to_string()),
            version,
            adapter_type,
            installed_at: chrono::Utc::now().to_rfc3339(),
        };
        
        Ok(record)
    }
    
    /// 从适配器包中提取 adapter_type
    /// 
    /// 策略：
    /// 1. 读取 package.json 的 "paperclip"/"parrot" 字段
    /// 2. 或者从包名推断（如 "droid-paperclip-adapter" -> "droid"）
    fn extract_adapter_type(&self, package_path: &Path) -> LoadResult<String> {
        let pkg_json_path = package_path.join("package.json");
        let content = std::fs::read_to_string(&pkg_json_path)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;
        
        // 策略1: 检查 "paperclip.adapterType" 或 "parrot.adapterType"
        if let Some(adapter_type) = pkg
            .get("paperclip")
            .or_else(|| pkg.get("parrot"))
            .and_then(|v| v.get("adapterType"))
            .and_then(|v| v.as_str())
        {
            return Ok(adapter_type.to_string());
        }
        
        // 策略2: 从包名推断
        // "droid-paperclip-adapter" -> "droid"
        // "@scope/name-adapter" -> "name"
        if let Some(name) = pkg.get("name").and_then(|v| v.as_str()) {
            let base_name = name
                .rsplit('/')
                .next()
                .unwrap_or(name)
                .trim_end_matches("-paperclip-adapter")
                .trim_end_matches("-parrot-adapter")
                .trim_end_matches("-adapter");
            
            if !base_name.is_empty() {
                return Ok(base_name.to_string());
            }
        }
        
        Err(AdapterLoadError::AdapterTypeNotFound)
    }
    
    /// 卸载适配器包
    pub fn uninstall_adapter(&self, package_name: &str) -> NpmResult<()> {
        self.npm_manager.uninstall_package(package_name)
    }
    
    /// 获取包的解析路径
    pub fn resolve_package_path(&self, record: &AdapterPluginRecord) -> PathBuf {
        if let Some(local_path) = &record.local_path {
            PathBuf::from(local_path)
        } else {
            self.npm_manager.get_package_path(&record.package_name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_extract_adapter_type_from_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_path = temp_dir.path();
        
        // 创建 package.json with paperclip.adapterType
        let pkg_json = serde_json::json!({
            "name": "test-adapter",
            "version": "1.0.0",
            "paperclip": {
                "adapterType": "test_adapter"
            }
        });
        
        fs::write(
            pkg_path.join("package.json"),
            serde_json::to_string_pretty(&pkg_json).unwrap(),
        )
        .unwrap();
        
        let npm_manager = NpmManager::new(temp_dir.path());
        let loader = AdapterPackageLoader::new(npm_manager);
        
        let adapter_type = loader.extract_adapter_type(pkg_path).unwrap();
        assert_eq!(adapter_type, "test_adapter");
    }
    
    #[test]
    fn test_extract_adapter_type_from_name() {
        let temp_dir = TempDir::new().unwrap();
        let pkg_path = temp_dir.path();
        
        // 创建 package.json without metadata
        let pkg_json = serde_json::json!({
            "name": "droid-paperclip-adapter",
            "version": "1.0.0"
        });
        
        fs::write(
            pkg_path.join("package.json"),
            serde_json::to_string_pretty(&pkg_json).unwrap(),
        )
        .unwrap();
        
        let npm_manager = NpmManager::new(temp_dir.path());
        let loader = AdapterPackageLoader::new(npm_manager);
        
        let adapter_type = loader.extract_adapter_type(pkg_path).unwrap();
        assert_eq!(adapter_type, "droid");
    }
}
