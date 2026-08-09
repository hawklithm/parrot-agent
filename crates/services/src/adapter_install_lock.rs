use parking_lot::{Mutex, MutexGuard};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 安装锁管理器
/// 
/// 防止并发安装同一个适配器包，确保安装操作的原子性
#[derive(Clone)]
pub struct AdapterInstallLock {
    /// 当前正在安装的包名集合
    installing: Arc<Mutex<HashSet<String>>>,
    
    /// 锁超时时间（防止死锁）
    timeout: Duration,
}

impl AdapterInstallLock {
    /// 创建新的安装锁管理器
    pub fn new() -> Self {
        Self {
            installing: Arc::new(Mutex::new(HashSet::new())),
            timeout: Duration::from_secs(300), // 5 分钟超时
        }
    }
    
    /// 使用自定义超时时间创建
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            installing: Arc::new(Mutex::new(HashSet::new())),
            timeout,
        }
    }
    
    /// 尝试获取安装锁
    /// 
    /// 如果包正在安装中，返回 None
    /// 如果成功获取锁，返回 InstallGuard，在 Drop 时自动释放
    pub fn try_acquire(&self, package_name: &str) -> Option<InstallGuard> {
        let mut installing = self.installing.lock();
        
        if installing.contains(package_name) {
            // 包正在安装中
            return None;
        }
        
        // 获取锁
        installing.insert(package_name.to_string());
        
        Some(InstallGuard {
            package_name: package_name.to_string(),
            installing: self.installing.clone(),
            acquired_at: Instant::now(),
            timeout: self.timeout,
        })
    }
    
    /// 检查包是否正在安装
    pub fn is_installing(&self, package_name: &str) -> bool {
        let installing = self.installing.lock();
        installing.contains(package_name)
    }
    
    /// 强制释放锁（用于超时清理）
    pub fn force_release(&self, package_name: &str) -> bool {
        let mut installing = self.installing.lock();
        installing.remove(package_name)
    }
}

impl Default for AdapterInstallLock {
    fn default() -> Self {
        Self::new()
    }
}

/// 安装锁守卫
/// 
/// Drop 时自动释放锁
pub struct InstallGuard {
    package_name: String,
    installing: Arc<Mutex<HashSet<String>>>,
    acquired_at: Instant,
    timeout: Duration,
}

impl InstallGuard {
    /// 检查锁是否已超时
    pub fn is_expired(&self) -> bool {
        self.acquired_at.elapsed() > self.timeout
    }
    
    /// 获取锁持有时长
    pub fn held_duration(&self) -> Duration {
        self.acquired_at.elapsed()
    }
    
    /// 获取包名
    pub fn package_name(&self) -> &str {
        &self.package_name
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        let mut installing = self.installing.lock();
        installing.remove(&self.package_name);
        
        tracing::debug!(
            package_name = %self.package_name,
            duration_ms = self.held_duration().as_millis(),
            "Released adapter install lock"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    
    #[test]
    fn test_acquire_and_release() {
        let lock = AdapterInstallLock::new();
        let package_name = "t-adapter";
        
        // 第一次获取应该成功
        let guard1 = lock.try_acquire(package_name);
        assert!(guard1.is_some());
        assert!(lock.is_installing(package_name));
        
        // 第二次获取应该失败（包正在安装）
        let guard2 = lock.try_acquire(package_name);
        assert!(guard2.is_none());
        
        // 释放锁
        drop(guard1);
        assert!(!lock.is_installing(package_name));
        
        // 再次获取应该成功
        let guard3 = lock.try_acquire(package_name);
        assert!(guard3.is_some());
    }
    
    #[test]
    fn test_multiple_packages() {
        let lock = AdapterInstallLock::new();
        
        let guard1 = lock.try_acquire("pkg1");
        let guard2 = lock.try_acquire("pkg2");
        
        assert!(guard1.is_some());
        assert!(guard2.is_some());
        assert!(lock.is_installing("pkg1"));
        assert!(lock.is_installing("pkg2"));
    }
    
    #[test]
    fn test_timeout_check() {
        let lock = AdapterInstallLock::with_timeout(Duration::from_millis(100));
        let guard = lock.try_acquire("test").unwrap();
        
        assert!(!guard.is_expired());
        
        thread::sleep(Duration::from_millis(150));
        
        assert!(guard.is_expired());
    }
    
    #[test]
    fn test_force_release() {
        let lock = AdapterInstallLock::new();
        let package_name = "test-adapter";
        
        let _guard = lock.try_acquire(package_name);
        assert!(lock.is_installing(package_name));
        
        // 强制释放
        let released = lock.force_release(package_name);
        assert!(released);
        assert!(!lock.is_installing(package_name));
        
        // 现在可以再次获取
        let guard2 = lock.try_acquire(package_name);
        assert!(guard2.is_some());
    }
}
