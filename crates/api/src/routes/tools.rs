//! Tool access read endpoints.
//!
//! The tool-access persistence/service layer has not been migrated yet, but
//! Paperclip's UI expects these company-scoped read contracts to exist. Return
//! the same empty, typed envelopes until tool connections, profiles and
//! policies are backed by their repositories.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use sqlx::Row;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use uuid::Uuid;

use crate::app_state::AppState;

fn hash_gateway_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn gateway_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-paperclip-tool-gateway-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bearer_or_gateway_token(headers: &HeaderMap) -> Option<String> {
    gateway_token(headers).or_else(|| {
        headers.get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

async fn mcp_http_request(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let response = reqwest::Client::new()
        .post(url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4(),
            "method": method,
            "params": params,
        }))
        .send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() { return Err(format!("MCP server returned HTTP {}", status)); }
    if let Some(error) = body.get("error") { return Err(error.to_string()); }
    Ok(body.get("result").cloned().unwrap_or(body))
}

async fn mcp_stdio_request(command: &str, args: &[String], method: &str, params: Value) -> Result<Value, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().map_err(|error| error.to_string())?;
    let request = serde_json::json!({"jsonrpc":"2.0","id":Uuid::new_v4(),"method":method,"params":params}).to_string() + "\n";
    if let Some(mut stdin) = child.stdin.take() { stdin.write_all(request.as_bytes()).await.map_err(|error| error.to_string())?; }
    let mut stdout = child.stdout.take().ok_or("MCP stdio stdout unavailable")?;
    let mut bytes = Vec::new();
    stdout.read_to_end(&mut bytes).await.map_err(|error| error.to_string())?;
    let _ = child.kill().await;
    let text = String::from_utf8_lossy(&bytes).to_string();
    let line = text.lines().find(|line| line.trim_start().starts_with('{')).ok_or("MCP stdio returned no JSON")?;
    let body: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
    if let Some(error) = body.get("error") { return Err(error.to_string()); }
    Ok(body.get("result").cloned().unwrap_or(body))
}

fn connection_url(config: &Value) -> Option<String> {
    config.get("url").or_else(|| config.get("endpoint")).and_then(Value::as_str).map(ToOwned::to_owned)
}

async fn execute_mcp_connection(
    state: &AppState,
    company_id: Uuid,
    tool_name: &str,
    parameters: Value,
) -> Result<Value, String> {
    let raw = tool_name.strip_prefix("mcp.").ok_or("MCP tool name must start with mcp.")?;
    let (uid, upstream_name) = raw.split_once(':').ok_or("MCP tool name must be mcp.<connection>:<tool>")?;
    let connection = sqlx::query("SELECT transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
        .bind(company_id).bind(uid).fetch_optional(&state.pool).await.map_err(|error| error.to_string())?
        .ok_or("MCP connection not found or disabled")?;
    let transport: String = connection.get("transport");
    let config: Value = connection.get("transport_config");
    if transport == "mcp_remote" {
        match connection_url(&config) {
            Some(url) => mcp_http_request(&url, "tools/call", serde_json::json!({"name": upstream_name, "arguments": parameters})).await,
            None => Err("MCP connection has no remote URL".to_string()),
        }
    } else {
        match config.get("command").and_then(Value::as_str) {
            Some(command) => {
                let args = config.get("args").and_then(Value::as_array).map(|values| values.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect::<Vec<_>>()).unwrap_or_default();
                mcp_stdio_request(command, &args, "tools/call", serde_json::json!({"name": upstream_name, "arguments": parameters})).await
            }
            None => Err("MCP connection has no executable transport configuration".to_string()),
        }
    }
}

async fn load_gateway_session(
    state: &AppState,
    token: &str,
) -> Result<sqlx::postgres::PgRow, (StatusCode, Json<Value>)> {
    let row = sqlx::query(
        "SELECT id, company_id, agent_id, run_id, issue_id, expires_at, revoked_at
           FROM tool_gateway_sessions WHERE token_hash = $1",
    )
    .bind(hash_gateway_token(token))
    .fetch_optional(&state.pool)
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": error.to_string()}))))?;
    let Some(row) = row else {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Tool gateway session is invalid"}))));
    };
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = row.get("revoked_at");
    let expires_at: chrono::DateTime<chrono::Utc> = row.get("expires_at");
    if revoked_at.is_some() || expires_at <= chrono::Utc::now() {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Tool gateway session is expired or revoked"}))));
    }
    let _ = sqlx::query("UPDATE tool_gateway_sessions SET last_used_at = NOW(), updated_at = NOW() WHERE id = $1")
        .bind(row.get::<Uuid, _>("id"))
        .execute(&state.pool)
        .await;
    Ok(row)
}

async fn create_gateway_session(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let agent_id = body.get("agentId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let run_id = body.get("runId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let (Some(company_id), Some(agent_id), Some(run_id)) = (company_id, agent_id, run_id) else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "companyId, agentId, and runId are required"})));
    };
    let valid = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM heartbeat_runs WHERE id = $1 AND company_id = $2 AND agent_id = $3 AND status IN ('queued','running'))",
    )
    .bind(run_id).bind(company_id).bind(agent_id)
    .fetch_one(&state.pool).await.unwrap_or(false);
    if !valid {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "runId is not an active run for this agent"})));
    }
    let session_id = Uuid::new_v4();
    let token = format!("ptg_{}", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now() + chrono::Duration::milliseconds(
        body.get("ttlMs").and_then(Value::as_i64).unwrap_or(30 * 60 * 1000).clamp(60_000, 24 * 60 * 60 * 1000),
    );
    let result = sqlx::query(
        "INSERT INTO tool_gateway_sessions (id, company_id, agent_id, run_id, issue_id, token_hash, expires_at)
         SELECT $1, $2, $3, $4, NULLIF(context_snapshot->>'issueId', '')::uuid, $5, $6 FROM heartbeat_runs WHERE id = $4",
    )
    .bind(session_id).bind(company_id).bind(agent_id).bind(run_id).bind(hash_gateway_token(&token)).bind(expires_at)
    .execute(&state.pool).await;
    if let Err(error) = result {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": error.to_string()})));
    }
    (StatusCode::CREATED, Json(serde_json::json!({
        "sessionId": session_id,
        "token": token,
        "expiresAt": expires_at,
        "toolsUrl": "/api/tool-gateway/tools",
        "callUrl": "/api/tool-gateway/tools/call",
    })))
}

