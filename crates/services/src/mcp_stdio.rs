//! MCP stdio server transport.
//!
//! Thin wrapper around the transport-agnostic [`crate::mcp_jsonrpc`] dispatch
//! core. Reads newline-delimited JSON-RPC 2.0 requests from an input stream,
//! dispatches each through a [`ToolHandler`], and writes the JSON-RPC response
//! (or nothing, for notifications) to an output stream. This is the stdio half
//! of plan §6B.3 "独立 MCP Server 包和 HTTP/stdio 传输" — the same framed
//! protocol the HTTP transport speaks, so discovery and execution stay in
//! lockstep across both transports.
//!
//! The loop is generic over the reader/writer so it is unit-testable without a
//! real TTY; the production entry point wires it to `std::io::Stdin` /
//! `std::io::Stdout`.

use std::io::{BufRead, Write};

use crate::mcp_jsonrpc::{handle_body, JsonRpcError, ToolHandler};
use serde_json::Value;

/// Run the stdio MCP server loop.
///
/// Returns when the input stream is exhausted (EOF) or a fatal framing error
/// occurs. Each input line is parsed as one JSON-RPC request; parse errors
/// produce a JSON-RPC error response written to the output stream so the
/// client gets a diagnosable result instead of a silent hang.
pub fn serve_stdio<R, W>(
    reader: R,
    mut writer: W,
    handler: &dyn ToolHandler,
) -> Result<(), std::io::Error>
where
    R: BufRead,
    W: Write,
{
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match handle_body(trimmed, handler) {
            Ok(Some(response)) => {
                if writeln!(writer, "{response}").is_err() {
                    break;
                }
            }
            Ok(None) => {
                // Notification: no response required.
            }
            Err(parse_err) => {
                // Parse error — emit a JSON-RPC error response so the client
                // is not left waiting.
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": parse_err.code,
                        "message": parse_err.message,
                    }
                });
                if writeln!(writer, "{}", serde_json::to_string(&error_response).unwrap_or_default())
                    .is_err()
                {
                    break;
                }
            }
        }
        let _ = writer.flush();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_jsonrpc::{ToolDefinition, JsonRpcErrorCode};
    use serde_json::json;
    use std::io::Cursor;

    struct EchoHandler;

    impl ToolHandler for EchoHandler {
        fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition {
                name: "echo".to_string(),
                description: Some("echo arguments".to_string()),
                input_schema: json!({"type": "object", "properties": {"msg": {"type": "string"}}}),
            }]
        }
        fn call_tool(&self, name: &str, params: &Value) -> Result<Value, String> {
            if name == "echo" {
                Ok(json!([{"type": "text", "text": params.to_string()}]))
            } else {
                Err(format!("unknown tool: {name}"))
            }
        }
    }

    fn run_script(script: &str) -> String {
        let reader = Cursor::new(script.as_bytes().to_vec());
        let mut out: Vec<u8> = Vec::new();
        serve_stdio(reader, &mut out, &EchoHandler).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn initialize_returns_protocol_version() {
        let out = run_script("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n");
        assert!(out.contains("\"protocolVersion\":\"2025-06-18\""));
        assert!(out.contains("\"serverInfo\""));
    }

    #[test]
    fn tools_list_returns_declared_tool() {
        let out = run_script("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n");
        assert!(out.contains("\"name\":\"echo\""));
    }

    #[test]
    fn tools_call_dispatches_to_handler() {
        let out = run_script(
            "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"echo\",\"arguments\":{\"msg\":\"hi\"}}}\n",
        );
        assert!(out.contains("\"content\""));
        // params echoed as text: {"msg":"hi"} -> JSON-escaped in the response
        assert!(out.contains("msg"));
        assert!(out.contains("hi"));
    }

    #[test]
    fn notification_is_not_answered() {
        // notifications/initialized has no id -> no response line.
        let out = run_script("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n");
        assert!(out.trim().is_empty(), "got: {out:?}");
    }

    #[test]
    fn parse_error_emits_jsonrpc_error() {
        let out = run_script("not-json\n");
        assert!(out.contains("\"error\""));
        assert!(out.contains(&JsonRpcErrorCode::ParseError.code().to_string()));
    }

    #[test]
    fn multiple_requests_each_answered() {
        let script = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        );
        let out = run_script(script);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "expected two response lines, got {lines:?}");
        assert!(lines[0].contains("\"id\":1"));
        assert!(lines[1].contains("\"id\":2"));
    }
}
