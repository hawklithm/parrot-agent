//! 速率限制中间件（对应任务拆解 §10 阶段二「实现速率限制」）。
//!
//! 基于 IP 和 API Key 的轻量级内存速率限制器。
//! 使用滑动窗口算法实现。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tokio::sync::Mutex;

use crate::auth::AuthError;

/// 速率限制配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 全局每分钟请求限制
    pub global_per_minute: u32,
    /// 基于 IP 的每分钟请求限制
    pub ip_per_minute: u32,
    /// 基于 API Key 的每分钟请求限制
    pub api_key_per_minute: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            global_per_minute: 10000,
            ip_per_minute: 100,
            api_key_per_minute: 500,
        }
    }
}

/// 滑动窗口记录
#[derive(Debug, Clone)]
struct SlidingWindow {
    timestamps: Vec<Instant>,
}

impl SlidingWindow {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    /// 清理过期的时间戳并检查是否超过限制
    fn check_and_record(&mut self, max_requests: u32, window: Duration) -> bool {
        let now = Instant::now();
        let cutoff = now - window;

        // 移除过期的时间戳
        self.timestamps.retain(|&t| t > cutoff);

        // 检查是否超过限制
        if self.timestamps.len() >= max_requests as usize {
            return false;
        }

        // 记录当前请求
        self.timestamps.push(now);
        true
    }
}

/// 速率限制器
#[derive(Debug, Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    ip_windows: Arc<Mutex<HashMap<String, SlidingWindow>>>,
    key_windows: Arc<Mutex<HashMap<String, SlidingWindow>>>,
    global_window: Arc<Mutex<SlidingWindow>>,
}

impl RateLimiter {
    /// 创建新的速率限制器
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_windows: Arc::new(Mutex::new(HashMap::new())),
            key_windows: Arc::new(Mutex::new(HashMap::new())),
            global_window: Arc::new(Mutex::new(SlidingWindow::new())),
        }
    }

    /// 检查请求是否被限流
    pub async fn check_rate_limit(
        &self,
        ip: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<(), RateLimitError> {
        let window = Duration::from_secs(60);

        // 全局限制检查
        {
            let mut global = self.global_window.lock().await;
            if !global.check_and_record(self.config.global_per_minute, window) {
                return Err(RateLimitError::GlobalLimitExceeded);
            }
        }

        // IP 限制检查
        if let Some(ip) = ip {
            let mut ip_map = self.ip_windows.lock().await;
            let entry = ip_map
                .entry(ip.to_string())
                .or_insert_with(SlidingWindow::new);
            if !entry.check_and_record(self.config.ip_per_minute, window) {
                return Err(RateLimitError::IpLimitExceeded);
            }
        }

        // API Key 限制检查
        if let Some(key) = api_key {
            let mut key_map = self.key_windows.lock().await;
            let entry = key_map
                .entry(key.to_string())
                .or_insert_with(SlidingWindow::new);
            if !entry.check_and_record(self.config.api_key_per_minute, window) {
                return Err(RateLimitError::ApiKeyLimitExceeded);
            }
        }

        Ok(())
    }

    /// 定期清理过期的窗口记录（防止内存泄漏）
    pub async fn cleanup(&self) {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(120); // 保留最近2分钟

        let mut ip_map = self.ip_windows.lock().await;
        ip_map.retain(|_, window| {
            window.timestamps.retain(|&t| t > cutoff);
            !window.timestamps.is_empty()
        });

        let mut key_map = self.key_windows.lock().await;
        key_map.retain(|_, window| {
            window.timestamps.retain(|&t| t > cutoff);
            !window.timestamps.is_empty()
        });
    }
}

/// 速率限制错误
#[derive(Debug, Clone)]
pub enum RateLimitError {
    /// 全局限制超出
    GlobalLimitExceeded,
    /// IP 限制超出
    IpLimitExceeded,
    /// API Key 限制超出
    ApiKeyLimitExceeded,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            RateLimitError::GlobalLimitExceeded => {
                (StatusCode::TOO_MANY_REQUESTS, "Global rate limit exceeded".to_string())
            }
            RateLimitError::IpLimitExceeded => {
                (StatusCode::TOO_MANY_REQUESTS, "IP rate limit exceeded. Please try again later.".to_string())
            }
            RateLimitError::ApiKeyLimitExceeded => {
                (StatusCode::TOO_MANY_REQUESTS, "API key rate limit exceeded".to_string())
            }
        };

        (status, Json(json!({
            "error": message,
            "code": "rate_limit_exceeded",
            "retryAfter": 60,
        })))
            .into_response()
    }
}

/// axum 速率限制中间件
pub async fn rate_limit_middleware(
    State(limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim());

    let api_key = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    limiter
        .check_rate_limit(ip, api_key.as_deref())
        .await?;

    Ok(next.run(request).await)
}
