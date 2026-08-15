/// Plugin Capability Validator
/// 
/// 验证插件的能力声明和运行时权限

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("operation not permitted: {0}")]
    OperationDenied(String),
    
    #[error("capability not declared: {0}")]
    CapabilityNotDeclared(String),
    
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
}

pub type CapabilityResult<T> = Result<T, CapabilityError>;

/// Plugin 能力声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// 可以访问的文件路径模式
    pub file_access: Vec<String>,
    
    /// 可以访问的网络域名
    pub network_access: Vec<String>,
    
    /// 可以使用的系统命令
    pub allowed_commands: Vec<String>,
    
    /// 可以调用的宿主 API
    pub host_apis: Vec<String>,
    
    /// 可以使用的工具
    pub tools: Vec<String>,
    
    /// 环境变量访问权限
    pub env_vars: Vec<String>,
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self {
            file_access: vec![],
            network_access: vec![],
            allowed_commands: vec![],
            host_apis: vec![],
            tools: vec![],
            env_vars: vec!["PATH".to_string(), "NODE_ENV".to_string()],
        }
    }
}

/// Plugin Manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: PluginCapabilities,
}

/// 能力验证器
pub struct CapabilityValidator {
    manifest: PluginManifest,
    file_patterns: HashSet<String>,
    network_domains: HashSet<String>,
    allowed_commands: HashSet<String>,
    host_apis: HashSet<String>,
    tools: HashSet<String>,
    env_vars: HashSet<String>,
}

impl CapabilityValidator {
    pub fn new(manifest: PluginManifest) -> Self {
        let file_patterns = manifest.capabilities.file_access.iter().cloned().collect();
        let network_domains = manifest.capabilities.network_access.iter().cloned().collect();
        let allowed_commands = manifest.capabilities.allowed_commands.iter().cloned().collect();
        let host_apis = manifest.capabilities.host_apis.iter().cloned().collect();
        let tools = manifest.capabilities.tools.iter().cloned().collect();
        let env_vars = manifest.capabilities.env_vars.iter().cloned().collect();
        
        Self {
            manifest,
            file_patterns,
            network_domains,
            allowed_commands,
            host_apis,
            tools,
            env_vars,
        }
    }
    
    /// 验证操作权限
    pub fn assert_operation(&self, operation: &str) -> CapabilityResult<()> {
        if !self.host_apis.contains(operation) {
            return Err(CapabilityError::OperationDenied(format!(
                "operation '{}' not declared in manifest capabilities",
                operation
            )));
        }
        Ok(())
    }
    
