/// Tool Gateway Service
/// 
/// 工具网关服务，负责：
/// - 工具调用的请求/响应转换
/// - 多提供商支持（MCP HTTP/stdio, Plugin, Virtual）
/// - 会话管理和速率限制
/// - 内容守卫和敏感信息过滤
/// - 审计日志记录
/// 
/// 参考: paperclip/server/src/services/tool-gateway.ts (~6317 行)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

// ============================================================================
// 错误类型
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("validation error: {0}")]
    ValidationError(String),
    
    #[error("transformation error: {0}")]
    TransformationError(String),
    
    #[error("tool execution error: {0}")]
    ExecutionError(String),
    
    #[error("provider error: {0}")]
    ProviderError(String),
    
    #[error("rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("session error: {0}")]
    SessionError(String),
    
    #[error("authentication error: {0}")]
    AuthenticationError(String),
    
    #[error("authorization error: {0}")]
    AuthorizationError(String),
    
    #[error("timeout: {0}")]
    Timeout(String),
    
    #[error("content validation error: {0}")]
    ContentValidationError(String),
    
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub type GatewayResult<T> = Result<T, GatewayError>;

// ============================================================================
// 提供商类型
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolProviderType {
    /// MCP HTTP 测试固件
    McpHttpFixture,
    /// MCP stdio 测试固件
    McpStdioFixture,
    /// 远程 MCP HTTP 提供商
    McpRemoteHttp,
    /// 本地 MCP stdio 提供商
    McpLocalStdio,
    /// Paperclip 自身工具
    PaperclipSelf,
    /// Paperclip 插件工具
    PaperclipPlugin,
    /// 虚拟工具（测试/模拟）
    PaperclipVirtual,
}

// ============================================================================
// 工具风险级别
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolRiskLevel {
    Read,
    Write,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRiskInfo {
    pub level: ToolRiskLevel,
    pub is_read_only: bool,
    pub is_write: bool,
    pub is_destructive: bool,
    pub requires_approval: bool,
}

// ============================================================================
// 请求/响应数据结构
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub request_id: String,
    pub tool_name: String,
    pub parameters: HashMap<String, Value>,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub timeout_ms: Option<u64>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub request_id: String,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub metadata: HashMap<String, Value>,
    pub execution_time_ms: u64,
}

// ============================================================================
// 工具描述符
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub provider_type: ToolProviderType,
    pub risk: ToolRiskInfo,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    pub application_id: Option<String>,
    pub connection_id: Option<String>,
    pub catalog_entry_id: Option<String>,
    pub upstream_tool_name: Option<String>,
    pub annotations: HashMap<String, Value>,
}

// ============================================================================
// MCP 会话管理
// ============================================================================

#[derive(Debug, Clone)]
pub struct McpSession {
    pub session_id: String,
    pub gateway_id: Uuid,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
    pub token_hash: Option<String>,
    pub metadata: HashMap<String, Value>,
}

impl McpSession {
    pub fn new(gateway_id: Uuid, ttl_ms: u64) -> Self {
        let now = SystemTime::now();
        let expires_at = now + Duration::from_millis(ttl_ms);
        
        Self {
            session_id: Uuid::new_v4().to_string(),
            gateway_id,
            created_at: now,
            expires_at,
            token_hash: None,
            metadata: HashMap::new(),
        }
    }
    
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires_at
    }
    
    pub fn remaining_ttl_ms(&self) -> u64 {
        match self.expires_at.duration_since(SystemTime::now()) {
            Ok(duration) => duration.as_millis() as u64,
            Err(_) => 0,
        }
    }
}

// ============================================================================
// 速率限制
// ============================================================================

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub window_ms: u64,
    pub max_requests: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            window_ms: 60_000, // 1 分钟
            max_requests: 300,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitState {
    pub limited: bool,
    pub count: u32,
    pub retry_after_ms: u64,
    pub window_start: SystemTime,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            limited: false,
            count: 0,
            retry_after_ms: 0,
            window_start: SystemTime::now(),
        }
    }
    
    pub fn check(&mut self, config: &RateLimitConfig) -> bool {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.window_start).unwrap_or_default();
        
        // 窗口过期，重置计数
        if elapsed.as_millis() as u64 > config.window_ms {
            self.count = 0;
            self.window_start = now;
            self.limited = false;
        }
        
        // 检查是否超限
        if self.count >= config.max_requests {
            self.limited = true;
            self.retry_after_ms = config.window_ms - elapsed.as_millis() as u64;
            return false;
        }
        
        self.count += 1;
        true
    }
}

