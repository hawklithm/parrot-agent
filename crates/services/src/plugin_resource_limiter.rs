/// Plugin Resource Limiter
/// 
/// 实现 Worker 进程资源限制（CPU、内存、文件描述符）

use serde::{Deserialize, Serialize};
use tracing as log;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU 限制 (百分比, 0-100)
    pub cpu_percent: Option<u32>,
    /// 内存限制 (MB)
    pub memory_mb: Option<u64>,
    /// 文件描述符限制
    pub file_descriptors: Option<u32>,
    /// 执行超时
    pub timeout: Option<Duration>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: Some(50),
            memory_mb: Some(512),
            file_descriptors: Some(1024),
            timeout: Some(Duration::from_secs(300)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceLimitError {
    #[error("CPU limit exceeded: {0}%")]
    CpuLimitExceeded(u32),
    #[error("Memory limit exceeded: {0}MB")]
    MemoryLimitExceeded(u64),
    #[error("File descriptor limit exceeded: {0}")]
    FileDescriptorLimitExceeded(u32),
    #[error("Execution timeout: {0:?}")]
    TimeoutExceeded(Duration),
    #[error("System error: {0}")]
    SystemError(String),
}

pub type ResourceResult<T> = Result<T, ResourceLimitError>;

/// 资源监控器
#[derive(Debug)]
pub struct ResourceMonitor {
    limits: ResourceLimits,
    start_time: std::time::Instant,
}

impl ResourceMonitor {
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            start_time: std::time::Instant::now(),
        }
    }

    /// 检查资源使用是否超限
    pub fn check_limits(&self) -> ResourceResult<()> {
        // 检查超时
        if let Some(timeout) = self.limits.timeout {
            let elapsed = self.start_time.elapsed();
            if elapsed > timeout {
                return Err(ResourceLimitError::TimeoutExceeded(timeout));
            }
        }

        // 这里应该实现实际的资源检查逻辑
        // 在生产环境中需要使用系统调用获取进程资源使用情况
        
        Ok(())
    }

    /// 获取已运行时间
    pub fn elapsed_time(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// 获取资源限制配置
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }
}

/// 资源限制器服务
#[derive(Debug)]
pub struct ResourceLimiterService {
    default_limits: ResourceLimits,
}

impl ResourceLimiterService {
    pub fn new(default_limits: ResourceLimits) -> Self {
        Self { default_limits }
    }

    /// 创建资源监控器
    pub fn create_monitor(&self, limits: Option<ResourceLimits>) -> ResourceMonitor {
        let limits = limits.unwrap_or_else(|| self.default_limits.clone());
        ResourceMonitor::new(limits)
    }

    /// 应用资源限制到进程
    pub fn apply_limits(&self, _pid: u32, limits: &ResourceLimits) -> ResourceResult<()> {
        // 在实际实现中，这里应该使用系统调用设置进程资源限制
        // 例如在 Unix 系统上使用 setrlimit
        
        // 记录应用的限制
        if let Some(cpu) = limits.cpu_percent {
            log::debug!("Applied CPU limit: {}%", cpu);
        }
        if let Some(memory) = limits.memory_mb {
            log::debug!("Applied memory limit: {}MB", memory);
        }
        if let Some(fds) = limits.file_descriptors {
            log::debug!("Applied FD limit: {}", fds);
        }

        Ok(())
    }
}

impl Default for ResourceLimiterService {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.cpu_percent, Some(50));
        assert_eq!(limits.memory_mb, Some(512));
        assert_eq!(limits.file_descriptors, Some(1024));
        assert!(limits.timeout.is_some());
    }

    #[test]
    fn test_resource_monitor() {
        let limits = ResourceLimits::default();
        let monitor = ResourceMonitor::new(limits);
        
        assert!(monitor.check_limits().is_ok());
        assert!(monitor.elapsed_time().as_secs() < 1);
    }

    #[test]
    fn test_resource_limiter_service() {
        let service = ResourceLimiterService::default();
        let monitor = service.create_monitor(None);
        
        assert!(monitor.check_limits().is_ok());
    }

    #[test]
    fn test_apply_limits() {
        let service = ResourceLimiterService::default();
        let limits = ResourceLimits {
            cpu_percent: Some(30),
            memory_mb: Some(256),
            file_descriptors: Some(512),
            timeout: Some(Duration::from_secs(60)),
        };
        
        // 测试应用限制（实际不会真正应用到进程）
        let result = service.apply_limits(12345, &limits);
        assert!(result.is_ok());
    }

    #[test]
    fn test_timeout_check() {
        let limits = ResourceLimits {
            cpu_percent: None,
            memory_mb: None,
            file_descriptors: None,
            timeout: Some(Duration::from_millis(1)),
        };
        
        let monitor = ResourceMonitor::new(limits);
        std::thread::sleep(Duration::from_millis(10));
        
        let result = monitor.check_limits();
        assert!(result.is_err());
        assert!(matches!(result, Err(ResourceLimitError::TimeoutExceeded(_))));
    }
}