async fn revoke_gateway_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let updated = sqlx::query(
        "UPDATE tool_gateway_sessions SET revoked_at = NOW(), updated_at = NOW()
          WHERE id = $1 AND revoked_at IS NULL RETURNING id, revoked_at",
    )
    .bind(session_id)
    .fetch_optional(&state.pool)
    .await;
    match updated {
        Ok(Some(row)) => (StatusCode::OK, Json(serde_json::json!({
            "sessionId": row.get::<Uuid, _>("id"),
            "revokedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("revoked_at"),
        }))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Tool gateway session not found"}))),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": error.to_string()}))),
    }
}

async fn mcp_session_protocol(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "jsonrpc": "2.0", "id": body.get("id").cloned().unwrap_or(Value::Null),
            "error": {"code": -32001, "message": "Bearer token is required"}
        })));
    };
    if let Err(response) = load_gateway_session(&state, &token).await {
        let (status, Json(error)) = response;
        return (status, Json(serde_json::json!({
            "jsonrpc": "2.0", "id": body.get("id").cloned().unwrap_or(Value::Null),
            "error": {"code": -32001, "message": error.get("error").cloned().unwrap_or(Value::String("Invalid session".into()))}
        })));
    }
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    match body.get("method").and_then(Value::as_str) {
        Some("initialize") => (StatusCode::OK, Json(serde_json::json!({
            "jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "Parrot Agent MCP Gateway", "version": env!("CARGO_PKG_VERSION")}
            }
        }))),
        Some("notifications/initialized") => (StatusCode::ACCEPTED, Json(Value::Null)),
        Some("tools/list") => {
            let (status, Json(value)) = list_gateway_tools(State(state), headers).await;
            if !status.is_success() {
                return (status, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"Tool discovery failed"}})));
            }
            let tools = match value { Value::Array(tools) => tools, _ => Vec::new() };
            (StatusCode::OK, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"tools":tools}})))
        }
        Some("tools/call") => {
            let params = body.get("params").cloned().unwrap_or_else(|| serde_json::json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            if name.is_empty() {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"params.name is required"}})));
            }
            let call_body = serde_json::json!({"tool":name,"parameters":params.get("arguments").cloned().unwrap_or_else(|| serde_json::json!({}))});
            let (status, Json(value)) = call_gateway_tool(State(state), headers, Json(call_body)).await;
            if !status.is_success() {
                return (status, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":value.get("error").cloned().unwrap_or(Value::String("Tool call failed".into())),"data":value}})));
            }
            let result = value.get("result").cloned().unwrap_or(Value::Null);
            (StatusCode::OK, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":result.to_string()}],"structuredContent":result,"isError":false}})))
        }
        Some(_) | None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}))),
    }
}

async fn list_gateway_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Tool gateway session token is required"})));
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    let rows = sqlx::query("SELECT id, plugin_key, manifest FROM plugins WHERE status = 'ready'")
        .fetch_all(&state.pool).await.unwrap_or_default();
    let mut tools: Vec<Value> = rows.into_iter().flat_map(|row| {
        let plugin_id: Uuid = row.get("id");
        let plugin_key: String = row.get("plugin_key");
        let manifest: Value = row.get("manifest");
        manifest.get("tools").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().filter_map(move |tool| {
            let name = tool.get("name").and_then(Value::as_str).or_else(|| tool.as_str())?;
            Some(serde_json::json!({"name": name, "description": tool.get("description").and_then(Value::as_str).unwrap_or(""), "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}})), "pluginId": plugin_id, "pluginKey": plugin_key}))
        })
    }).collect();
    let connections = sqlx::query("SELECT id, uid, transport, transport_config FROM tool_connections WHERE company_id = $1 AND enabled = true")
        .bind(session.get::<Uuid, _>("company_id")).fetch_all(&state.pool).await.unwrap_or_default();
    for connection in connections {
        let connection_id: Uuid = connection.get("id");
        let uid: String = connection.get("uid");
        let transport: String = connection.get("transport");
        let config: Value = connection.get("transport_config");
        let result = if transport == "mcp_remote" {
            if let Some(url) = connection_url(&config) {
                mcp_http_request(&url, "tools/list", serde_json::json!({})).await
            } else {
                Err("MCP remote connection has no URL".to_string())
            }
        } else {
            if let Some(command) = config.get("command").and_then(Value::as_str) {
                let args = config.get("args").and_then(Value::as_array).map(|values| values.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect::<Vec<_>>()).unwrap_or_default();
                mcp_stdio_request(command, &args, "tools/list", serde_json::json!({})).await
            } else {
                Err("MCP stdio connection has no command".to_string())
            }
        };
        if let Ok(result) = result {
            for tool in result.get("tools").and_then(Value::as_array).cloned().unwrap_or_default() {
                if let Some(upstream_name) = tool.get("name").and_then(Value::as_str) {
                    tools.push(serde_json::json!({
                        "name": format!("mcp.{}:{}", uid, upstream_name),
                        "description": tool.get("description").and_then(Value::as_str).unwrap_or(""),
                        "inputSchema": tool.get("inputSchema").cloned().unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}})),
                        "connectionId": connection_id,
                        "upstreamToolName": upstream_name,
                    }));
                }
            }
        }
    }
    let company_id: Uuid = session.get("company_id");
    let agent_id: Uuid = session.get("agent_id");
    let mut visible = Vec::with_capacity(tools.len());
    for tool in tools.drain(..) {
        let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
        if gateway_decision(&state, company_id, agent_id, name).await != "deny" {
            visible.push(tool);
        }
    }
    (StatusCode::OK, Json(Value::Array(visible)))
}