    /// 检查文件访问权限
    pub fn can_access_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        // 检查是否匹配任何声明的模式
        self.file_patterns.iter().any(|pattern| {
            // 简单的 glob 匹配
            if pattern.ends_with("/*") {
                let prefix = &pattern[..pattern.len() - 2];
                path_str.starts_with(prefix)
            } else if pattern.contains('*') {
                // 使用 glob 库进行完整匹配（这里简化处理）
                self.simple_glob_match(pattern, &path_str)
            } else {
                path_str.as_ref() == pattern
            }
        })
    }
    
    /// 检查网络访问权限
    pub fn can_access_network(&self, domain: &str) -> bool {
        self.network_domains.contains(domain) || 
        self.network_domains.iter().any(|pattern| {
            // 支持通配符域名，如 *.example.com
            if pattern.starts_with("*.") {
                let suffix = &pattern[2..];
                domain.ends_with(suffix) || domain == suffix.trim_start_matches('.')
            } else {
                domain == pattern
            }
        })
    }
    
    /// 检查命令执行权限
    pub fn can_execute_command(&self, command: &str) -> bool {
        self.allowed_commands.contains(command)
    }
    
    /// 检查工具使用权限
    pub fn can_use_tool(&self, tool_name: &str) -> bool {
        self.tools.contains(tool_name) || self.tools.contains("*")
    }
    
    /// 检查环境变量访问权限
    pub fn can_access_env(&self, var_name: &str) -> bool {
        self.env_vars.contains(var_name)
    }
    
    /// 获取 manifest
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
    
    /// 简单的 glob 匹配实现
    fn simple_glob_match(&self, pattern: &str, text: &str) -> bool {
        // 简化的 glob 实现，支持 * 通配符
        let parts: Vec<&str> = pattern.split('*').collect();
        
        if parts.is_empty() {
            return text.is_empty();
        }
        
        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if i == 0 {
                // 第一部分必须在开头匹配
                if !text[pos..].starts_with(part) {
                    return false;
                }
                pos += part.len();
            } else if i == parts.len() - 1 {
                // 最后一部分必须在结尾匹配
                return text[pos..].ends_with(part);
            } else {
                // 中间部分查找匹配
                if let Some(found_pos) = text[pos..].find(part) {
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    fn test_manifest() -> PluginManifest {
        PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: PluginCapabilities {
                file_access: vec![
                    "/tmp/plugin/*".to_string(),
                    "/data/shared".to_string(),
                ],
                network_access: vec![
                    "api.example.com".to_string(),
                    "*.cdn.com".to_string(),
                ],
                allowed_commands: vec!["git".to_string(), "npm".to_string()],
                host_apis: vec![
                    "read_file".to_string(),
                    "write_file".to_string(),
                    "http_request".to_string(),
                ],
                tools: vec!["web_search".to_string(), "code_analyzer".to_string()],
                env_vars: vec!["PATH".to_string(), "HOME".to_string()],
            },
        }
    }
    
    #[test]
    fn test_operation_validation() {
        let validator = CapabilityValidator::new(test_manifest());
        
        // 允许的操作
        assert!(validator.assert_operation("read_file").is_ok());
        assert!(validator.assert_operation("write_file").is_ok());
        assert!(validator.assert_operation("http_request").is_ok());
        
        // 不允许的操作
        assert!(validator.assert_operation("delete_database").is_err());
        assert!(validator.assert_operation("execute_shell").is_err());
    }
    
    #[test]
    fn test_file_access_validation() {
        let validator = CapabilityValidator::new(test_manifest());
        
        // 允许的路径
        assert!(validator.can_access_file(Path::new("/tmp/plugin/data.txt")));
        assert!(validator.can_access_file(Path::new("/tmp/plugin/subdir/file.json")));
        assert!(validator.can_access_file(Path::new("/data/shared")));
        
        // 不允许的路径
        assert!(!validator.can_access_file(Path::new("/etc/passwd")));
        assert!(!validator.can_access_file(Path::new("/tmp/other/file.txt")));
    }
    
    #[test]
    fn test_network_access_validation() {
        let validator = CapabilityValidator::new(test_manifest());
        
        // 允许的域名
        assert!(validator.can_access_network("api.example.com"));
        assert!(validator.can_access_network("images.cdn.com"));
        assert!(validator.can_access_network("static.cdn.com"));
        
        // 不允许的域名
        assert!(!validator.can_access_network("evil.com"));
        assert!(!validator.can_access_network("api.other.com"));
    }
    
    #[test]
    fn test_tool_access_validation() {
        let validator = CapabilityValidator::new(test_manifest());
        
        assert!(validator.can_use_tool("web_search"));
        assert!(validator.can_use_tool("code_analyzer"));
        assert!(!validator.can_use_tool("file_deleter"));
    }
    
    #[test]
    fn test_glob_matching() {
        let validator = CapabilityValidator::new(test_manifest());
        
        assert!(validator.simple_glob_match("/tmp/*", "/tmp/file.txt"));
        assert!(validator.simple_glob_match("/tmp/*/data", "/tmp/plugin/data"));
        assert!(validator.simple_glob_match("*.txt", "file.txt"));
        assert!(!validator.simple_glob_match("/tmp/*", "/var/file.txt"));
    }
}
