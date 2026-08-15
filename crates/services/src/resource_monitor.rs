/// Resource Monitor
/// 
/// 监控插件的资源使用（内存、CPU、文件句柄等）

use std::time::Instant;
use sysinfo::{System, Pid};

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("memory limit exceeded: {current} > {limit} bytes")]
    MemoryLimitExceeded { current: u64, limit: u64 },
    
    #[error("CPU time limit exceeded: {current} > {limit} seconds")]
    CpuLimitExceeded { current: u64, limit: u64 },
    
    #[error("process not found: {0}")]
    ProcessNotFound(u32),
}

pub type ResourceResult<T> = Result<T, ResourceError>;

/// 资源使用情况
#[derive(Debug, Clone)]
pub struct ResourceUsage {
    /// 内存使用（字节）
    pub memory_bytes: u64,
    
    /// CPU 时间（秒）
    pub cpu_seconds: u64,
    
    /// 采样时间
    pub sampled_at: Instant,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            memory_bytes: 0,
            cpu_seconds: 0,
            sampled_at: Instant::now(),
        }
    }
}

/// 资源配置限制
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// 最大内存（字节）
    pub max_memory_bytes: Option<u64>,
    
    /// 最大 CPU 时间（秒）
    pub max_cpu_seconds: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
            max_cpu_seconds: Some(300), // 5分钟
        }
    }
}

/// 资源监控器
pub struct ResourceMonitor {
    system: System,
    pid: Option<u32>,
    limits: ResourceLimits,
    start_time: Instant,
}

impl ResourceMonitor {
    /// 创建新的资源监控器
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            system: System::new_all(),
            pid: None,
            limits,
            start_time: Instant::now(),
        }
    }
    
    /// 设置要监控的进程 ID
    pub fn set_pid(&mut self, pid: u32) {
        self.pid = Some(pid);
    }
    
    /// 获取当前资源使用情况
    pub fn get_usage(&mut self) -> ResourceResult<ResourceUsage> {
        let pid = self.pid.ok_or_else(|| {
            ResourceError::ProcessNotFound(0)
        })?;
        
        // 刷新系统信息
        self.system.refresh_process(Pid::from_u32(pid));
        
        let process = self.system.process(Pid::from_u32(pid))
            .ok_or(ResourceError::ProcessNotFound(pid))?;
        
        // 获取内存使用
        let memory_bytes = process.memory() * 1024; // KB -> bytes
        
        // 获取 CPU 使用（这里简化处理，实际应该累积）
        let cpu_usage = process.cpu_usage() as u64;
        let elapsed_secs = self.start_time.elapsed().as_secs();
        let cpu_seconds = (cpu_usage * elapsed_secs) / 100; // 近似计算
        
        Ok(ResourceUsage {
            memory_bytes,
            cpu_seconds,
            sampled_at: Instant::now(),
        })
    }
    
    /// 检查资源使用是否超限
    pub fn check_limits(&mut self) -> ResourceResult<()> {
        let usage = self.get_usage()?;
        
        // 检查内存限制
        if let Some(max_mem) = self.limits.max_memory_bytes {
            if usage.memory_bytes > max_mem {
                return Err(ResourceError::MemoryLimitExceeded {
                    current: usage.memory_bytes,
                    limit: max_mem,
                });
            }
        }
        
        // 检查 CPU 时间限制
        if let Some(max_cpu) = self.limits.max_cpu_seconds {
            if usage.cpu_seconds > max_cpu {
                return Err(ResourceError::CpuLimitExceeded {
                    current: usage.cpu_seconds,
                    limit: max_cpu,
                });
            }
        }
        
        Ok(())
    }
    
    /// 获取资源限制配置
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }
    
    /// 更新资源限制
    pub fn update_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_bytes, Some(512 * 1024 * 1024));
        assert_eq!(limits.max_cpu_seconds, Some(300));
    }
    
    #[test]
    fn test_resource_usage_default() {
        let usage = ResourceUsage::default();
        assert_eq!(usage.memory_bytes, 0);
        assert_eq!(usage.cpu_seconds, 0);
    }
    
    #[test]
    fn test_monitor_creation() {
        let limits = ResourceLimits::default();
        let mut monitor = ResourceMonitor::new(limits);
        
        // 设置当前进程PID
        let pid = std::process::id();
        monitor.set_pid(pid);
        
        // 应该能获取资源使用情况
        let usage = monitor.get_usage();
        assert!(usage.is_ok());
        
        if let Ok(usage) = usage {
            // 当前进程应该有一些内存使用
            assert!(usage.memory_bytes > 0);
        }
    }
    
    #[test]
    fn test_limit_checking() {
        let limits = ResourceLimits {
            max_memory_bytes: Some(1024), // 只允许 1KB，肯定会超
            max_cpu_seconds: Some(1000),
        };
        let mut monitor = ResourceMonitor::new(limits);
        
        let pid = std::process::id();
        monitor.set_pid(pid);
        
        // 应该超出内存限制
        let result = monitor.check_limits();
        assert!(result.is_err());
    }
}