async fn gateway_decision(
    state: &AppState,
    company_id: Uuid,
    agent_id: Uuid,
    tool_name: &str,
) -> String {
    let profile_effect: Option<String> = sqlx::query_scalar(
        "SELECT e.effect FROM tool_profile_entries e
           JOIN tool_profile_bindings b ON b.profile_id = e.profile_id
          WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2
            AND (e.tool_name = $3 OR e.tool_name = '*')
          ORDER BY CASE WHEN e.tool_name = $3 THEN 0 ELSE 1 END,
                   CASE WHEN e.effect = 'deny' THEN 0 ELSE 1 END
          LIMIT 1",
    )
    .bind(company_id).bind(agent_id).bind(tool_name)
    .fetch_optional(&state.pool).await.unwrap_or(None);
    if let Some(effect) = profile_effect {
        if effect == "deny" { return "deny".to_string(); }
        if effect == "allow" { return "allow".to_string(); }
    }
    let policy_type: Option<String> = sqlx::query_scalar(
        "SELECT policy_type FROM tool_policies
          WHERE company_id = $1 AND enabled = true
            AND (selectors->>'toolName' = $2 OR selectors->>'tool_name' = $2 OR selectors->>'tool' = $2)
          ORDER BY priority DESC LIMIT 1",
    )
    .bind(company_id).bind(tool_name)
    .fetch_optional(&state.pool).await.unwrap_or(None);
    match policy_type.as_deref() {
        Some("deny") | Some("block") => "deny".to_string(),
        Some("require_approval") | Some("approval") | Some("ask_first") => "require_approval".to_string(),
        Some("allow") => "allow".to_string(),
        _ => "deny".to_string(),
    }
}

async fn call_gateway_tool(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let Some(token) = bearer_or_gateway_token(&headers) else {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Tool gateway session token is required"})));
    };
    let session = match load_gateway_session(&state, &token).await {
        Ok(row) => row,
        Err(response) => return response,
    };
    let tool_name = body.get("tool").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty());
    let Some(tool_name) = tool_name else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "tool is required and must be a string"})));
    };
    let company_id: Uuid = session.get("company_id");
    let agent_id: Uuid = session.get("agent_id");
    let run_id: Uuid = session.get("run_id");
    let plugin = sqlx::query("SELECT id, manifest FROM plugins WHERE status = 'ready' AND EXISTS (SELECT 1 FROM jsonb_array_elements(manifest->'tools') item WHERE item->>'name' = $1)")
        .bind(tool_name).fetch_optional(&state.pool).await.unwrap_or(None);
    if plugin.is_none() && tool_name.starts_with("mcp.") {
        let raw = &tool_name[4..];
        if let Some((uid, upstream_name)) = raw.split_once(':') {
            let connection = sqlx::query("SELECT id, transport, transport_config FROM tool_connections WHERE company_id=$1 AND uid=$2 AND enabled=true")
                .bind(company_id).bind(uid).fetch_optional(&state.pool).await.unwrap_or(None);
            if let Some(connection) = connection {
                let connection_id: Uuid = connection.get("id");
                let transport: String = connection.get("transport");
                let config: Value = connection.get("transport_config");
                let parameters = body.get("parameters").cloned().unwrap_or_else(|| serde_json::json!({}));
                let decision = gateway_decision(&state, company_id, agent_id, tool_name).await;
                let invocation_id = Uuid::new_v4();
                let args_summary = serde_json::json!({"valueType":"object","keys":parameters.as_object().map(|value| value.len()).unwrap_or(0)});
                if decision == "deny" {
                    let _ = sqlx::query("INSERT INTO tool_invocations (id,company_id,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,arguments_summary,policy_decision,status,error_code,completed_at) VALUES ($1,$2,'agent',$3,$4,$5,$6,$7,$8,'deny','denied','policy_denied',NOW())")
                        .bind(invocation_id).bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&args_summary).execute(&state.pool).await;
                    return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Tool call denied by policy","reasonCode":"policy_denied","decision":"deny","invocationId":invocation_id})));
                }
                let _ = sqlx::query("INSERT INTO tool_invocations (id,company_id,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,arguments_summary,policy_decision,status,started_at) VALUES ($1,$2,'agent',$3,$4,$5,$6,$7,$8,$9,'executing',NOW())")
                    .bind(invocation_id).bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&args_summary).bind(&decision).execute(&state.pool).await;
                let result = if transport == "mcp_remote" {
                    match connection_url(&config) {
                        Some(url) => mcp_http_request(&url, "tools/call", serde_json::json!({"name": upstream_name, "arguments": parameters})).await,
                        None => Err("MCP connection has no remote URL".to_string()),
                    }
                } else {
                    match config.get("command").and_then(Value::as_str) {
                        Some(command) => {
                            let args = config.get("args").and_then(Value::as_array).map(|values| values.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect::<Vec<_>>()).unwrap_or_default();
                            mcp_stdio_request(command, &args, "tools/call", serde_json::json!({"name": upstream_name, "arguments": parameters})).await
                        }
                        None => Err("MCP connection has no executable transport configuration".to_string()),
                    }
                };
                return match result {
                    Ok(value) => {
                        let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded',result_summary=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
                        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,connection_id,tool_name,decision,outcome,invocation_id) VALUES ($1,'call_completed','agent',$2,$3,$4,$5,$6,$7,'success',$8)").bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(connection_id).bind(tool_name).bind(&decision).bind(invocation_id).execute(&state.pool).await;
                        (StatusCode::OK, Json(serde_json::json!({"decision":"allowed","invocationId":invocation_id,"result":value})))
                    }
                    Err(error) => {
                        let _ = sqlx::query("UPDATE tool_invocations SET status='failed',error_message=$2,completed_at=NOW(),updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(&error).execute(&state.pool).await;
                        (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":error,"reasonCode":"mcp_tool_execution_failed","invocationId":invocation_id})))
                    }
                };
            }
        }
    }
    let Some(plugin) = plugin else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Tool not found", "reasonCode": "tool_not_found"})));
    };
    let plugin_id: Uuid = plugin.get("id");
    let parameters = body.get("parameters").cloned().unwrap_or_else(|| serde_json::json!({}));
    let decision = gateway_decision(&state, company_id, agent_id, tool_name).await;
    let invocation_id = Uuid::new_v4();
    let args_summary = serde_json::json!({"valueType":"object","keys":parameters.as_object().map(|value| value.len()).unwrap_or(0)});
    if decision == "deny" {
        let _ = sqlx::query("INSERT INTO tool_invocations (id, company_id, actor_type, actor_id, agent_id, run_id, tool_name, arguments_summary, policy_decision, status, error_code, completed_at) VALUES ($1,$2,'agent',$3,$4,$5,$6,$7,'deny','denied','policy_denied',NOW())")
            .bind(invocation_id).bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(&args_summary).execute(&state.pool).await;
        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,reason_code) VALUES ($1,'call_denied','agent',$2,$3,$4,$5,'deny','denied',$6,'policy_denied')")
            .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).execute(&state.pool).await;
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Tool call denied by policy","reasonCode":"policy_denied","decision":"deny","invocationId":invocation_id})));
    }
    let insert = sqlx::query("INSERT INTO tool_invocations (id, company_id, actor_type, actor_id, agent_id, run_id, tool_name, arguments_summary, policy_decision, status, started_at) VALUES ($1,$2,'agent',$3,$4,$5,$6,$7,$8,$9,NOW())")
        .bind(invocation_id).bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(&args_summary)
        .bind(&decision).bind(if decision == "require_approval" { "pending" } else { "executing" })
        .execute(&state.pool).await;
    if let Err(error) = insert {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": error.to_string()})));
    }
    if decision == "require_approval" {
        let action_id = Uuid::new_v4();
        let _ = sqlx::query("INSERT INTO tool_action_requests (id,company_id,invocation_id,issue_id,status,canonical_arguments_hash,canonical_arguments_summary,signed_arguments,preview_markdown,requested_by_agent_id) VALUES ($1,$2,$3,(SELECT issue_id FROM tool_gateway_sessions WHERE id=$4),'pending',$5,$6,$7,$8,$9)")
            .bind(action_id).bind(company_id).bind(invocation_id).bind(session.get::<Uuid, _>("id"))
            .bind(hash_gateway_token(&parameters.to_string())).bind(&args_summary).bind(parameters.to_string()).bind(format!("Tool call requires approval: {tool_name}" )).bind(agent_id).execute(&state.pool).await;
        let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,action_request_id,reason_code) VALUES ($1,'approval_requested','agent',$2,$3,$4,$5,'require_approval','pending',$6,$7,'policy_requires_approval')")
            .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(action_id).execute(&state.pool).await;
        return (StatusCode::OK, Json(serde_json::json!({"decision":"require_approval","invocationId":invocation_id,"actionRequestId":action_id,"status":"pending"})));
    }
    let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,arguments_summary,invocation_id) VALUES ($1,'call_started','agent',$2,$3,$4,$5,'allow','pending',$6,$7)")
        .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(&args_summary).bind(invocation_id).execute(&state.pool).await;
    let result = state.plugin_service.dispatch_tool(plugin_id, tool_name, parameters).await;
    match result {
        Ok(value) => {
            let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded', result_summary=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,result_summary) VALUES ($1,'call_completed','agent',$2,$3,$4,$5,'allow','success',$6,$7)")
                .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            (StatusCode::OK, Json(serde_json::json!({"decision":"allowed","invocationId":invocation_id,"result":value})))
        }
        Err(error) => {
            let message = error.to_string();
            let _ = sqlx::query("UPDATE tool_invocations SET status='failed', error_message=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1")
                .bind(invocation_id).bind(&message).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,error_message) VALUES ($1,'call_failed','agent',$2,$3,$4,$5,'allow','failure',$6,$7)")
                .bind(company_id).bind(agent_id.to_string()).bind(agent_id).bind(run_id).bind(tool_name).bind(invocation_id).bind(&message).execute(&state.pool).await;
            (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":message,"reasonCode":"tool_execution_failed","invocationId":invocation_id})))
        }
    }
}

