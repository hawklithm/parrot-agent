//! 全局错误恢复策略
//!
//! 提供 RetryPolicy, CircuitBreaker, Fallback 等错误恢复基础设施
//! 对应 cross-module-integration-tasks.md §5 全局错误恢复策略

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

// ============================================================================
// 重试策略
// ============================================================================

/// 回退策略
#[derive(Debug, Clone, Copy)]
pub enum BackoffStrategy {
    /// 固定间隔
    Fixed { interval_ms: u64 },
    /// 指数退避：base * multiplier^attempt
    Exponential { base_ms: u64, multiplier: f64, max_ms: u64 },
    /// 斐波那契退避
    Fibonacci { base_ms: u64, max_ms: u64 },
}

impl BackoffStrategy {
    /// 计算第 attempt 次重试的等待时间（attempt 从 0 开始）
    pub fn delay_ms(&self, attempt: u32) -> u64 {
        match self {
            BackoffStrategy::Fixed { interval_ms } => *interval_ms,
            BackoffStrategy::Exponential { base_ms, multiplier, max_ms } => {
                let delay = (*base_ms as f64 * multiplier.powi(attempt as i32)).round() as u64;
                delay.min(*max_ms)
            }
            BackoffStrategy::Fibonacci { base_ms, max_ms } => {
                let fib = fibonacci(attempt as usize);
                let delay = *base_ms * fib as u64;
                delay.min(*max_ms)
            }
        }
    }
}

fn fibonacci(n: usize) -> usize {
    match n {
        0 => 1,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

/// 重试策略
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 回退策略
    pub backoff: BackoffStrategy,
    /// 超时时间（毫秒）
    pub timeout_ms: u64,
}

impl RetryPolicy {
    pub fn default_db() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential { base_ms: 100, multiplier: 2.0, max_ms: 5000 },
            timeout_ms: 30_000,
        }
    }

    pub fn default_api() -> Self {
        Self {
            max_attempts: 5,
            backoff: BackoffStrategy::Exponential { base_ms: 200, multiplier: 2.0, max_ms: 30_000 },
            timeout_ms: 60_000,
        }
    }

    pub fn default_environment() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Fixed { interval_ms: 5000 },
            timeout_ms: 120_000,
        }
    }
}

/// 可重试错误 trait
pub trait RetryableError: std::error::Error {
    /// 该错误是否值得重试
    fn is_retryable(&self) -> bool;
}

/// 使用重试策略执行异步操作
pub async fn with_retry<F, Fut, T, E>(policy: &RetryPolicy, f: F) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: RetryableError,
{
    let mut last_error = None;

    for attempt in 0..policy.max_attempts {
        // 非第一次重试时等待
        if attempt > 0 {
            let delay_ms = policy.backoff.delay_ms(attempt - 1);
            sleep(Duration::from_millis(delay_ms)).await;
        }

        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() {
                    return Err(e);
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error.expect("retry loop should have at least one error"))
}

// ============================================================================
// 熔断器（Circuit Breaker）
// ============================================================================

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 正常——请求通过
    Closed,
    /// 熔断——请求被拒绝
    Open,
    /// 半开——允许少量请求测试
    HalfOpen,
}

/// 熔断器
pub struct CircuitBreaker {
    state: RwLock<CircuitState>,
    failure_count: AtomicUsize,
    success_count: AtomicUsize,
    /// 连续失败阈值（达到此值后熔断）
    threshold: usize,
    /// 熔断持续时间（毫秒）
    timeout_ms: u64,
    /// 半开状态下的成功恢复次数
    half_open_success_threshold: usize,
    /// 上次熔断时间
    last_open_time: RwLock<tokio::time::Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: usize, timeout_ms: u64) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            threshold,
            timeout_ms,
            half_open_success_threshold: 1,
            last_open_time: RwLock::new(tokio::time::Instant::now()),
        }
    }

    /// 检查请求是否可以通过
    pub async fn is_allowed(&self) -> bool {
        let state = *self.state.read().await;
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // 检查是否超过熔断时间
                let elapsed = self.last_open_time.read().await.elapsed();
                if elapsed >= Duration::from_millis(self.timeout_ms) {
                    // 进入半开状态
                    let mut state = self.state.write().await;
                    *state = CircuitState::HalfOpen;
                    self.success_count.store(0, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // 半开状态只允许有限请求
                self.success_count.load(Ordering::SeqCst) < self.half_open_success_threshold
            }
        }
    }

    /// 记录成功
    pub async fn on_success(&self) {
        let mut state = self.state.write().await;
        self.failure_count.store(0, Ordering::SeqCst);

        if *state == CircuitState::HalfOpen {
            let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
            if successes >= self.half_open_success_threshold {
                *state = CircuitState::Closed;
                self.success_count.store(0, Ordering::SeqCst);
            }
        }
    }

    /// 记录失败
    pub async fn on_failure(&self) {
        if *self.state.read().await == CircuitState::HalfOpen {
            // 半开状态下失败，立即回到打开状态
            let mut state = self.state.write().await;
            *state = CircuitState::Open;
            *self.last_open_time.write().await = tokio::time::Instant::now();
            self.success_count.store(0, Ordering::SeqCst);
            return;
        }

        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= self.threshold {
            let mut state = self.state.write().await;
            *state = CircuitState::Open;
            *self.last_open_time.write().await = tokio::time::Instant::now();
        }
    }

    /// 获取当前状态
    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// 重置熔断器
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
    }
}

// ============================================================================
// 降级策略（Fallback）
// ============================================================================

/// 降级结果
pub enum FallbackResult<T> {
    /// 正常结果
    Primary(T),
    /// 降级结果
    Degraded(T),
    /// 无法降级
    Unavailable,
}

/// 降级策略 trait
#[async_trait::async_trait]
pub trait Fallback<T>: Send + Sync {
    /// 当主操作失败时，尝试降级
    async fn fallback(&self, error: &str) -> FallbackResult<T>;
}

// ============================================================================
// 错误追踪
// ============================================================================

/// 错误追踪记录
#[derive(Debug, Clone)]
pub struct ErrorTrace {
    pub correlation_id: String,
    pub errors: Vec<String>,
    pub service: String,
    pub operation: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ErrorTrace {
    pub fn new(service: &str, operation: &str) -> Self {
        Self {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            errors: Vec::new(),
            service: service.to_string(),
            operation: operation.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
}
