//! MCP JSON-RPC 2.0 framing and dispatch core.
//!
//! Shared by the MCP client transport (`mcp_http`) and any server/stdio
//! transport. Implements the JSON-RPC 2.0 message model the MCP protocol
//! requires: `initialize`, `tools/list`, `tools/call`, plus notifications
//! (`notifications/initialized`). This is the canonical contract behind
//! plan §6B.3 "独立 MCP Server 包和 HTTP/stdio 传输" — both transports speak
//! the same framed protocol, so discovery and execution stay in lockstep.
//!
//! The dispatch core is transport-agnostic: callers feed it a parsed
//! `JsonRpcRequest` and a [`ToolHandler`]; it returns a [`JsonRpcResponse`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// JSON-RPC 2.0 protocol version string.
pub const JSONRPC_VERSION: &str = "2.0";

/// Standard JSON-RPC error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonRpcErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

impl JsonRpcErrorCode {
    pub fn code(self) -> i64 {
        self as i64
    }
}

/// A JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// A JSON-RPC 2.0 response (result or error).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub result: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

/// A tool descriptor returned from `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub input_schema: Value,
}

/// The handler a server dispatches tool calls to.
pub trait ToolHandler: Send + Sync {
    /// List registered tools.
    fn list_tools(&self) -> Vec<ToolDefinition>;
    /// Call a tool by name with JSON params; returns the tool result content.
    fn call_tool(&self, name: &str, params: &Value) -> Result<Value, String>;
}

/// Parse a raw JSON-RPC request body.
pub fn parse_request(body: &str) -> Result<JsonRpcRequest, JsonRpcError> {
    match serde_json::from_str::<JsonRpcRequest>(body) {
        Ok(req) if req.jsonrpc == JSONRPC_VERSION => Ok(req),
        Ok(_) => Err(JsonRpcError {
            code: JsonRpcErrorCode::InvalidRequest.code(),
            message: "invalid jsonrpc version".to_string(),
            data: Value::Null,
        }),
        Err(_) => Err(JsonRpcError {
            code: JsonRpcErrorCode::ParseError.code(),
            message: "parse error".to_string(),
            data: Value::Null,
        }),
    }
}

fn ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result,
        error: None,
    }
}

fn err(id: Value, error: JsonRpcError) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id,
        result: Value::Null,
        error: Some(error),
    }
}

/// Dispatch a parsed JSON-RPC request against a tool handler.
///
/// Returns `None` for notifications (methods without a non-null id), matching
/// the JSON-RPC rule that notifications are not answered.
pub fn dispatch(request: &JsonRpcRequest, handler: &dyn ToolHandler) -> Option<JsonRpcResponse> {
    // Notification: no response required.
    if request.id.is_null() {
        return None;
    }

    match request.method.as_str() {
        "initialize" => Some(ok(
            request.id.clone(),
            json!({"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"parrot-mcp","version":"1.0.0"}}),
        )),
        "tools/list" => {
            let tools: Vec<Value> = handler
                .list_tools()
                .into_iter()
                .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
                .collect();
            Some(ok(request.id.clone(), json!({ "tools": tools })))
        }
        "tools/call" => {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
            let params = request.params.get("arguments").cloned().unwrap_or(Value::Null);
            match name {
                Some(name) => match handler.call_tool(&name, &params) {
                    Ok(content) => Some(ok(request.id.clone(), json!({ "content": content }))),
                    Err(e) => Some(err(
                        request.id.clone(),
                        JsonRpcError {
                            code: JsonRpcErrorCode::InternalError.code(),
                            message: e,
                            data: Value::Null,
                        },
                    )),
                },
                None => Some(err(
                    request.id.clone(),
                    JsonRpcError {
                        code: JsonRpcErrorCode::InvalidParams.code(),
                        message: "missing tool name".to_string(),
                        data: Value::Null,
                    },
                )),
            }
        }
        _ => Some(err(
            request.id.clone(),
            JsonRpcError {
                code: JsonRpcErrorCode::MethodNotFound.code(),
                message: format!("method not found: {}", request.method),
                data: Value::Null,
            },
        )),
    }
}

/// Parse + dispatch a raw request body (convenience for transport layers).
pub fn handle_body(body: &str, handler: &dyn ToolHandler) -> Result<Option<String>, JsonRpcError> {
    let request = parse_request(body)?;
    let response = dispatch(&request, handler);
    Ok(response.map(|r| serde_json::to_string(&r).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubHandler;

    impl ToolHandler for StubHandler {
        fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "kv_get".to_string(),
                description: Some("get a value".to_string()),
                input_schema: json!({"type": "object", "properties": {"key": {"type": "string"}}}),
            }]
        }
        fn call_tool(&self, name: &str, params: &Value) -> Result<Value, String> {
            match name {
                "kv_get" => Ok(json!([{"type": "text", "text": format!("value={}", params.get("key").and_then(Value::as_str).unwrap_or(""))}])),
                _ => Err(format!("unknown tool: {name}")),
            }
        }
    }

    #[test]
    fn initialize_returns_protocol_and_capabilities() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from(1),
            method: "initialize".to_string(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        assert_eq!(resp.error, None);
        assert!(resp.result.get("capabilities").is_some());
        assert!(resp.result.get("serverInfo").is_some());
    }

    #[test]
    fn tools_list_returns_registered_tools() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from("2"),
            method: "tools/list".to_string(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        let tools = resp.result.get("tools").unwrap().as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].get("name").unwrap(), "kv_get");
    }

    #[test]
    fn tools_call_dispatches_to_handler() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from(3),
            method: "tools/call".to_string(),
            params: json!({"name": "kv_get", "arguments": {"key": "demo"}}),
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        let content = resp.result.get("content").unwrap().as_array().unwrap();
        assert_eq!(content[0].get("text").unwrap(), "value=demo");
    }

    #[test]
    fn tools_call_unknown_tool_is_internal_error() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from(4),
            method: "tools/call".to_string(),
            params: json!({"name": "nope", "arguments": {}}),
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        assert_eq!(resp.error.as_ref().unwrap().code, JsonRpcErrorCode::InternalError.code());
    }

    #[test]
    fn tools_call_missing_name_is_invalid_params() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from(5),
            method: "tools/call".to_string(),
            params: json!({}),
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        assert_eq!(resp.error.as_ref().unwrap().code, JsonRpcErrorCode::InvalidParams.code());
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::from(6),
            method: "bogus".to_string(),
            params: Value::Null,
        };
        let resp = dispatch(&req, &StubHandler).unwrap();
        assert_eq!(resp.error.as_ref().unwrap().code, JsonRpcErrorCode::MethodNotFound.code());
    }

    #[test]
    fn notification_is_not_answered() {
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: Value::Null,
            method: "notifications/initialized".to_string(),
            params: Value::Null,
        };
        assert!(dispatch(&req, &StubHandler).is_none());
    }

    #[test]
    fn malformed_body_is_parse_error() {
        let err = parse_request("{not json").unwrap_err();
        assert_eq!(err.code, JsonRpcErrorCode::ParseError.code());
    }

    #[test]
    fn wrong_version_is_invalid_request() {
        let err = parse_request(r#"{"jsonrpc":"1.0","id":1,"method":"tools/list"}"#).unwrap_err();
        assert_eq!(err.code, JsonRpcErrorCode::InvalidRequest.code());
    }

    #[test]
    fn handle_body_round_trips_request_and_response() {
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#;
        let out = handle_body(body, &StubHandler).unwrap().unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.id, Value::from(9));
        assert!(parsed.result.get("tools").is_some());
    }
}