async fn approve_gateway_action(
    Path(action_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let Some(company_id) = company_id else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"companyId is required"})));
    };
    let row = sqlx::query("SELECT ar.invocation_id, ar.status, ar.signed_arguments, i.tool_name, i.agent_id, i.run_id FROM tool_action_requests ar JOIN tool_invocations i ON i.id = ar.invocation_id WHERE ar.id=$1 AND ar.company_id=$2")
        .bind(action_id).bind(company_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let Some(row) = row else { return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Action request not found"}))); };
    if row.get::<String, _>("status") != "pending" { return (StatusCode::CONFLICT, Json(serde_json::json!({"error":"Action request is not pending"}))); }
    let invocation_id: Uuid = row.get("invocation_id");
    let tool_name: String = row.get("tool_name");
    let agent_id: Uuid = row.get("agent_id");
    let run_id: Uuid = row.get("run_id");
    let parameters = row.get::<Option<String>, _>("signed_arguments").and_then(|value| serde_json::from_str::<Value>(&value).ok()).unwrap_or_else(|| serde_json::json!({}));
    let plugin = sqlx::query("SELECT id FROM plugins WHERE status='ready' AND EXISTS (SELECT 1 FROM jsonb_array_elements(manifest->'tools') item WHERE item->>'name'=$1)")
        .bind(&tool_name).fetch_optional(&state.pool).await.unwrap_or(None);
    let _ = sqlx::query("UPDATE tool_action_requests SET status='executing', decided_at=NOW(), updated_at=NOW() WHERE id=$1").bind(action_id).execute(&state.pool).await;
    let _ = sqlx::query("UPDATE tool_invocations SET status='executing', policy_decision='allow', started_at=COALESCE(started_at,NOW()), updated_at=NOW() WHERE id=$1").bind(invocation_id).execute(&state.pool).await;
    let result = if let Some(plugin) = plugin {
        let plugin_id: Uuid = plugin.get("id");
        state.plugin_service.dispatch_tool(plugin_id, &tool_name, parameters).await.map_err(|error| error.to_string())
    } else if tool_name.starts_with("mcp.") {
        execute_mcp_connection(&state, company_id, &tool_name, parameters).await
    } else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Tool not found"})));
    };
    match result {
        Ok(value) => {
            let _ = sqlx::query("UPDATE tool_action_requests SET status='executed', resolved_at=NOW(), updated_at=NOW() WHERE id=$1").bind(action_id).execute(&state.pool).await;
            let _ = sqlx::query("UPDATE tool_invocations SET status='succeeded', result_summary=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(serde_json::json!({"valueType":"json"})).execute(&state.pool).await;
            let _ = sqlx::query("INSERT INTO tool_call_events (company_id,event_type,actor_type,actor_id,agent_id,run_id,tool_name,decision,outcome,invocation_id,action_request_id,reason_code) VALUES ($1,'call_completed','board','board',$2,$3,$4,'allow','success',$5,$6,'approved_action_executed')").bind(company_id).bind(agent_id).bind(run_id).bind(&tool_name).bind(invocation_id).bind(action_id).execute(&state.pool).await;
            (StatusCode::OK, Json(serde_json::json!({"decision":"allowed","invocationId":invocation_id,"actionRequestId":action_id,"result":value})))
        }
        Err(error) => {
            let message = error.to_string();
            let _ = sqlx::query("UPDATE tool_action_requests SET status='failed', resolved_at=NOW(), updated_at=NOW() WHERE id=$1").bind(action_id).execute(&state.pool).await;
            let _ = sqlx::query("UPDATE tool_invocations SET status='failed', error_message=$2, completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).bind(&message).execute(&state.pool).await;
            (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":message,"reasonCode":"approved_tool_execution_failed","actionRequestId":action_id})))
        }
    }
}

async fn decline_gateway_action(
    Path(action_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|value| Uuid::parse_str(value).ok());
    let Some(company_id) = company_id else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"companyId is required"}))); };
    let updated = sqlx::query("UPDATE tool_action_requests SET status='declined', resolved_at=NOW(), updated_at=NOW() WHERE id=$1 AND company_id=$2 AND status='pending' RETURNING id, invocation_id")
        .bind(action_id).bind(company_id).fetch_optional(&state.pool).await.unwrap_or(None);
    let Some(row) = updated else { return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Pending action request not found"}))); };
    let invocation_id: Uuid = row.get("invocation_id");
    let _ = sqlx::query("UPDATE tool_invocations SET status='denied', error_code='approval_declined', completed_at=NOW(), updated_at=NOW() WHERE id=$1").bind(invocation_id).execute(&state.pool).await;
    (StatusCode::OK, Json(serde_json::json!({"id":action_id,"invocationId":invocation_id,"status":"declined"})))
}

