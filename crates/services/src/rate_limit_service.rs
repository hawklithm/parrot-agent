/// Rate Limit Service
/// 
/// 速率限制和配额管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("rate limit exceeded: {0}")]
    Exceeded(String),
    
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}

pub type RateLimitResult<T> = Result<T, RateLimitError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub window_seconds: u32,
    pub burst_size: Option<u32>,
}

impl RateLimitConfig {
    pub fn per_minute(max_requests: u32) -> Self {
        Self {
            max_requests,
            window_seconds: 60,
            burst_size: None,
        }
    }
    
    pub fn per_hour(max_requests: u32) -> Self {
        Self {
            max_requests,
            window_seconds: 3600,
            burst_size: None,
        }
    }
    
    pub fn with_burst(mut self, burst_size: u32) -> Self {
        self.burst_size = Some(burst_size);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RateLimitState {
    requests: Vec<chrono::DateTime<chrono::Utc>>,
    burst_tokens: u32,
}

impl RateLimitState {
    fn new(burst_size: u32) -> Self {
        Self {
            requests: Vec::new(),
            burst_tokens: burst_size,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub allowed: bool,
    pub current_requests: u32,
    pub max_requests: u32,
    pub remaining: u32,
    pub reset_at: chrono::DateTime<chrono::Utc>,
}

pub struct RateLimitService {
    states: Arc<RwLock<HashMap<String, RateLimitState>>>,
    configs: Arc<RwLock<HashMap<String, RateLimitConfig>>>,
}

impl RateLimitService {
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 设置速率限制配置
    pub async fn set_config(&self, key: String, config: RateLimitConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(key, config);
    }
    
    /// 检查是否允许请求
    pub async fn check(&self, key: &str) -> RateLimitResult<RateLimitStatus> {
        let configs = self.configs.read().await;
        let config = configs.get(key)
            .ok_or_else(|| RateLimitError::InvalidConfig(format!("No config for key: {}", key)))?
            .clone();
        drop(configs);
        
        let now = chrono::Utc::now();
        let window_start = now - chrono::Duration::seconds(config.window_seconds as i64);
        
        let mut states = self.states.write().await;
        let state = states.entry(key.to_string())
            .or_insert_with(|| RateLimitState::new(config.burst_size.unwrap_or(0)));
        
        // 清理窗口外的请求
        state.requests.retain(|&req_time| req_time > window_start);
        
        let current_requests = state.requests.len() as u32;
        let allowed = current_requests < config.max_requests;
        let remaining = if allowed {
            config.max_requests - current_requests - 1
        } else {
            0
        };
        
        let reset_at = if let Some(oldest) = state.requests.first() {
            *oldest + chrono::Duration::seconds(config.window_seconds as i64)
        } else {
            now + chrono::Duration::seconds(config.window_seconds as i64)
        };
        
        Ok(RateLimitStatus {
            allowed,
            current_requests,
            max_requests: config.max_requests,
            remaining,
            reset_at,
        })
    }
    
    /// 记录请求
    pub async fn record(&self, key: &str) -> RateLimitResult<()> {
        let status = self.check(key).await?;
        
        if !status.allowed {
            return Err(RateLimitError::Exceeded(format!(
                "Rate limit exceeded for key: {}. Current: {}, Max: {}",
                key, status.current_requests, status.max_requests
            )));
        }
        
        let mut states = self.states.write().await;
        if let Some(state) = states.get_mut(key) {
            state.requests.push(chrono::Utc::now());
        }
        
        Ok(())
    }
    
    /// 重置速率限制
    pub async fn reset(&self, key: &str) {
        let mut states = self.states.write().await;
        states.remove(key);
    }
    
    /// 获取当前状态
    pub async fn get_status(&self, key: &str) -> Option<RateLimitStatus> {
        self.check(key).await.ok()
    }
    
    /// 批量检查
    pub async fn check_batch(&self, keys: &[String]) -> HashMap<String, RateLimitResult<RateLimitStatus>> {
        let mut results = HashMap::new();
        
        for key in keys {
            let status = self.check(key).await;
            results.insert(key.clone(), status);
        }
        
        results
    }
    
    /// 用户级别速率限制
    pub async fn check_user(&self, user_id: Uuid, resource: &str) -> RateLimitResult<RateLimitStatus> {
        let key = format!("user:{}:{}", user_id, resource);
        self.check(&key).await
    }
    
    /// 记录用户请求
    pub async fn record_user(&self, user_id: Uuid, resource: &str) -> RateLimitResult<()> {
        let key = format!("user:{}:{}", user_id, resource);
        self.record(&key).await
    }
    
    /// IP级别速率限制
    pub async fn check_ip(&self, ip: &str, resource: &str) -> RateLimitResult<RateLimitStatus> {
        let key = format!("ip:{}:{}", ip, resource);
        self.check(&key).await
    }
    
    /// 记录IP请求
    pub async fn record_ip(&self, ip: &str, resource: &str) -> RateLimitResult<()> {
        let key = format!("ip:{}:{}", ip, resource);
        self.record(&key).await
    }
    
    /// 获取所有活跃的限制状态
    pub async fn get_all_statuses(&self) -> Vec<(String, RateLimitStatus)> {
        let states = self.states.read().await;
        let mut results = Vec::new();
        
        for key in states.keys() {
            if let Ok(status) = self.check(key).await {
                results.push((key.clone(), status));
            }
        }
        
        results
    }
    
    /// 清理过期状态
    pub async fn cleanup_expired(&self) -> usize {
        let configs = self.configs.read().await;
        let mut states = self.states.write().await;
        let now = chrono::Utc::now();
        
        let mut to_remove = Vec::new();
        
        for (key, state) in states.iter() {
            if let Some(config) = configs.get(key) {
                let window_start = now - chrono::Duration::seconds(config.window_seconds as i64);
                let active_requests: Vec<_> = state.requests.iter()
                    .filter(|&&req_time| req_time > window_start)
                    .collect();
                
                if active_requests.is_empty() {
                    to_remove.push(key.clone());
                }
            }
        }
        
        let count = to_remove.len();
        for key in to_remove {
            states.remove(&key);
        }
        
        count
    }
}

impl Default for RateLimitService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_rate_limit_basic() {
        let service = RateLimitService::new();
        let config = RateLimitConfig::per_minute(3);
        
        service.set_config("test".to_string(), config).await;
        
        // 前3个请求应该成功
        for _ in 0..3 {
            assert!(service.record("test").await.is_ok());
        }
        
        // 第4个请求应该失败
        assert!(service.record("test").await.is_err());
    }
    
    #[tokio::test]
    async fn test_rate_limit_status() {
        let service = RateLimitService::new();
        let config = RateLimitConfig::per_minute(5);
        
        service.set_config("test".to_string(), config).await;
        
        service.record("test").await.unwrap();
        service.record("test").await.unwrap();
        
        let status = service.get_status("test").await.unwrap();
        assert_eq!(status.current_requests, 2);
        assert_eq!(status.remaining, 2);
        assert!(status.allowed);
    }
    
    #[tokio::test]
    async fn test_rate_limit_reset() {
        let service = RateLimitService::new();
        let config = RateLimitConfig::per_minute(2);
        
        service.set_config("test".to_string(), config).await;
        
        service.record("test").await.unwrap();
        service.record("test").await.unwrap();
        assert!(service.record("test").await.is_err());
        
        service.reset("test").await;
        
        assert!(service.record("test").await.is_ok());
    }
    
    #[tokio::test]
    async fn test_user_rate_limit() {
        let service = RateLimitService::new();
        let user_id = Uuid::new_v4();
        let config = RateLimitConfig::per_minute(3);
        
        service.set_config(format!("user:{}:api", user_id), config).await;
        
        assert!(service.record_user(user_id, "api").await.is_ok());
        assert!(service.record_user(user_id, "api").await.is_ok());
        assert!(service.record_user(user_id, "api").await.is_ok());
        assert!(service.record_user(user_id, "api").await.is_err());
    }
}
