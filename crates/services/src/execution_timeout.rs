/// Execution Timeout Controller
/// 
/// 执行超时控制，确保插件代码不会无限运行

use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum TimeoutError {
    #[error("execution timeout: {0}")]
    Timeout(String),
}

pub type TimeoutResult<T> = Result<T, TimeoutError>;

/// 执行超时控制器
#[derive(Debug, Clone)]
pub struct ExecutionTimeout {
    duration: Duration,
    start_time: Instant,
}

impl ExecutionTimeout {
    /// 创建新的超时控制器
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            start_time: Instant::now(),
        }
    }
    
    /// 检查是否超时
    pub fn check(&self) -> TimeoutResult<()> {
        let elapsed = self.start_time.elapsed();
        if elapsed > self.duration {
            Err(TimeoutError::Timeout(format!(
                "execution exceeded timeout of {:?} (elapsed: {:?})",
                self.duration, elapsed
            )))
        } else {
            Ok(())
        }
    }
    
    /// 获取剩余时间
    pub fn remaining(&self) -> Duration {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            Duration::from_secs(0)
        } else {
            self.duration - elapsed
        }
    }
    
    /// 获取已用时间
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// 检查是否已超时（不返回错误）
    pub fn is_timeout(&self) -> bool {
        self.check().is_err()
    }
    
    /// 获取超时时长配置
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

impl Default for ExecutionTimeout {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 默认5分钟
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    
    #[test]
    fn test_timeout_check() {
        let timeout = ExecutionTimeout::new(Duration::from_millis(100));
        
        // 刚创建时不应超时
        assert!(timeout.check().is_ok());
        
        // 等待超时
        sleep(Duration::from_millis(150));
        
        // 现在应该超时
        assert!(timeout.check().is_err());
        assert!(timeout.is_timeout());
    }
    
    #[test]
    fn test_remaining_time() {
        let timeout = ExecutionTimeout::new(Duration::from_secs(10));
        
        let remaining = timeout.remaining();
        assert!(remaining.as_secs() <= 10);
        assert!(remaining.as_secs() >= 9); // 允许一些误差
    }
    
    #[test]
    fn test_elapsed_time() {
        let timeout = ExecutionTimeout::new(Duration::from_secs(10));
        
        sleep(Duration::from_millis(100));
        
        let elapsed = timeout.elapsed();
        assert!(elapsed.as_millis() >= 100);
        assert!(elapsed.as_millis() < 200); // 应该接近100ms
    }
    
    #[test]
    fn test_default_timeout() {
        let timeout = ExecutionTimeout::default();
        assert_eq!(timeout.duration(), Duration::from_secs(300));
        assert!(timeout.check().is_ok());
    }
}