async fn list_named_gateways(Path(company_id): Path<Uuid>, State(state): State<AppState>) -> impl IntoResponse {
    let gateways = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(
          'id',g.id,'companyId',g.company_id,'gatewayPublicId',g.gateway_public_id,
          'name',g.name,'slug',g.slug,'description',g.description,'status',g.status,
          'profileId',g.profile_id,'agentId',g.agent_id,'issueId',g.issue_id,
          'metadata',g.metadata,'createdAt',g.created_at,'updatedAt',g.updated_at,
          'tokens',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',t.id,'gatewayId',t.gateway_id,'name',t.name,'tokenPrefix',t.token_prefix,'allowedActions',t.allowed_actions,'expiresAt',t.expires_at,'lastUsedAt',t.last_used_at,'revokedAt',t.revoked_at,'createdAt',t.created_at,'updatedAt',t.updated_at) ORDER BY t.created_at DESC) FROM tool_mcp_gateway_tokens t WHERE t.gateway_id=g.id),'[]'::jsonb)
        ) ORDER BY g.name),'[]'::jsonb) FROM tool_mcp_gateways g WHERE g.company_id=$1 AND g.status <> 'archived'",
    ).bind(company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({"gateways": gateways})))
}

async fn create_named_gateway(Path(company_id): Path<Uuid>, State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let name = body.get("name").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
    let Some(name) = name else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"name is required"}))); };
    let slug = body.get("slug").and_then(Value::as_str).unwrap_or(name).trim().to_lowercase().replace(' ', "-");
    let row = sqlx::query("INSERT INTO tool_mcp_gateways (company_id,name,slug,description,agent_id,issue_id,metadata) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id,gateway_public_id,created_at,updated_at")
        .bind(company_id).bind(name).bind(&slug).bind(body.get("description").and_then(Value::as_str))
        .bind(body.get("agentId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok()))
        .bind(body.get("issueId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok()))
        .bind(body.get("metadata").cloned().unwrap_or_else(|| serde_json::json!({}))).fetch_one(&state.pool).await;
    match row {
        Ok(row) => (StatusCode::CREATED, Json(serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":row.get::<String,_>("gateway_public_id"),"name":name,"slug":slug,"description":body.get("description"),"status":"active","agentId":body.get("agentId"),"issueId":body.get("issueId"),"metadata":body.get("metadata").cloned().unwrap_or_else(||serde_json::json!({})),"tokens":[],"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}))),
        Err(error) => (StatusCode::CONFLICT, Json(serde_json::json!({"error":error.to_string()}))),
    }
}

async fn update_named_gateway(Path(gateway_id): Path<Uuid>, State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok());
    let Some(company_id) = company_id else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"companyId is required"}))); };
    let row = sqlx::query("UPDATE tool_mcp_gateways SET name=COALESCE($3,name), description=COALESCE($4,description), status=COALESCE($5,status), updated_at=NOW() WHERE id=$1 AND company_id=$2 RETURNING id,gateway_public_id,name,slug,description,status,agent_id,issue_id,metadata,created_at,updated_at")
        .bind(gateway_id).bind(company_id).bind(body.get("name").and_then(Value::as_str)).bind(body.get("description").and_then(Value::as_str)).bind(body.get("status").and_then(Value::as_str)).fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => (StatusCode::OK, Json(serde_json::json!({"id":row.get::<Uuid,_>("id"),"companyId":company_id,"gatewayPublicId":row.get::<String,_>("gateway_public_id"),"name":row.get::<String,_>("name"),"slug":row.get::<String,_>("slug"),"description":row.get::<Option<String>,_>("description"),"status":row.get::<String,_>("status"),"agentId":row.get::<Option<Uuid>,_>("agent_id"),"issueId":row.get::<Option<Uuid>,_>("issue_id"),"metadata":row.get::<Value,_>("metadata"),"tokens":[],"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Gateway not found"}))),
        Err(error) => (StatusCode::CONFLICT, Json(serde_json::json!({"error":error.to_string()}))),
    }
}

