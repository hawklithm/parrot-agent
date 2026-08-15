/// 工具运行时监控
/// 
/// 监控工具调用，实现超时控制、异常捕获和熔断机制

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("timeout: {0}")]
    Timeout(String),
    
    #[error("circuit breaker open: {0}")]
    CircuitBreakerOpen(String),
    
    #[error("execution error: {0}")]
    ExecutionError(String),
}

pub type SupervisorResult<T> = Result<T, SupervisorError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<chrono::DateTime<chrono::Utc>>,
    pub threshold: u32,
}

impl CircuitBreaker {
    pub fn new(threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            threshold,
        }
    }
    
    pub fn record_success(&mut self) {
        self.success_count += 1;
        
        if matches!(self.state, CircuitState::HalfOpen) && self.success_count >= 3 {
            self.state = CircuitState::Closed;
            self.failure_count = 0;
        }
    }
    
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(chrono::Utc::now());
        
        if self.failure_count >= self.threshold {
            self.state = CircuitState::Open;
        }
    }
    
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否应该转换到半开状态
                if let Some(last_failure) = self.last_failure_time {
                    let elapsed = chrono::Utc::now() - last_failure;
                    if elapsed.num_seconds() > 60 {
                        self.state = CircuitState::HalfOpen;
                        self.success_count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetrics {
    pub tool_name: String,
    pub call_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub timeout_count: u64,
    pub total_duration_ms: u64,
}

/// 工具运行时监控服务
pub struct ToolRuntimeSupervisor {
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    metrics: Arc<RwLock<HashMap<String, ToolMetrics>>>,
    default_timeout: Duration,
}

impl ToolRuntimeSupervisor {
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            default_timeout,
        }
    }
    
    /// 检查是否可以执行工具
    pub async fn can_execute(&self, tool_name: &str) -> bool {
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers.entry(tool_name.to_string())
            .or_insert_with(|| CircuitBreaker::new(5));
        
        breaker.can_execute()
    }
    
    /// 记录成功调用
    pub async fn record_success(&self, tool_name: &str, duration_ms: u64) {
        // 更新熔断器
        let mut breakers = self.circuit_breakers.write().await;
        if let Some(breaker) = breakers.get_mut(tool_name) {
            breaker.record_success();
        }
        
        // 更新指标
        let mut metrics = self.metrics.write().await;
        let metric = metrics.entry(tool_name.to_string())
            .or_insert_with(|| ToolMetrics {
                tool_name: tool_name.to_string(),
                call_count: 0,
                success_count: 0,
                failure_count: 0,
                timeout_count: 0,
                total_duration_ms: 0,
            });
        
        metric.call_count += 1;
        metric.success_count += 1;
        metric.total_duration_ms += duration_ms;
    }
    
    /// 记录失败调用
    pub async fn record_failure(&self, tool_name: &str, is_timeout: bool) {
        // 更新熔断器
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers.entry(tool_name.to_string())
            .or_insert_with(|| CircuitBreaker::new(5));
        breaker.record_failure();
        
        // 更新指标
        let mut metrics = self.metrics.write().await;
        let metric = metrics.entry(tool_name.to_string())
            .or_insert_with(|| ToolMetrics {
                tool_name: tool_name.to_string(),
                call_count: 0,
                success_count: 0,
                failure_count: 0,
                timeout_count: 0,
                total_duration_ms: 0,
            });
        
        metric.call_count += 1;
        metric.failure_count += 1;
        if is_timeout {
            metric.timeout_count += 1;
        }
    }
    
    /// 获取工具指标
    pub async fn get_metrics(&self, tool_name: &str) -> Option<ToolMetrics> {
        self.metrics.read().await.get(tool_name).cloned()
    }
    
    /// 获取所有工具指标
    pub async fn get_all_metrics(&self) -> Vec<ToolMetrics> {
        self.metrics.read().await.values().cloned().collect()
    }
    
    /// 重置熔断器
    pub async fn reset_circuit_breaker(&self, tool_name: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        breakers.remove(tool_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_circuit_breaker() {
        let supervisor = ToolRuntimeSupervisor::new(Duration::from_secs(30));
        
        assert!(supervisor.can_execute("test_tool").await);
        
        // 记录5次失败，触发熔断
        for _ in 0..5 {
            supervisor.record_failure("test_tool", false).await;
        }
        
        assert!(!supervisor.can_execute("test_tool").await);
    }
    
    #[tokio::test]
    async fn test_metrics() {
        let supervisor = ToolRuntimeSupervisor::new(Duration::from_secs(30));
        
        supervisor.record_success("test_tool", 100).await;
        supervisor.record_success("test_tool", 200).await;
        supervisor.record_failure("test_tool", false).await;
        
        let metrics = supervisor.get_metrics("test_tool").await.unwrap();
        assert_eq!(metrics.call_count, 3);
        assert_eq!(metrics.success_count, 2);
        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.total_duration_ms, 300);
    }
}
