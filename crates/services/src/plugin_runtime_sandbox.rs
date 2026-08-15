/// Plugin 运行时沙箱
/// 
/// 实现插件沙箱环境隔离，包括：
/// - 文件系统权限限制
/// - 网络访问控制
/// - 环境变量隔离
/// - 资源配额管理
/// - 能力验证
/// - 超时控制

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

use super::plugin_capability_validator::{PluginManifest, CapabilityValidator};
use super::execution_timeout::ExecutionTimeout;
use super::resource_monitor::{ResourceMonitor, ResourceLimits};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("access denied: {0}")]
    AccessDenied(String),
    
    #[error("path traversal attempt: {0}")]
    PathTraversal(String),
    
    #[error("resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type SandboxResult<T> = Result<T, SandboxError>;

/// 沙箱配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 允许访问的文件路径白名单
    pub allowed_paths: Vec<PathBuf>,
    
    /// 允许访问的网络域名白名单
    pub allowed_domains: Vec<String>,
    
    /// 允许的环境变量白名单
    pub allowed_env_vars: Vec<String>,
    
    /// 最大内存使用（字节）
    pub max_memory_bytes: Option<u64>,
    
    /// 最大 CPU 时间（秒）
    pub max_cpu_seconds: Option<u64>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            allowed_paths: vec![],
            allowed_domains: vec![],
            allowed_env_vars: vec!["PATH".to_string(), "NODE_ENV".to_string()],
            max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
            max_cpu_seconds: Some(300), // 5分钟
        }
    }
}

/// Plugin 运行时沙箱
pub struct PluginRuntimeSandbox {
    plugin_id: Uuid,
    config: SandboxConfig,
    
    // 能力验证器
    validator: Option<CapabilityValidator>,
    
    // 超时控制
    timeout: ExecutionTimeout,
    
    // 资源监控
    monitor: Option<ResourceMonitor>,
    
    // 路径和域名白名单缓存
    allowed_path_set: HashSet<PathBuf>,
    allowed_domain_set: HashSet<String>,
}

impl PluginRuntimeSandbox {
    /// 创建新的沙箱实例
    pub fn new(plugin_id: Uuid, config: SandboxConfig) -> Self {
        let allowed_path_set = config.allowed_paths.iter().cloned().collect();
        let allowed_domain_set = config.allowed_domains.iter().cloned().collect();
        
        // 初始化超时控制（默认5分钟）
        let timeout_duration = config.max_cpu_seconds
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(300));
        let timeout = ExecutionTimeout::new(timeout_duration);
        
        // 初始化资源监控（如果配置了限制）
        let monitor = if config.max_memory_bytes.is_some() || config.max_cpu_seconds.is_some() {
            Some(ResourceMonitor::new(ResourceLimits {
                max_memory_bytes: config.max_memory_bytes,
                max_cpu_seconds: config.max_cpu_seconds,
            }))
        } else {
            None
        };

