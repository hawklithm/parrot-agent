use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NpmError {
    #[error("npm command failed: {0}")]
    CommandFailed(String),
    
    #[error("package.json not found or invalid: {0}")]
    InvalidPackageJson(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
}

pub type NpmResult<T> = Result<T, NpmError>;

/// npm 包管理器
pub struct NpmManager {
    /// 插件安装目录（类似 Paperclip 的 data/adapter-plugins）
    plugins_dir: PathBuf,
}

impl NpmManager {
    /// 创建新的 npm 管理器
    pub fn new(plugins_dir: impl AsRef<Path>) -> Self {
        Self {
            plugins_dir: plugins_dir.as_ref().to_path_buf(),
        }
    }
    
    /// 获取默认插件目录
    pub fn default_plugins_dir() -> PathBuf {
        PathBuf::from("data").join("adapter-plugins")
    }
    
    /// 确保插件目录存在
    pub fn ensure_plugins_dir(&self) -> NpmResult<()> {
        std::fs::create_dir_all(&self.plugins_dir)?;
        Ok(())
    }
    
    /// 安装 npm 包
    /// 
    /// # Arguments
    /// * `package_name` - 包名（如 "droid-paperclip-adapter"）
    /// * `version` - 可选的版本号（如 "1.0.0"）
    /// 
    /// # Returns
    /// 安装后的实际版本号
    pub fn install_package(
        &self,
        package_name: &str,
        version: Option<&str>,
    ) -> NpmResult<String> {
        self.ensure_plugins_dir()?;
        
        let spec = if let Some(v) = version {
            format!("{}@{}", package_name, v)
        } else {
            package_name.to_string()
        };
        
        tracing::info!(
            spec = %spec,
            plugins_dir = %self.plugins_dir.display(),
            "Installing adapter package via npm"
        );
        
        let output = Command::new("npm")
            .args(&["install", "--no-save", &spec])
            .current_dir(&self.plugins_dir)
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NpmError::CommandFailed(format!(
                "npm install failed: {}",
                stderr
            )));
        }
        
        // 读取安装后的版本号
        self.read_installed_version(package_name)
    }
    
    /// 读取已安装包的版本号
    pub fn read_installed_version(&self, package_name: &str) -> NpmResult<String> {
        let pkg_json_path = self
            .plugins_dir
            .join("node_modules")
            .join(package_name)
            .join("package.json");
        
        let content = std::fs::read_to_string(&pkg_json_path).map_err(|e| {
            NpmError::InvalidPackageJson(format!("Failed to read package.json: {}", e))
        })?;
        
        let pkg: serde_json::Value = serde_json::from_str(&content)?;
        
        pkg.get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .ok_or_else(|| NpmError::InvalidPackageJson("version field missing".to_string()))
    }
    
    /// 卸载 npm 包
    pub fn uninstall_package(&self, package_name: &str) -> NpmResult<()> {
        tracing::info!(
            package_name = %package_name,
            plugins_dir = %self.plugins_dir.display(),
            "Uninstalling adapter package"
        );
        
        let output = Command::new("npm")
            .args(&["uninstall", package_name])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NpmError::CommandFailed(format!(
                "npm uninstall failed: {}",
                stderr
            )));
        }
        
        Ok(())
    }
    
    /// 获取包的安装路径
    pub fn get_package_path(&self, package_name: &str) -> PathBuf {
        self.plugins_dir.join("node_modules").join(package_name)
    }
    
    /// 检查包是否已安装
    pub fn is_package_installed(&self, package_name: &str) -> bool {
        self.get_package_path(package_name).exists()
    }
    
    /// 读取本地路径的包版本
    pub fn read_local_package_version(local_path: &Path) -> NpmResult<Option<String>> {
        let pkg_json_path = local_path.join("package.json");
        
        if !pkg_json_path.exists() {
            return Ok(None);
        }
        
        let content = std::fs::read_to_string(&pkg_json_path)?;
        let pkg: serde_json::Value = serde_json::from_str(&content)?;
        
        Ok(pkg
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string()))
    }
    
    /// 解析包名和版本（处理 "pkg@1.2.3" 格式）
    /// 
    /// # Examples
    /// - "droid-adapter" -> ("droid-adapter", None)
    /// - "droid-adapter@1.0.0" -> ("droid-adapter", Some("1.0.0"))
    /// - "@scope/pkg@1.0.0" -> ("@scope/pkg", Some("1.0.0"))
    pub fn parse_package_spec(spec: &str) -> (&str, Option<&str>) {
        if let Some(captures) = spec.rfind('@') {
            // 处理 scoped package: "@scope/name@1.2.3"
            if spec.starts_with('@') && captures > 0 {
                let (name, version) = spec.split_at(captures);
                return (name, Some(&version[1..])); // 跳过 '@'
            }
            // 处理普通 package: "name@1.2.3"
            if captures > 0 {
                let (name, version) = spec.split_at(captures);
                return (name, Some(&version[1..]));
            }
        }
        (spec, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_package_spec() {
        assert_eq!(
            NpmManager::parse_package_spec("droid-adapter"),
            ("droid-adapter", None)
        );
        
        assert_eq!(
            NpmManager::parse_package_spec("droid-adapter@1.0.0"),
            ("droid-adapter", Some("1.0.0"))
        );
        
        assert_eq!(
            NpmManagpackage_spec("@scope/pkg@1.0.0"),
            ("@scope/pkg", Some("1.0.0"))
        );
        
        assert_eq!(
            NpmManager::parse_package_spec("@scope/pkg"),
            ("@scope/pkg", None)
        );
    }
}