async fn create_named_gateway_token(Path(gateway_id): Path<Uuid>, State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok());
    let Some(company_id) = company_id else { return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"companyId is required"}))); };
    let token = format!("pcgw_{}", Uuid::new_v4().simple());
    let token_id = Uuid::new_v4();
    let name = body.get("name").and_then(Value::as_str).unwrap_or("Gateway token");
    let row = sqlx::query("INSERT INTO tool_mcp_gateway_tokens (id,company_id,gateway_id,name,token_hash,token_prefix,allowed_actions,expires_at) SELECT $1,$2,$3,$4,$5,$6,COALESCE($7,'[\"tools/list\",\"tools/call\"]'::jsonb),$8 WHERE EXISTS (SELECT 1 FROM tool_mcp_gateways WHERE id=$3 AND company_id=$2) RETURNING id,gateway_id,token_prefix,created_at,updated_at,expires_at,allowed_actions")
        .bind(token_id).bind(company_id).bind(gateway_id).bind(name).bind(hash_gateway_token(&token)).bind(&token[..12.min(token.len())]).bind(body.get("allowedActions")).bind(body.get("expiresAt").and_then(Value::as_str).and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok()).map(|v|v.with_timezone(&chrono::Utc))).fetch_optional(&state.pool).await;
    match row {
        Ok(Some(row)) => (StatusCode::CREATED, Json(serde_json::json!({"id":row.get::<Uuid,_>("id"),"gatewayId":row.get::<Uuid,_>("gateway_id"),"companyId":company_id,"name":name,"token":token,"tokenPrefix":row.get::<String,_>("token_prefix"),"allowedActions":row.get::<Value,_>("allowed_actions"),"expiresAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("expires_at"),"createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at")}))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Gateway not found"}))),
        Err(error) => (StatusCode::CONFLICT, Json(serde_json::json!({"error":error.to_string()}))),
    }
}

async fn revoke_named_gateway_token(Path(token_id): Path<Uuid>, State(state): State<AppState>, Json(body): Json<Value>) -> impl IntoResponse {
    let company_id = body.get("companyId").and_then(Value::as_str).and_then(|v| Uuid::parse_str(v).ok());
    let updated = sqlx::query("UPDATE tool_mcp_gateway_tokens SET revoked_at=NOW(),updated_at=NOW() WHERE id=$1 AND company_id=$2 AND revoked_at IS NULL RETURNING id,revoked_at").bind(token_id).bind(company_id).fetch_optional(&state.pool).await.unwrap_or(None);
    match updated { Some(row) => (StatusCode::OK, Json(serde_json::json!({"id":row.get::<Uuid,_>("id"),"revokedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("revoked_at")}))), None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Token not found"}))) }
}

async fn list_connections(
    Path(_company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'applicationId', application_id,\
            'name', name, 'uid', uid, 'connectionKind', connection_kind,\
            'ownership', ownership, 'transport', transport, 'authKind', auth_kind,\
            'status', status, 'transportConfig', transport_config,\
            'credentialSecretRefs', credential_secret_refs, 'enabled', enabled,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY name), '[]'::jsonb)\
         FROM tool_connections WHERE company_id = $1",
    ).bind(_company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({ "connections": rows })))
}

async fn list_policies(
    Path(_company_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', id, 'companyId', company_id, 'name', name, 'description', description,\
            'policyType', policy_type, 'priority', priority, 'enabled', enabled,\
            'selectors', selectors, 'conditions', conditions, 'config', config,\
            'createdByAgentId', created_by_agent_id, 'createdByUserId', created_by_user_id,\
            'createdAt', created_at, 'updatedAt', updated_at) ORDER BY priority, name), '[]'::jsonb)\
         FROM tool_policies WHERE company_id = $1",
    ).bind(_company_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({ "policies": rows })))
}

async fn effective_profiles_for_agent(
    Path((_company_id, agent_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let profiles = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(p) || jsonb_build_object('profileKey', p.profile_key) ORDER BY p.name), '[]'::jsonb)\
         FROM tool_profiles p JOIN tool_profile_bindings b ON b.profile_id = p.id\
         WHERE p.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let bindings = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(to_jsonb(b) ORDER BY b.created_at), '[]'::jsonb)\
         FROM tool_profile_bindings b WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let entries = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
            'id', e.id, 'profileId', e.profile_id, 'selectorType', e.selector_type,\
            'selectorValue', e.selector_value, 'effect', e.effect, 'connectionId', e.connection_id,\
            'toolName', e.tool_name, 'createdAt', e.created_at, 'updatedAt', e.updated_at)\
            ORDER BY e.created_at), '[]'::jsonb)\
         FROM tool_profile_entries e WHERE e.profile_id IN (\
            SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let allowed_names = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(DISTINCT e.tool_name) FILTER (WHERE e.effect = 'allow' AND e.tool_name IS NOT NULL), '[]'::jsonb)\
         FROM tool_profile_entries e WHERE e.profile_id IN (\
            SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    let installed_connections = sqlx::query_scalar::<_, Value>(
        "SELECT COALESCE(jsonb_agg(DISTINCT jsonb_build_object(\
            'id', c.id, 'companyId', c.company_id, 'applicationId', c.application_id, 'name', c.name,\
            'uid', c.uid, 'connectionKind', c.connection_kind, 'ownership', c.ownership,\
            'transport', c.transport, 'authKind', c.auth_kind, 'status', c.status,\
            'transportConfig', c.transport_config, 'credentialSecretRefs', c.credential_secret_refs,\
            'enabled', c.enabled, 'createdAt', c.created_at, 'updatedAt', c.updated_at)), '[]'::jsonb)\
         FROM tool_connections c JOIN tool_profile_entries e ON e.connection_id = c.id\
         WHERE e.profile_id IN (SELECT b.profile_id FROM tool_profile_bindings b\
            WHERE b.company_id = $1 AND b.target_type = 'agent' AND b.target_id = $2)",
    ).bind(_company_id).bind(agent_id).fetch_one(&state.pool).await.unwrap_or(Value::Array(vec![]));
    (StatusCode::OK, Json(serde_json::json!({"agentId": agent_id, "profiles": profiles, "entries": entries, "bindings": bindings, "allowedTools": [], "allowedToolNames": allowed_names, "installedConnections": installed_connections})))
}