// ============================================================================
// 工具网关服务 Trait
// ============================================================================

#[async_trait]
pub trait ToolGatewayService: Send + Sync {
    /// 调用工具
    async fn call_tool(&self, request: ToolRequest) -> GatewayResult<ToolResponse>;
    
    /// 列出可用工具
    async fn list_tools(&self, agent_id: Option<Uuid>) -> GatewayResult<Vec<ToolDescriptor>>;
    
    /// 获取工具描述
    async fn get_tool(&self, tool_name: &str) -> GatewayResult<Option<ToolDescriptor>>;
    
    /// 创建 MCP 会话
    async fn create_session(&self, gateway_id: Uuid, ttl_ms: Option<u64>) -> GatewayResult<McpSession>;
    
    /// 验证会话
    async fn validate_session(&self, session_id: &str) -> GatewayResult<McpSession>;
    
    /// 销毁会话
    async fn destroy_session(&self, session_id: &str) -> GatewayResult<()>;
    
    /// 检查速率限制
    async fn check_rate_limit(&self, key: &str) -> GatewayResult<bool>;
    
    /// 记录审计日志
    async fn audit_tool_call(
        &self,
        request: &ToolRequest,
        response: &ToolResponse,
        risk: &ToolRiskInfo,
    ) -> GatewayResult<()>;
}

// ============================================================================
// 默认实现
// ============================================================================

pub struct ToolGatewayServiceImpl {
    pool: PgPool,
    rate_limits: parking_lot::RwLock<HashMap<String, RateLimitState>>,
    sessions: parking_lot::RwLock<HashMap<String, McpSession>>,
    default_timeout_ms: u64,
    default_session_ttl_ms: u64,
    rate_limit_config: RateLimitConfig,
}

