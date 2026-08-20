/// Plugin Tool Dispatcher
/// 
/// 增强版工具调度器，负责：
/// - 工具调用路由
/// - 参数验证和转换
/// - 结果序列化
/// - 错误传播和处理

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use super::plugin_worker_manager::{PluginWorkerManager, WorkerError};

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("plugin not available: {0}")]
    PluginNotAvailable(Uuid),
    
    #[error("parameter validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("serialization error: {0}")]
    SerializationError(String),
    
    #[error("execution error: {0}")]
    ExecutionError(String),
    
    #[error("timeout: tool execution exceeded {0}ms")]
    Timeout(u64),
}

pub type DispatchResult<T> = Result<T, DispatchError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub plugin_id: Uuid,
    pub parameters: HashMap<String, Value>,
    pub timeout_ms: Option<u64>,
    pub agent_id: Uuid,
    pub call_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: Uuid,
    pub success: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterSchema {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: Option<String>,
    pub default: Option<Value>,
}

/// 增强版工具调度器
pub struct PluginToolDispatcher {
    worker_manager: Option<Arc<PluginWorkerManager>>,
}

impl PluginToolDispatcher {
    pub fn new(worker_manager: Option<Arc<PluginWorkerManager>>) -> Self {
        Self { worker_manager }
    }
    
    /// 验证调用参数
    pub fn validate_parameters(
        &self,
        params: &HashMap<String, Value>,
        schema: &[ParameterSchema],
    ) -> DispatchResult<()> {
        // 检查必需参数
        for param_schema in schema {
            if param_schema.required && !params.contains_key(&param_schema.name) {
                return Err(DispatchError::ValidationFailed(format!(
                    "missing required parameter: {}",
                    param_schema.name
                )));
            }
        }
        
        // 检查参数类型
        for (name, value) in params {
            if let Some(schema) = schema.iter().find(|s| &s.name == name) {
                if !self.validate_type(value, &schema.param_type) {
                    return Err(DispatchError::ValidationFailed(format!(
                        "parameter '{}' has invalid type, expected {}",
                        name, schema.param_type
                    )));
                }
            }
        }
        
        Ok(())
    }
    
    fn validate_type(&self, value: &Value, expected_type: &str) -> bool {
        match expected_type {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => true, // 未知类型不验证
        }
    }
    
    /// 转换参数
    pub fn transform_parameters(
        &self,
        params: HashMap<String, Value>,
        _schema: &[ParameterSchema],
    ) -> DispatchResult<HashMap<String, Value>> {
        let mut transformed = params;
        for parameter in _schema {
            if !transformed.contains_key(&parameter.name) {
                if let Some(default) = &parameter.default {
                    transformed.insert(parameter.name.clone(), default.clone());
                }
            }
        }
        Ok(transformed)
    }
    
    /// 执行工具调用
    pub async fn execute_tool(&self, call: &ToolCall) -> DispatchResult<ToolResult> {
        let start_time = std::time::Instant::now();
        
        // 1. 检查 worker_manager 是否可用
        let worker_manager = self.worker_manager.as_ref()
            .ok_or_else(|| DispatchError::ExecutionError(
                "worker_manager not configured".to_string()
            ))?;
        
        // 2. 验证 worker 是否运行
        if !worker_manager.is_running(&call.plugin_id).await {
            return Err(DispatchError::PluginNotAvailable(call.plugin_id));
        }
        
        // 3. 准备 RPC 参数
        let rpc_params = serde_json::json!({
            "toolName": call.tool_name,
            "parameters": call.parameters,
            "runContext": {
                "agentId": call.agent_id.to_string(),
                "callId": call.call_id.to_string(),
            }
        });
        
        // 4. 通过 IPC 调用 worker 的 executeTool 方法
        let result = worker_manager
            .call(
                &call.plugin_id,
                "executeTool",
                rpc_params,
                call.timeout_ms,
            )
            .await
            .map_err(|e| match e {
                WorkerError::RpcTimeout { timeout_ms, .. } => {
                    DispatchError::Timeout(timeout_ms)
                }
                WorkerError::NotRunning(_) => {
                    DispatchError::PluginNotAvailable(call.plugin_id)
                }
                _ => DispatchError::ExecutionError(e.to_string()),
            })?;
        
        // 5. 计算执行时间
        let execution_time_ms = start_time.elapsed().as_millis() as u64;
        
        // 6. 解析结果
        let success = result.get("error").is_none();
        let error = result.get("error")
            .and_then(|e| e.as_str())
            .map(|s| s.to_string());
        
        // 7. 构造返回结果
        Ok(ToolResult {
            call_id: call.call_id,
          success,
            result: Some(result),
            error,
            execution_time_ms,
            metadata: HashMap::new(),
        })
    }
    
    /// 序列化结果
    
    /// 将错误传播为工具结果
    pub fn propagate_error(&self, error: DispatchError) -> ToolResult {
        ToolResult {
            call_id: uuid::Uuid::new_v4(),
            success: false,
            result: None,
            error: Some(error.to_string()),
            execution_time_ms: 0,
            metadata: HashMap::new(),
        }
    }

    pub fn serialize_result(&self, result: &ToolResult) -> DispatchResult<Value> {
        serde_json::to_value(result)
            .map_err(|error| DispatchError::SerializationError(error.to_string()))
    }
}

impl Default for PluginToolDispatcher {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parameter_validation() {
        let dispatcher = PluginToolDispatcher::new(None);
        
        let schema = vec![
            ParameterSchema {
                name: "query".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: None,
                default: None,
            },
            ParameterSchema {
                name: "limit".to_string(),
                param_type: "number".to_string(),
                required: false,
                description: None,
                default: Some(Value::Number(10.into())),
            },
        ];
        
        let mut params = HashMap::new();
        params.insert("query".to_string(), Value::String("test".to_string()));
        
        assert!(dispatcher.validate_parameters(&params, &schema).is_ok());
        
        // 缺少必需参数
        let empty_params = HashMap::new();
        assert!(dispatcher.validate_parameters(&empty_params, &schema).is_err());
    }
    
    #[test]
    fn test_parameter_transformation() {
        let dispatcher = PluginToolDispatcher::new(None);
        
        let schema = vec![
            ParameterSchema {
                name: "limit".to_string(),
                param_type: "number".to_string(),
                required: false,
                description: None,
                default: Some(Value::Number(10.into())),
            },
        ];
        
        let params = HashMap::new();
        let transformed = dispatcher.transform_parameters(params, &schema).unwrap();
        
        assert!(transformed.contains_key("limit"));
        assert_eq!(transformed.get("limit"), Some(&Value::Number(10.into())));
    }
    
    #[test]
    fn test_result_serialization() {
        let dispatcher = PluginToolDispatcher::new(None);
        
        let result = ToolResult {
            call_id: Uuid::new_v4(),
            success: true,
            result: Some(Value::String("success".to_string())),
            error: None,
            execution_time_ms: 100,
            metadata: HashMap::new(),
        };
        
        let serialized = dispatcher.serialize_result(&result);
        assert!(serialized.is_ok());
    }
    
    #[test]
    fn test_error_propagation() {
        let dispatcher = PluginToolDispatcher::new(None);
        
        let error = DispatchError::ToolNotFound("test-tool".to_string());
        let result = dispatcher.propagate_error(error);
        
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