/// Paperclip UI run-detail contract: return tool decisions associated with a
/// heartbeat run. Tool invocation persistence is not migrated yet, so an
/// existing run receives an empty decision list instead of a 404.
async fn get_run_decisions(
    Path((company_id, run_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let invocations = match sqlx::query(
        "SELECT id, idempotency_key, actor_type, actor_id, agent_id, issue_id, run_id,
                application_id, connection_id, catalog_entry_id, tool_name,
                arguments_hash, arguments_summary, policy_decision, matched_policy_ids,
                approval_state, status, upstream_request_id, result_hash, result_summary,
                result_size_bytes, result_artifact_id, error_code, error_message,
                started_at, completed_at, created_at, updated_at
           FROM tool_invocations
          WHERE company_id = $1 AND run_id = $2
          ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(run_id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            );
        }
    };

    let mut decisions = Vec::with_capacity(invocations.len());
    for invocation in invocations {
        let invocation_id: Uuid = invocation.get("id");
        let action = match sqlx::query(
            "SELECT id, issue_id, interaction_id, approval_id, status,
                    canonical_arguments_hash, canonical_arguments_summary, signed_arguments,
                    preview_markdown, requested_by_agent_id, requested_by_user_id,
                    resolved_by_agent_id, resolved_by_user_id, decided_by_agent_id,
                    decided_by_user_id, decided_at, expires_at, resolved_at, created_at, updated_at
               FROM tool_action_requests WHERE company_id = $1 AND invocation_id = $2
               ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(invocation_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(row) => row,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                );
            }
        };

        let events = match sqlx::query(
            "SELECT id, event_type, actor_type, actor_id, agent_id, run_id, issue_id,
                    application_id, connection_id, catalog_entry_id, invocation_id,
                    action_request_id, runtime_slot_id, tool_name, decision,
                    matched_policy_ids, reason_code, outcome, latency_ms, arguments_summary,
                    request_hash, request_summary, result_hash, result_summary,
                    result_size_bytes, redaction_plan, rate_limit_state, metadata,
                    error_code, error_message, created_at
               FROM tool_call_events
              WHERE company_id = $1 AND invocation_id = $2
              ORDER BY created_at DESC",
        )
        .bind(company_id)
        .bind(invocation_id)
        .fetch_all(&state.pool)
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                );
            }
        };

        let event_values: Vec<Value> = events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "id": event.get::<Uuid, _>("id"),
                    "companyId": company_id,
                    "eventType": event.get::<String, _>("event_type"),
                    "actorType": event.get::<String, _>("actor_type"),
                    "actorId": event.get::<Option<String>, _>("actor_id"),
                    "agentId": event.get::<Option<Uuid>, _>("agent_id"),
                    "runId": event.get::<Option<Uuid>, _>("run_id"),
                    "issueId": event.get::<Option<Uuid>, _>("issue_id"),
                    "invocationId": event.get::<Option<Uuid>, _>("invocation_id"),
                    "actionRequestId": event.get::<Option<Uuid>, _>("action_request_id"),
                    "toolName": event.get::<Option<String>, _>("tool_name"),
                    "decision": event.get::<Option<String>, _>("decision"),
                    "matchedPolicyIds": event.get::<Value, _>("matched_policy_ids"),
                    "reasonCode": event.get::<Option<String>, _>("reason_code"),
                    "outcome": event.get::<String, _>("outcome"),
                    "latencyMs": event.get::<Option<i32>, _>("latency_ms"),
                    "argumentsSummary": event.get::<Option<Value>, _>("arguments_summary"),
                    "requestHash": event.get::<Option<String>, _>("request_hash"),
                    "requestSummary": event.get::<Option<Value>, _>("request_summary"),
                    "resultHash": event.get::<Option<String>, _>("result_hash"),
                    "resultSummary": event.get::<Option<Value>, _>("result_summary"),
                    "resultSizeBytes": event.get::<Option<i32>, _>("result_size_bytes"),
                    "redactionPlan": event.get::<Option<Value>, _>("redaction_plan"),
                    "rateLimitState": event.get::<Option<Value>, _>("rate_limit_state"),
                    "metadata": event.get::<Option<Value>, _>("metadata"),
                    "errorCode": event.get::<Option<String>, _>("error_code"),
                    "errorMessage": event.get::<Option<String>, _>("error_message"),
                    "createdAt": event.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                })
            })
            .collect();
        let latest_event = event_values.first().cloned().unwrap_or(Value::Null);
        let policy_decision: Option<String> = invocation.get("policy_decision");
        let pending_action = action.as_ref().and_then(|row| {
            (row.get::<String, _>("status") == "pending").then(|| serde_json::json!({
                "actionRequestId": row.get::<Uuid, _>("id"),
                "issueId": row.get::<Option<Uuid>, _>("issue_id"),
                "interactionId": row.get::<Option<Uuid>, _>("interaction_id"),
                "approvalId": row.get::<Option<Uuid>, _>("approval_id"),
                "status": row.get::<String, _>("status"),
                "previewMarkdown": row.get::<Option<String>, _>("preview_markdown"),
            }))
        });
        let latest_decision = event_values
            .first()
            .and_then(|value| value.get("decision"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(policy_decision.clone());
        let latest_outcome = event_values
            .first()
            .and_then(|value| value.get("outcome"))
            .cloned()
            .unwrap_or(Value::Null);
        let latest_reason_code = event_values
            .first()
            .and_then(|value| value.get("reasonCode"))
            .cloned()
            .unwrap_or(Value::Null);

        decisions.push(serde_json::json!({
            "invocation": {
                "id": invocation_id,
                "companyId": company_id,
                "idempotencyKey": invocation.get::<Option<String>, _>("idempotency_key"),
                "actorType": invocation.get::<String, _>("actor_type"),
                "actorId": invocation.get::<Option<String>, _>("actor_id"),
                "agentId": invocation.get::<Option<Uuid>, _>("agent_id"),
                "issueId": invocation.get::<Option<Uuid>, _>("issue_id"),
                "runId": invocation.get::<Option<Uuid>, _>("run_id"),
                "toolName": invocation.get::<String, _>("tool_name"),
                "argumentsHash": invocation.get::<Option<String>, _>("arguments_hash"),
                "argumentsSummary": invocation.get::<Option<Value>, _>("arguments_summary"),
                "policyDecision": policy_decision,
                "matchedPolicyIds": invocation.get::<Value, _>("matched_policy_ids"),
                "approvalState": invocation.get::<String, _>("approval_state"),
                "status": invocation.get::<String, _>("status"),
                "upstreamRequestId": invocation.get::<Option<String>, _>("upstream_request_id"),
                "resultHash": invocation.get::<Option<String>, _>("result_hash"),
                "resultSummary": invocation.get::<Option<Value>, _>("result_summary"),
                "resultSizeBytes": invocation.get::<Option<i32>, _>("result_size_bytes"),
                "resultArtifactId": invocation.get::<Option<Uuid>, _>("result_artifact_id"),
                "errorCode": invocation.get::<Option<String>, _>("error_code"),
                "errorMessage": invocation.get::<Option<String>, _>("error_message"),
                "startedAt": invocation.get::<Option<chrono::DateTime<chrono::Utc>>, _>("started_at"),
                "completedAt": invocation.get::<Option<chrono::DateTime<chrono::Utc>>, _>("completed_at"),
                "createdAt": invocation.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": invocation.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            },
            "actionRequest": action.as_ref().map(|row| serde_json::json!({
                "id": row.get::<Uuid, _>("id"), "companyId": company_id,
                "invocationId": invocation_id, "issueId": row.get::<Option<Uuid>, _>("issue_id"),
                "interactionId": row.get::<Option<Uuid>, _>("interaction_id"),
                "approvalId": row.get::<Option<Uuid>, _>("approval_id"), "status": row.get::<String, _>("status"),
                "canonicalArgumentsHash": row.get::<String, _>("canonical_arguments_hash"),
                "canonicalArgumentsSummary": row.get::<Value, _>("canonical_arguments_summary"),
                "signedArguments": row.get::<Option<String>, _>("signed_arguments"),
                "previewMarkdown": row.get::<Option<String>, _>("preview_markdown"),
                "requestedByAgentId": row.get::<Option<Uuid>, _>("requested_by_agent_id"),
                "requestedByUserId": row.get::<Option<String>, _>("requested_by_user_id"),
                "resolvedByAgentId": row.get::<Option<Uuid>, _>("resolved_by_agent_id"),
                "resolvedByUserId": row.get::<Option<String>, _>("resolved_by_user_id"),
                "decidedByAgentId": row.get::<Option<Uuid>, _>("decided_by_agent_id"),
                "decidedByUserId": row.get::<Option<String>, _>("decided_by_user_id"),
                "decidedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("decided_at"),
                "expiresAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("expires_at"),
                "resolvedAt": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                "createdAt": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                "updatedAt": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            })),
            "auditEvents": event_values.clone(),
            "latestAuditEvent": latest_event,
            "decision": latest_decision,
            "outcome": latest_outcome,
            "reasonCode": latest_reason_code,
            "denialReason": Value::Null,
            "pendingAction": pending_action,
        }));
    }

    (StatusCode::OK, Json(serde_json::json!({"runId": run_id, "decisions": decisions})))
}

pub fn tool_routes() -> Router<AppState> {
    Router::new()
        .route("/tool-gateway/sessions", post(create_gateway_session))
        .route("/tool-gateway/sessions/:session_id/revoke", post(revoke_gateway_session))
        .route("/tool-gateway/tools", get(list_gateway_tools))
        .route("/tool-gateway/tools/call", post(call_gateway_tool))
        .route("/tool-gateway/mcp", post(mcp_session_protocol))
        .route("/mcp/gateways/:gateway_public_id", post(mcp_session_protocol).get(|| async { Json(serde_json::json!({"transport":"streamable_http","authentication":"bearer"})) }))
        .route("/tool-gateway/gateways/:gateway_id/mcp", post(mcp_session_protocol).get(|| async { Json(serde_json::json!({"transport":"streamable_http","authentication":"bearer"})) }))
        .route("/companies/:company_id/tools/gateways", get(list_named_gateways).post(create_named_gateway))
        .route("/tool-gateway/gateways/:gateway_id", axum::routing::patch(update_named_gateway))
        .route("/tool-gateway/gateways/:gateway_id/tokens", post(create_named_gateway_token))
        .route("/tool-gateway/gateway-tokens/:token_id/revoke", post(revoke_named_gateway_token))
        .route("/tool-gateway/action-requests/:action_id/approve", post(approve_gateway_action))
        .route("/tool-gateway/action-requests/:action_id/decline", post(decline_gateway_action))
        .route("/companies/:company_id/tools/connections", get(list_connections))
        .route("/companies/:company_id/tools/policies", get(list_policies))
        .route(
            "/companies/:company_id/tools/runs/:run_id/decisions",
            get(get_run_decisions),
        )
        .route(
            "/companies/:company_id/tools/profiles/effective/agents/:agent_id",
            get(effective_profiles_for_agent),
        )
}
