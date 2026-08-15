/// Plugin IPC Protocol
/// 
/// 实现 Worker 进程间通信协议（JSON-RPC 2.0）

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(method: String, params: HashMap<String, Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Uuid::new_v4().to_string(),
            method,
            params,
        }
    }
}

impl JsonRpcResponse {
    pub fn success(id: String, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: String, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }
}

/// IPC 传输层
#[derive(Debug)]
pub struct IpcTransport {
    /// 待发送的请求队列
    pending_requests: HashMap<String, JsonRpcRequest>,
}

impl IpcTransport {
    pub fn new() -> Self {
        Self {
            pending_requests: HashMap::new(),
        }
    }

    /// 发送请求
    pub fn send_request(&mut self, request: JsonRpcRequest) -> Result<(), String> {
        self.pending_requests.insert(request.id.clone(), request);
        Ok(())
    }

    /// 接收响应
    pub fn receive_response(&mut self, response: JsonRpcResponse) -> Result<Value, String> {
        if let Some(_req) = self.pending_requests.remove(&response.id) {
            if let Some(error) = response.error {
                return Err(error.message);
            }
            response.result.ok_or_else(|| "No result in response".to_string())
        } else {
            Err("Unknown request ID".to_string())
        }
    }
}

impl Default for IpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request() {
        let mut params = HashMap::new();
        params.insert("arg1".to_string(), Value::String("value1".to_string()));
        
        let request = JsonRpcRequest::new("test_method".to_string(), params);
        
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "test_method");
        assert!(!request.id.is_empty());
    }

    #[test]
    fn test_json_rpc_response_success() {
        let response = JsonRpcResponse::success(
            "test-id".to_string(),
            Value::String("result".to_string()),
        );
        
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_response_error() {
        let response = JsonRpcResponse::error(
            "test-id".to_string(),
            -32600,
            "Invalid Request".to_string(),
        );
        
        assert!(response.result.is_none());
        assert!(response.error.is_some());
    }

    #[test]
    fn test_ipc_transport() {
        let mut transport = IpcTransport::new();
        
        let request = JsonRpcRequest::new(
            "test".to_string(),
            HashMap::new(),
        );
        let id = request.id.clone();
        
        transport.send_request(request).unwrap();
        
        let response = JsonRpcResponse::success(
            id,
            Value::String("ok".to_string()),
        );
        
        let result = transport.receive_response(response).unwrap();
        assert_eq!(result, Value::String("ok".to_string()));
    }
}