        Self {
            plugin_id,
            config,
            validator: None,
            timeout,
            monitor,
            allowed_path_set,
            allowed_domain_set,
        }
    }
    
    /// 创建带能力验证的沙箱
    pub fn with_manifest(
        plugin_id: Uuid,
        config: SandboxConfig,
        manifest: PluginManifest,
    ) -> Self {
        let mut sandbox = Self::new(plugin_id, config);
        sandbox.validator = Some(CapabilityValidator::new(manifest));
        sandbox
    }
    
    /// 设置要监控的 Worker 进程 PID
    pub fn set_worker_pid(&mut self, pid: u32) {
        if let Some(ref mut monitor) = self.monitor {
            monitor.set_pid(pid);
        }
    }
    
    /// 检查文件路径访问权限
    pub fn check_file_access(&self, path: &Path) -> SandboxResult<()> {
        // 规范化路径
        let canonical = path.canonicalize().map_err(|e| {
            SandboxError::AccessDenied(format!("cannot resolve path: {}", e))
        })?;
        
        // 检查路径穿越
        if canonical.to_string_lossy().contains("..") {
            return Err(SandboxError::PathTraversal(
                canonical.display().to_string()
            ));
        }
        
        // 检查白名单
        let is_allowed = self.allowed_path_set.iter().any(|allowed| {
            canonical.starts_with(allowed)
        });
        
        if !is_allowed {
            return Err(SandboxError::AccessDenied(format!(
                "path not in whitelist: {}",
                canonical.display()
            )));
        }
        
        Ok(())
    }
    
    /// 检查危险操作
    pub fn check_dangerous_operation(&self, operation: &str) -> SandboxResult<()> {
        const DANGEROUS_OPS: &[&str] = &[
            "exec",
            "spawn",
            "fork",
            "eval",
            "system",
            "shell",
        ];
        
        for &dangerous in DANGEROUS_OPS {
            if operation.contains(dangerous) {
                return Err(SandboxError::AccessDenied(format!(
                    "dangerous operation '{}' is not allowed",
                    operation
                )));
            }
        }
        
        Ok(())
    }
    
    /// 检查操作权限（使用能力验证器）
    pub fn check_operation(&self, operation: &str) -> SandboxResult<()> {
        if let Some(ref validator) = self.validator {
            validator.assert_operation(operation)
                .map_err(|e| SandboxError::AccessDenied(e.to_string()))?;
        }
        Ok(())
    }
    
    /// 检查所有限制（超时 + 资源）
    pub fn check_all_limits(&mut self) -> SandboxResult<()> {
        // 检查超时
        self.timeout.check()
            .map_err(|e| SandboxError::ResourceLimitExceeded(e.to_string()))?;
        
        // 检查资源使用
        if let Some(ref mut monitor) = self.monitor {
            monitor.check_limits()
                .map_err(|e| SandboxError::ResourceLimitExceeded(e.to_string()))?;
        }
        
        Ok(())
    }
    
    /// 执行操作（带完整检查）
    pub async fn execute_with_checks<F, Fut, T>(
        &mut self,
        operation: &str,
        f: F,
    ) -> SandboxResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        // 1. 检查操作权限
        self.check_operation(operation)?;
        
        // 2. 检查超时和资源（执行前）
        self.check_all_limits()?;
        
        // 3. 执行操作
        let result = f().await;
        
        // 4. 再次检查限制（执行后）
        self.check_all_limits()?;
        
        Ok(result)
    }
    
    /// 验证资源配额
    pub fn check_resource_quota(&self, used_memory: u64, used_cpu: u64) -> SandboxResult<()> {
        if let Some(max_mem) = self.config.max_memory_bytes {
            if used_memory > max_mem {
                return Err(SandboxError::ResourceLimitExceeded(format!(
                    "memory usage {} exceeds limit {}",
                    used_memory, max_mem
                )));
            }
        }
        
        if let Some(max_cpu) = self.config.max_cpu_seconds {
            if used_cpu > max_cpu {
                return Err(SandboxError::ResourceLimitExceeded(format!(
                    "CPU time {} exceeds limit {}",
                    used_cpu, max_cpu
                )));
            }
        }
        
        Ok(())
    }
    
    /// 获取剩余执行时间
    pub fn remaining_time(&self) -> Duration {
        self.timeout.remaining()
    }
    
    /// 获取已用执行时间
    pub fn elapsed_time(&self) -> Duration {
        self.timeout.elapsed()
    }
    
    /// 获取资源使用情况
    pub fn get_resource_usage(&mut self) -> Option<super::resource_monitor::ResourceUsage> {
        self.monitor.as_mut().and_then(|m| m.get_usage().ok())
    }
    
    /// 获取沙箱配置
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }
    
    /// 更新沙箱配置
    pub fn update_config(&mut self, config: SandboxConfig) {
        self.allowed_path_set = config.allowed_paths.iter().cloned().collect();
        self.allowed_domain_set = config.allowed_domains.iter().cloned().collect();
        self.config = config;
    }
    
    /// 检查网络访问权限
    pub fn check_network_access(&self, domain: &str) -> SandboxResult<()> {
        if self.allowed_domain_set.contains(domain) {
            Ok(())
        } else {
            Err(SandboxError::AccessDenied(format!(
                "domain not in whitelist: {}",
                domain
            )))
        }
    }
    
    /// 检查环境变量访问权限
    pub fn check_env_access(&self, var_name: &str) -> SandboxResult<()> {
        if self.config.allowed_env_vars.contains(&var_name.to_string()) {
            Ok(())
        } else {
            Err(SandboxError::AccessDenied(format!(
                "environment variable not allowed: {}",
                var_name
            )))
        }
    }
    
    /// 过滤环境变量
    pub fn filter_env_vars(&self, env: &[(String, String)]) -> Vec<(String, String)> {
        env.iter()
            .filter(|(key, _)| self.check_env_access(key).is_ok())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_sandbox_creation() {
        let config = SandboxConfig::default();
        let sandbox = PluginRuntimeSandbox::new(Uuid::new_v4(), config);
        
        assert_eq!(sandbox.allowed_path_set.len(), 0);
        assert_eq!(sandbox.allowed_domain_set.len(), 0);
    }
    
    #[test]
    fn test_dangerous_operation_blocked() {
        let config = SandboxConfig::default();
        let sandbox = PluginRuntimeSandbox::new(Uuid::new_v4(), config);
        
        assert!(sandbox.check_dangerous_operation("eval('code')").is_err());
        assert!(sandbox.check_dangerous_operation("exec('command')").is_err());
        assert!(sandbox.check_dangerous_operation("safe_operation").is_ok());
    }
    
    #[test]
    fn test_sandbox_env_filter() {
        let config = SandboxConfig {
            allowed_env_vars: vec!["PATH".to_string(), "HOME".to_string()],
            ..Default::default()
        };
        
        let sandbox = PluginRuntimeSandbox::new(Uuid::new_v4(), config);
        
        let env = vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("SECRET".to_string(), "password".to_string()),
            ("HOME".to_string(), "/home/user".to_string()),
        ];
        
        let filtered = sandbox.filter_env_vars(&env);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|(k, _)| k == "PATH" || k == "HOME"));
    }
    
    #[tokio::test]
    async fn test_sandbox_with_manifest() {
        use super::super::plugin_capability_validator::*;
        
        let manifest = PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: PluginCapabilities {
                file_access: vec!["/tmp/*".to_string()],
                network_access: vec!["api.example.com".to_string()],
                allowed_commands: vec![],
                host_apis: vec!["read_file".to_string(), "write_file".to_string()],
                tools: vec![],
                env_vars: vec!["PATH".to_string()],
            },
        };
        
        let config = SandboxConfig::default();
        let mut sandbox = PluginRuntimeSandbox::with_manifest(
            Uuid::new_v4(),
            config,
            manifest,
        );
        
        // 测试允许的操作
        let result = sandbox.execute_with_checks("read_file", || async {
            "file content".to_string()
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "file content");
    }
    
    #[tokio::test]
    async fn test_sandbox_operation_denied() {
        use super::super::plugin_capability_validator::*;
        
        let manifest = PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: PluginCapabilities {
                file_access: vec![],
                network_access: vec![],
                allowed_commands: vec![],
                host_apis: vec!["read_file".to_string()],
                tools: vec![],
                env_vars: vec![],
            },
        };
        
        let config = SandboxConfig::default();
        let mut sandbox = PluginRuntimeSandbox::with_manifest(
            Uuid::new_v4(),
            config,
            manifest,
        );
        
        // 尝试执行未授权的操作
        let result = sandbox.execute_with_checks("delete_database", || async {
            ()
        }).await;
        
        assert!(result.is_err());
        if let Err(SandboxError::AccessDenied(msg)) = result {
            assert!(msg.contains("delete_database"));
        } else {
            panic!("Expected AccessDenied error");
        }
    }
    
    #[tokio::test]
    async fn test_sandbox_timeout_check() {
        use super::super::plugin_capability_validator::*;
        use tokio::time::sleep;
        
        let manifest = PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: PluginCapabilities {
                file_access: vec![],
                network_access: vec![],
                allowed_commands: vec![],
                host_apis: vec!["slow_operation".to_string()],
                tools: vec![],
                env_vars: vec![],
            },
        };
        
        let mut config = SandboxConfig::default();
        config.max_cpu_seconds = Some(1); // 1秒超时
        
        let mut sandbox = PluginRuntimeSandbox::with_manifest(
            Uuid::new_v4(),
            config,
            manifest,
        );
        
        // 等待超时
        sleep(Duration::from_millis(1100)).await;
        
        // 检查限制应该失败
        let result = sandbox.check_all_limits();
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_sandbox_resource_monitoring() {
        let config = SandboxConfig {
            allowed_paths: vec![],
            allowed_domains: vec![],
            allowed_env_vars: vec![],
            max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
            max_cpu_seconds: Some(300),
        };
        
        let mut sandbox = PluginRuntimeSandbox::new(Uuid::new_v4(), config);
        
        // 设置当前进程为监控目标
        let pid = std::process::id();
        sandbox.set_worker_pid(pid);
        
        // 获取资源使用情况
        let usage = sandbox.get_resource_usage();
        assert!(usage.is_some());
        
        if let Some(usage) = usage {
            // 当前进程应该有一些内存使用
            assert!(usage.memory_bytes > 0);
        }
    }
    
    #[tokio::test]
    async fn test_sandbox_timing_info() {
        let config = SandboxConfig::default();
        let sandbox = PluginRuntimeSandbox::new(Uuid::new_v4(), config);
        
        // 刚创建时，elapsed 应该接近 0
        let elapsed = sandbox.elapsed_time();
        assert!(elapsed.as_millis() < 100);
        
        // remaining 应该接近最大值（300秒）
        let remaining = sandbox.remaining_time();
        assert!(remaining.as_secs() >= 299);
    }
    
    #[tokio::test]
    async fn test_sandbox_multiple_checks() {
        use super::super::plugin_capability_validator::*;
        
        let manifest = PluginManifest {
            id: "test-plugin".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            capabilities: PluginCapabilities {
                file_access: vec![],
                network_access: vec![],
                allowed_commands: vec![],
                host_apis: vec![
                    "operation1".to_string(),
                    "operation2".to_string(),
                    "operation3".to_string(),
                ],
                tools: vec![],
                env_vars: vec![],
            },
        };
        
        let config = SandboxConfig::default();
        let mut sandbox = PluginRuntimeSandbox::with_manifest(
            Uuid::new_v4(),
            config,
            manifest,
        );
        
        // 连续执行多个操作
        for op in &["operation1", "operation2", "operation3"] {
            let result = sandbox.execute_with_checks(op, || async {
                format!("executed {}", op)
            }).await;
            assert!(result.is_ok());
        }
        
        // 尝试未授权操作
        let result = sandbox.execute_with_checks("operation4", || async {
            "should fail".to_string()
        }).await;
        assert!(result.is_err());
    }
}
