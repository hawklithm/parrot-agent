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
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

use super::plugin_capability_validator::CapabilityValidator;
use super::execution_timeout::ExecutionTimeout;
use super::resource_monitor::ResourceMonitor;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("execution timeout: {0}")]
    Timeout(String),
    #[error("resource limit exceeded: {0}")]
    ResourceExceeded(String),
    #[error("validation failed: {0}")]
    ValidationError(String),
    #[error("capability denied: {0}")]
    CapabilityDenied(String),
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("path access denied: {0}")]
    PathDenied(String),
    #[error("network access denied: {0}")]
    NetworkDenied(String),
}

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxConfig {
    pub allowed_paths: Vec<PathBuf>,
    
    /// 允许访问的网络域名白名单
    pub allowed_domains: Vec<String>,
    
    /// 允许的能力白名单
    pub allowed_capabilities: Vec<String>,
    
    /// 允许的环境变量白名单
    pub allowed_env_vars: Vec<String>,
    
    /// 最大内存使用（字节）
    pub max_memory_bytes: Option<u64>,
    
    /// 最大 CPU 时间（秒）
    pub max_cpu_seconds: Option<u64>,
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
    pub fn new(plugin_id: Uuid, config: SandboxConfig) -> Self {
        let allowed_path_set = config.allowed_paths.iter().cloned().collect();
        let allowed_domain_set = config.allowed_domains.iter().cloned().collect();
        
        Self {
            plugin_id,
            config,
            validator: None,
            timeout: ExecutionTimeout::default(),
            monitor: None,
            allowed_path_set,
            allowed_domain_set,
        }
    }

    pub fn with_manifest(plugin_id: Uuid, config: SandboxConfig, manifest: super::plugin_capability_validator::PluginManifest) -> Self {
        let mut sandbox = Self::new(plugin_id, config);
        sandbox.validator = Some(CapabilityValidator::new(manifest));
        sandbox
    }

    pub fn check_dangerous_operation(&self, operation: &str) -> SandboxResult<()> {
        let lower = operation.to_ascii_lowercase();
        if lower.contains("eval(") || lower.contains("exec(") || lower.contains("drop database") {
            return Err(SandboxError::AccessDenied(operation.to_owned()));
        }
        Ok(())
    }

    pub fn filter_env_vars(&self, env: &[(String, String)]) -> Vec<(String, String)> {
        env.iter()
            .filter(|(name, _)| self.config.allowed_env_vars.iter().any(|allowed| allowed == name))
            .cloned()
            .collect()
    }

    pub async fn execute_with_checks<F, Fut, T>(&mut self, operation: &str, action: F) -> SandboxResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.check_dangerous_operation(operation)?;
        if let Some(validator) = &self.validator {
            validator.assert_operation(operation)
                .map_err(|error| SandboxError::AccessDenied(error.to_string()))?;
        }
        self.check_all_limits()?;
        let result = action().await;
        self.check_all_limits()?;
        Ok(result)
    }

    pub fn check_all_limits(&mut self) -> SandboxResult<()> {
        self.timeout
            .check()
            .map_err(|error| SandboxError::Timeout(error.to_string()))?;
        if let Some(monitor) = &mut self.monitor {
            monitor
                .check_limits()
                .map_err(|error| SandboxError::ResourceExceeded(error.to_string()))?;
        }
        Ok(())
    }

    pub fn set_worker_pid(&mut self, pid: u32) {
        let limits = super::resource_monitor::ResourceLimits {
            max_memory_bytes: self.config.max_memory_bytes,
            max_cpu_seconds: self.config.max_cpu_seconds,
        };
        let monitor = self.monitor.get_or_insert_with(|| ResourceMonitor::new(limits));
        monitor.set_pid(pid);
    }

    pub fn get_resource_usage(&mut self) -> Option<super::resource_monitor::ResourceUsage> {
        self.monitor.as_mut().and_then(|monitor| monitor.get_usage().ok())
    }

    pub fn elapsed_time(&self) -> Duration {
        self.timeout.elapsed()
    }

    pub fn remaining_time(&self) -> Duration {
        self.timeout.remaining()
    }
    
    /// 验证能力
    pub fn validate_capability(&self, capability: &str) -> SandboxResult<()> {
        if self.config.allowed_capabilities.contains(&capability.to_string()) {
            Ok(())
        } else {
            // 记录违规行为（使用 plugin_id）
            tracing::warn!(
                plugin_id = %self.plugin_id,
                capability = %capability,
                "plugin attempted to use unauthorized capability"
            );
            Err(SandboxError::CapabilityDenied(capability.to_string()))
        }
    }
    
    /// 验证文件路径
    pub fn validate_path(&self, path: &PathBuf) -> SandboxResult<()> {
        if self.allowed_path_set.iter().any(|allowed| path.starts_with(allowed)) {
            Ok(())
        } else {
            // 记录违规行为（使用 plugin_id）
            tracing::warn!(
                plugin_id = %self.plugin_id,
                path = %path.display(),
                "plugin attempted to access unauthorized path"
            );
            Err(SandboxError::PathDenied(path.display().to_string()))
        }
    }
    
    /// 验证网络域名
    pub fn validate_domain(&self, domain: &str) -> SandboxResult<()> {
        if self.allowed_domain_set.contains(domain) {
            Ok(())
        } else {
            // 记录违规行为（使用 plugin_id）
            tracing::warn!(
                plugin_id = %self.plugin_id,
                domain = %domain,
                "plugin attempted to access unauthorized domain"
            );
            Err(SandboxError::NetworkDenied(domain.to_string()))
        }
    }
    
    /// 检查资源限制
    pub fn check_resource_limits(&self) -> SandboxResult<()> {
        // TODO: 实现实际的资源检查
        // 使用 plugin_id 查询当前资源使用情况
        Ok(())
    }
    
    /// 记录审计事件
    pub fn log_audit_event(&self, event_type: &str, details: &str) {
        tracing::info!(
            plugin_id = %self.plugin_id,
            event_type = %event_type,
            details = %details,
            "plugin sandbox audit event"
        );
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
            allowed_capabilities: vec![],
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