impl ToolGatewayServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            rate_limits: parking_lot::RwLock::new(HashMap::new()),
            sessions: parking_lot::RwLock::new(HashMap::new()),
            default_timeout_ms: 10_000,
            default_session_ttl_ms: 15 * 60 * 1000, // 15 分钟
            rate_limit_config: RateLimitConfig::default(),
        }
    }
    
    /// 验证请求
    fn validate_request(&self, request: &ToolRequest) -> GatewayResult<()> {
        if request.tool_name.is_empty() {
            return Err(GatewayError::ValidationError("tool_name is required".to_string()));
        }
        
        if request.request_id.is_empty() {
            return Err(GatewayError::ValidationError("request_id is required".to_string()));
        }
        
        Ok(())
    }
    
    /// 转换请求参数
    fn transform_request(&self, request: &ToolRequest) -> GatewayResult<HashMap<String, Value>> {
        // 基础参数验证和转换
        let mut transformed = request.parameters.clone();
        
        // 规范化参数（例如：统一日期格式、数字类型等）
        for (key, value) in transformed.iter_mut() {
            // 示例：移除 null 值
            if value.is_null() {
                continue;
            }
            
            // 可以添加更多转换逻辑
            match key.as_str() {
                "timeout" | "timeout_ms" => {
                    // 确保超时值是合理的
                    if let Some(n) = value.as_u64() {
                        if n > 60_000 {
                            *value = Value::from(60_000u64);
                        }
                    }
                }
                _ => {}
            }
        }
        
        Ok(transformed)
    }
    
    /// 转换响应
    fn transform_response(
        &self,
        result: Result<Value, String>,
        request_id: String,
        execution_time_ms: u64,
    ) -> ToolResponse {
        match result {
            Ok(value) => ToolResponse {
                request_id,
                success: true,
                result: Some(value),
                error: None,
                metadata: HashMap::new(),
                execution_time_ms,
            },
            Err(error) => ToolResponse {
                request_id,
                success: false,
                result: None,
                error: Some(error),
                metadata: HashMap::new(),
                execution_time_ms,
            },
        }
    }
    
    /// 过滤敏感信息
    fn filter_sensitive_data(&self, mut response: ToolResponse) -> ToolResponse {
        if let Some(Value::Object(ref mut map)) = response.result {
            // 移除常见的敏感字段
            let sensitive_keys = [
                "password", "token", "secret", "api_key", "apiKey",
                "private_key", "privateKey", "access_token", "accessToken",
                "refresh_token", "refreshToken", "bearer", "authorization",
                "credential", "credentials", "auth", "session",
            ];
            
            for key in &sensitive_keys {
                map.remove(*key);
            }
            
            // 递归过滤嵌套对象
            for (_key, value) in map.iter_mut() {
                if let Value::Object(nested) = value {
                    for sensitive_key in &sensitive_keys {
                        nested.remove(*sensitive_key);
                    }
                }
            }
        }
        
        response
    }
    
    /// 评估工具风险
    fn assess_tool_risk(&self, tool_name: &str) -> ToolRiskInfo {
        // 基于工具名称或元数据评估风险
        // 这里是简化的实现，实际应该从数据库或配置中读取
        
        let is_write = tool_name.contains("write") || 
                       tool_name.contains("create") || 
                       tool_name.contains("update") ||
                       tool_name.contains("delete");
        
        let is_destructive = tool_name.contains("delete") || 
                             tool_name.contains("destroy") ||
                             tool_name.contains("remove");
        
        let level = if is_destructive {
            ToolRiskLevel::Destructive
        } else if is_write {
            ToolRiskLevel::Write
        } else {
            ToolRiskLevel::Read
        };
        
        ToolRiskInfo {
            level: level.clone(),
            is_read_only: matches!(level, ToolRiskLevel::Read),
            is_write,
            is_destructive,
            requires_approval: is_destructive,
        }
    }
    
    /// 执行实际的工具调用（简化实现）
    async fn execute_tool_internal(
        &self,
        tool_name: &str,
        parameters: HashMap<String, Value>,
        _timeout_ms: u64,
    ) -> Result<Value, String> {
        // TODO: 这里应该路由到实际的工具提供商
        // 根据 tool_name 查找提供商类型，然后调用相应的执行器
        
        // 目前返回模拟结果
        Ok(serde_json::json!({
            "tool": tool_name,
            "parameters": parameters,
            "result": "mock_execution_result",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    }
}

#[async_trait]
impl ToolGatewayService for ToolGatewayServiceImpl {
    async fn call_tool(&self, request: ToolRequest) -> GatewayResult<ToolResponse> {
        let start = SystemTime::now();
        
        // 1. 验证请求
        self.validate_request(&request)?;
        
        // 2. 检查速率限制
        let rate_limit_key = request.agent_id
            .map(|id| format!("agent:{}", id))
            .unwrap_or_else(|| format!("request:{}", request.request_id));
        
        if !self.check_rate_limit(&rate_limit_key).await? {
            return Err(GatewayError::RateLimitExceeded(
                "Rate limit exceeded, please try again later".to_string()
            ));
        }
        
        // 3. 评估工具风险
        let risk = self.assess_tool_risk(&request.tool_name);
        
        // 4. 转换参数
        let transformed_params = self.transform_request(&request)?;
        
        // 5. 执行工具调用
        let timeout_ms = request.timeout_ms.unwrap_or(self.default_timeout_ms);
        let result = self.execute_tool_internal(
            &request.tool_name,
            transformed_params,
            timeout_ms,
        ).await;
        
        // 6. 计算执行时间
        let execution_time_ms = start.elapsed()
            .unwrap_or_default()
            .as_millis() as u64;
        
        // 7. 转换响应
        let mut response = self.transform_response(
            result,
            request.request_id.clone(),
            execution_time_ms,
        );
        
        // 8. 过滤敏感信息
        response = self.filter_sensitive_data(response);
        
        // 9. 记录审计日志
        let _ = self.audit_tool_call(&request, &response, &risk).await;
        
        Ok(response)
    }
    
    async fn list_tools(&self, _agent_id: Option<Uuid>) -> GatewayResult<Vec<ToolDescriptor>> {
        // TODO: 从数据库或注册表中读取可用工具列表
        // 应该根据 agent_id 过滤工具权限
        
        Ok(vec![
            // 示例工具
            ToolDescriptor {
                name: "echo".to_string(),
                description: "Echo back the input".to_string(),
                provider_type: ToolProviderType::PaperclipSelf,
                risk: ToolRiskInfo {
                    level: ToolRiskLevel::Read,
                    is_read_only: true,
                    is_write: false,
                    is_destructive: false,
                    requires_approval: false,
                },
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"]
                }),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "echo": { "type": "string" }
                    }
                })),
                application_id: None,
                connection_id: None,
                catalog_entry_id: None,
                upstream_tool_name: None,
                annotations: HashMap::new(),
            },
        ])
    }
    
    async fn get_tool(&self, tool_name: &str) -> GatewayResult<Option<ToolDescriptor>> {
        let tools = self.list_tools(None).await?;
        Ok(tools.into_iter().find(|t| t.name == tool_name))
    }
    
    async fn create_session(&self, gateway_id: Uuid, ttl_ms: Option<u64>) -> GatewayResult<McpSession> {
        let ttl = ttl_ms.unwrap_or(self.default_session_ttl_ms);
        let session = McpSession::new(gateway_id, ttl);
        
        // 存储会话
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(session.session_id.clone(), session.clone());
        
        Ok(session)
    }
    
    async fn validate_session(&self, session_id: &str) -> GatewayResult<McpSession> {
        let sessions = self.sessions.read().unwrap();
        
        let session = sessions
            .get(session_id)
            .ok_or_else(|| GatewayError::SessionError("Session not found".to_string()))?;
        
        if session.is_expired() {
            return Err(GatewayError::SessionError("Session expired".to_string()));
        }
        
        Ok(session.clone())
    }
    
    async fn destroy_session(&self, session_id: &str) -> GatewayResult<()> {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(session_id);
        Ok(())
    }
    
    async fn check_rate_limit(&self, key: &str) -> GatewayResult<bool> {
        let mut limits = self.rate_limits.write().unwrap();
        
        let state = limits
            .entry(key.to_string())
            .or_insert_with(RateLimitState::new);
        
        Ok(state.check(&self.rate_limit_config))
    }
    
    async fn audit_tool_call(
        &self,
        request: &ToolRequest,
        response: &ToolResponse,
        risk: &ToolRiskInfo,
    ) -> GatewayResult<()> {
        // TODO: 将审计日志写入数据库
        // 包括：request_id, tool_name, agent_id, run_id, risk_level, 
        //      success, execution_time_ms, timestamp
        
        let _audit_entry = serde_json::json!({
            "request_id": request.request_id,
            "tool_name": request.tool_name,
            "agent_id": request.agent_id,
            "run_id": request.run_id,
            "risk_level": format!("{:?}", risk.level),
            "success": response.success,
            "execution_time_ms": response.execution_time_ms,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        // 简化实现：只在控制台输出
        println!("Tool call audit: {}", request.tool_name);
        
        Ok(())
    }
}

impl Default for ToolGatewayServiceImpl {
    fn default() -> Self {
        // 这个实现需要一个 PgPool，所以 Default 实际上不太合理
        // 但为了满足某些测试场景，提供一个占位实现
        panic!("ToolGatewayServiceImpl requires a PgPool and cannot use Default::default()")
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 规范化工具参数（移除 null、空字符串等）
pub fn canonicalize_tool_arguments(args: HashMap<String, Value>) -> HashMap<String, Value> {
    args.into_iter()
        .filter(|(_, v)| !v.is_null())
        .filter(|(_, v)| {
            if let Value::String(s) = v {
                !s.is_empty()
            } else {
                true
            }
        })
        .collect()
}

/// 摘要化工具值（用于日志和审计）
pub fn summarize_tool_value(value: &Value, max_length: usize) -> String {
    let s = value.to_string();
    if s.len() <= max_length {
        s
    } else {
        format!("{}... ({} bytes)", &s[..max_length], s.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_expiry() {
        let session = McpSession::new(Uuid::new_v4(), 1000);
        assert!(!session.is_expired());
        
        std::thread::sleep(Duration::from_millis(1100));
        assert!(session.is_expired());
    }
    
    #[test]
    fn test_rate_limit() {
        let config = RateLimitConfig {
            window_ms: 1000,
            max_requests: 3,
        };
        
        let mut state = RateLimitState::new();
        
        assert!(state.check(&config)); // 1
        assert!(state.check(&config)); // 2
        assert!(state.check(&config)); // 3
        assert!(!state.check(&config)); // 超限
    }
    
    #[test]
    fn test_canonicalize_tool_arguments() {
        let mut args = HashMap::new();
        args.insert("key1".to_string(), Value::String("value1".to_string()));
        args.insert("key2".to_string(), Value::Null);
        args.insert("key3".to_string(), Value::String("".to_string()));
        
        let result = canonicalize_tool_arguments(args);
        
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("key1"));
    }
    
    #[test]
    fn test_summarize_tool_value() {
        let long_value = Value::String("a".repeat(200));
        let summary = summarize_tool_value(&long_value, 50);
        
        assert!(summary.len() < 200);
        assert!(summary.contains("..."));
        assert!(summary.contains("bytes"));
    }
}
