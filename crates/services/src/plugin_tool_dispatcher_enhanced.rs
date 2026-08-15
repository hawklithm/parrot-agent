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
use uuid::Uuid;

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
}

/// 增强版工具调度器
pub struct PluginToolDispatcher {
    // 简化实现，实际应该包含工具注册表和worker管理器
}

impl PluginToolDispatcher {
    pub fn new() -> Self {
        Self {}
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
        // ParameterSchema 暂不支持默认值，直接返回原参数
        Ok(params)
    }
    
    async fn execute_tool(&self, call: &ToolCall) -> DispatchResult<Value> {
        // 模拟工具执行
        // 实际实现应该：
        // 1. 查找工具所属的plugin
        // 2. 获取worker进程
        // 3. 通过IPC发送调用请求
        // 4. 等待结果并处理超时
        
        Ok(Value::Object(serde_json::Map::new()))
    }
    
    /// 序列化结果
    pub fn serialize_result(&self, result: &ToolResult) -> DispatchResult<String> {
        serde_json::to_string(result)
            .map_err(|e| DispatchError::SerializationError(e.to_string()))
    }
    
    /// 反序列化参数
    pub fn deserialize_parameters(&self, json: &str) -> DispatchResult<HashMap<String, Value>> {
        serde_json::from_str(json)
            .map_err(|e| DispatchError::SerializationError(e.to_string()))
    }
    
    /// 传播错误
    pub fn propagate_error(&self, error: DispatchError) -> ToolResult {
        ToolResult {
            call_id: Uuid::new_v4(),
            success: false,
            result: None,
            error: Some(error.to_string()),
            execution_time_ms: 0,
            metadata: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parameter_validation() {
        let dispatcher = PluginToolDispatcher::new();
        
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
        let dispatcher = PluginToolDispatcher::new();
        
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
        let dispatcher = PluginToolDispatcher::new();
        
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
        let dispatcher = PluginToolDispatcher::new();
        
        let error = DispatchError::ToolNotFound("test-tool".to_string());
        let result = dispatcher.propagate_error(error);
        
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}
