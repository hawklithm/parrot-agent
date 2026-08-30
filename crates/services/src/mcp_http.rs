//! MCP Streamable HTTP transport client contract.
//!
//! Faithful Rust port of Paperclip's `server/src/services/mcp-http.ts`
//! (`MCP_HTTP_ACCEPT`, `mcpHttpRequestHeaders`, `parseMcpHttpResponseBody`) and
//! the behaviors pinned by `mcp-http.test.ts`.
//!
//! The MCP Streamable HTTP spec requires the client to advertise that it
//! accepts BOTH a single JSON response and an SSE stream on every POST
//! (`Accept: application/json, text/event-stream`). Spec-compliant servers
//! reject requests missing this header with 406, and may answer with an
//! SSE-framed body instead of bare JSON. This module therefore (a) builds the
//! required headers and (b) parses either response shape.

use std::collections::HashMap;

/// The Accept header value required by the MCP Streamable HTTP transport.
pub const MCP_HTTP_ACCEPT: &str = "application/json, text/event-stream";

/// Default headers for an MCP Streamable HTTP JSON-RPC POST. Caller-supplied
/// headers (e.g. resolved credentials) are preserved, while the required
/// Streamable HTTP Accept value is kept authoritative.
pub fn mcp_http_request_headers(extra: Option<&HashMap<String, String>>) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    if let Some(extra) = extra {
        for (k, v) in extra {
            headers.insert(k.clone(), v.clone());
        }
    }
    headers.insert("accept".to_string(), MCP_HTTP_ACCEPT.to_string());
    headers
}

/// Whether a parsed JSON value looks like a JSON-RPC message.
fn looks_like_json_rpc_message(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("result")
                || map.contains_key("error")
                || map.contains_key("method")
                || map.contains_key("id")
        }
        _ => false,
    }
}

/// Parse the body of an MCP Streamable HTTP response into its JSON-RPC payload.
///
/// Handles both response shapes the transport allows:
/// - `application/json`: the body is the JSON-RPC message directly.
/// - `text/event-stream`: one or more SSE events; we return the JSON payload of
///   the first `data:` event that parses as a JSON-RPC message.
///
/// Falls back to a plain JSON parse when the content type is unknown so we stay
/// compatible with non-compliant servers that ignore the Accept header.
pub fn parse_mcp_http_response_body(
    body_text: &str,
    content_type: Option<&str>,
) -> Result<serde_json::Value, McpHttpParseError> {
    let ct = content_type.unwrap_or("");
    let is_event_stream = ct.to_lowercase().contains("text/event-stream");
    if !is_event_stream {
        return serde_json::from_str(body_text).map_err(McpHttpParseError::Json);
    }

    // Split the SSE stream into events on blank lines, then collect each event's
    // `data:` lines (which may span multiple lines per the SSE spec).
    let normalized = body_text.replace("\r\n", "\n");
    let events: Vec<&str> = normalized.split("\n\n").collect();
    let mut last_error: Option<serde_json::Error> = None;
    let mut first_parsed: Option<serde_json::Value> = None;
    let mut saw_data = false;

    for event in events {
        let data_lines: Vec<&str> = event
            .split('\n')
            .filter(|line| line.starts_with("data:"))
            .map(|line| {
                let rest = &line["data:".len()..];
                rest.strip_prefix(' ').unwrap_or(rest)
            })
            .collect();
        if data_lines.is_empty() {
            continue;
        }
        let data = data_lines.join("\n");
        match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(parsed) => {
                if !saw_data {
                    first_parsed = Some(parsed.clone());
                    saw_data = true;
                }
                if looks_like_json_rpc_message(&parsed) {
                    return Ok(parsed);
                }
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    if saw_data {
        return first_parsed.ok_or(McpHttpParseError::NoData);
    }
    if let Some(err) = last_error {
        return Err(McpHttpParseError::Json(err));
    }
    Err(McpHttpParseError::NoData)
}

/// Error parsing an MCP Streamable HTTP response body.
#[derive(Debug, thiserror::Error)]
pub enum McpHttpParseError {
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("MCP SSE response contained no data events")]
    NoData,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_headers_advertise_json_and_sse() {
        let headers = mcp_http_request_headers(None);
        assert_eq!(
            headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(
            headers.get("accept").map(String::as_str),
            Some(MCP_HTTP_ACCEPT)
        );
        assert_eq!(MCP_HTTP_ACCEPT, "application/json, text/event-stream");
    }

    #[test]
    fn request_headers_preserve_caller_while_keeping_accept() {
        let mut extra = HashMap::new();
        extra.insert("Authorization".to_string(), "Bearer x".to_string());
        extra.insert("accept".to_string(), "application/json".to_string());
        let headers = mcp_http_request_headers(Some(&extra));
        assert_eq!(
            headers.get("accept").map(String::as_str),
            Some("application/json, text/event-stream")
        );
        assert_eq!(headers.get("Authorization").map(String::as_str), Some("Bearer x"));
    }

    #[test]
    fn parses_plain_application_json_body() {
        let payload = serde_json::json!({"jsonrpc":"2.0","id":"1","result":{"tools":[]}});
        let parsed =
            parse_mcp_http_response_body(&payload.to_string(), Some("application/json")).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn parses_sse_framed_body() {
        let payload =
            serde_json::json!({"jsonrpc":"2.0","id":"1","result":{"tools":[{"name":"kv_get"}]}});
        let body = format!("event: message\ndata: {}\n\n", payload);
        let parsed =
            parse_mcp_http_response_body(&body, Some("text/event-stream; charset=utf-8")).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn skips_non_jsonrpc_sse_events() {
        let message = serde_json::json!({"jsonrpc":"2.0","id":"1","result":{"ok":true}});
        let ping = "event: ping\ndata: {\"type\":\"ping\"}";
        let body = format!("{}\n\nevent: message\ndata: {}\n\n", ping, message);
        let parsed = parse_mcp_http_response_body(&body, Some("text/event-stream")).unwrap();
        assert_eq!(parsed, message);
    }

    #[test]
    fn handles_multi_line_sse_data_fields() {
        let payload = serde_json::json!({"jsonrpc":"2.0","id":"1","result":{"note":"line"}});
        let json = serde_json::to_string_pretty(&payload).unwrap();
        let body = format!("data: {}\n\n", json.replace('\n', "\ndata: "));
        let parsed = parse_mcp_http_response_body(&body, Some("text/event-stream")).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn throws_when_sse_carries_no_data_events() {
        let result = parse_mcp_http_response_body("event: ping\n\n", Some("text/event-stream"));
        assert!(result.is_err());
    }
}
