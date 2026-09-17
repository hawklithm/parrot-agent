//! Client-specific MCP configuration builders.
//!
//! The hosted gateway exposes one Streamable HTTP endpoint, but MCP clients do
//! not share one configuration schema. Keep the wire endpoint and the client
//! presentation separate so heartbeat execution and external client snippets
//! cannot drift apart.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tempfile::TempDir;
use tokio::sync::{Mutex, OwnedMutexGuard};

/// Build the common HTTP MCP server entry used by clients whose schema follows
/// the Claude/Cursor shape.
pub fn claude_mcp_server(url: &str, bearer_token: &str) -> Value {
    json!({
        "type": "http",
        "url": url,
        "headers": {
            "Authorization": format!("Bearer {bearer_token}"),
        },
    })
}

/// Build a Cursor `mcp.json` server entry.
pub fn cursor_mcp_server(url: &str, bearer_token: &str) -> Value {
    json!({
        "url": url,
        "headers": {
            "Authorization": format!("Bearer {bearer_token}"),
        },
    })
}

/// Build a Gemini CLI `settings.json` server entry for Streamable HTTP.
pub fn gemini_mcp_server(url: &str, bearer_token: &str) -> Value {
    json!({
        "httpUrl": url,
        "headers": {
            "Authorization": format!("Bearer {bearer_token}"),
        },
    })
}

/// Build an OpenCode JSON config fragment.
pub fn opencode_mcp_config(name: &str, url: &str, bearer_token: &str) -> Value {
    json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": {
            "servers": named_server_map(name, opencode_mcp_server(url, bearer_token)),
        },
    })
}

fn opencode_mcp_server(url: &str, bearer_token: &str) -> Value {
    json!({
        "type": "remote",
        "url": url,
        "oauth": false,
        "headers": {
            "Authorization": format!("Bearer {bearer_token}"),
        },
    })
}

/// Build a Codex command-line override for a managed HTTP MCP server.
///
/// The Parrot heartbeat currently uses CLI overrides instead of mutating the
/// user's persistent `config.toml`. This is the same effective configuration
/// path as a Codex `[mcp_servers.<name>]` entry, but scoped to one process.
pub fn codex_mcp_overrides(name: &str, url: &str) -> Vec<String> {
    let server_name = toml_string(name);
    vec![
        "-c".to_string(),
        format!("mcp_servers.{server_name}.url={url:?}"),
        "-c".to_string(),
        format!(
            "mcp_servers.{server_name}.env_http_headers.Authorization=\"PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION\""
        ),
    ]
}

/// Return a client-specific set of configuration snippets for a named
/// Gateway. The bearer value is deliberately a placeholder because the API
/// only returns the full token at creation time.
pub fn named_gateway_endpoint_path(public_id: &str) -> String {
    format!("/api/mcp/gateways/{public_id}")
}

pub fn named_gateway_client_snippets(name: &str, public_id: &str) -> Value {
    let endpoint = named_gateway_endpoint_path(public_id);
    let bearer = "pcgw_...";
    Value::Array(vec![
        json!({
            "client": "claude_desktop",
            "label": "Claude Desktop",
            "config": {
                "mcpServers": named_server_map(name, json!({
                    "url": endpoint,
                    "headers": { "Authorization": format!("Bearer {bearer}") },
                })),
            },
            "notes": ["Use the full Parrot origin before the endpoint path."],
        }),
        json!({
            "client": "vscode",
            "label": "VS Code",
            "config": {
                "servers": named_server_map(name, json!({
                    "type": "http",
                    "url": endpoint,
                    "headers": { "Authorization": format!("Bearer {bearer}") },
                })),
            },
            "notes": ["Place this under your MCP extension or editor MCP settings."],
        }),
        json!({
            "client": "claude_code",
            "label": "Claude Code",
            "config": {
                "command": "claude",
                "args": ["mcp", "add", name, endpoint, "--header", format!("Authorization: Bearer {bearer}")],
            },
            "notes": ["Use the full Parrot origin before the endpoint path."],
        }),
        json!({
            "client": "codex",
            "label": "Codex",
            "configToml": format!(
                "[mcp_servers.{}]\nurl = \"{}\"\nheaders = {{ Authorization = \"Bearer {}\" }}\n",
                toml_string(name), endpoint, bearer
            ),
            "notes": ["Paste this block into the effective Codex config.toml."],
        }),
        json!({
            "client": "cursor",
            "label": "Cursor",
            "config": {
                "mcpServers": named_server_map(name, json!({
                    "url": endpoint,
                    "headers": { "Authorization": format!("Bearer {bearer}") },
                })),
            },
            "notes": ["Place this under .cursor/mcp.json or ~/.cursor/mcp.json."],
        }),
        json!({
            "client": "gemini_cli",
            "label": "Gemini CLI",
            "config": {
                "mcpServers": named_server_map(name, json!({
                    "httpUrl": endpoint,
                    "headers": { "Authorization": format!("Bearer {bearer}") },
                })),
            },
            "notes": ["Place this under mcpServers in .gemini/settings.json."],
        }),
        json!({
            "client": "opencode",
            "label": "OpenCode",
            "config": opencode_mcp_config(name, &endpoint, bearer),
            "notes": ["Place this under mcp.servers in opencode.json or opencode.jsonc."],
        }),
    ])
}

fn named_server_map(name: &str, server: Value) -> Value {
    let mut servers = Map::new();
    servers.insert(name.to_string(), server);
    Value::Object(servers)
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Merge one runtime MCP server into an existing JSON configuration without
/// discarding unrelated client settings.
pub fn merge_mcp_server(
    existing: Option<&[u8]>,
    root_key: &str,
    name: &str,
    server: Value,
) -> Result<Value, String> {
    let mut root = match existing {
        Some(bytes) if !bytes.is_empty() => serde_json::from_slice::<Value>(bytes)
            .map_err(|error| format!("MCP client configuration is not valid JSON: {error}"))?,
        _ => json!({}),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| "MCP client configuration root must be a JSON object".to_string())?;
    let servers = object
        .entry(root_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| format!("MCP client configuration field '{root_key}' must be an object"))?;
    servers.insert(name.to_string(), server);
    Ok(root)
}

/// Merge one runtime MCP server into an existing OpenCode configuration.
/// OpenCode nests MCP servers under `mcp.servers`, unlike the top-level
/// `mcpServers` shape used by Claude, Cursor and Gemini.
pub fn merge_opencode_mcp_server(
    existing: Option<&[u8]>,
    name: &str,
    url: &str,
    bearer_token: &str,
) -> Result<Value, String> {
    let mut root = match existing {
        Some(bytes) if !bytes.is_empty() => serde_json::from_slice::<Value>(bytes)
            .map_err(|error| format!("OpenCode configuration is not valid JSON: {error}"))?,
        _ => json!({}),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| "OpenCode configuration root must be a JSON object".to_string())?;
    let mcp = object
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "OpenCode configuration field 'mcp' must be an object".to_string())?;
    let servers = mcp
        .entry("servers".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| {
            "OpenCode configuration field 'mcp.servers' must be an object".to_string()
        })?;
    servers.insert(name.to_string(), opencode_mcp_server(url, bearer_token));
    Ok(root)
}

/// A run-scoped JSON file written into a client workspace and restored when the
/// heartbeat command exits. Cursor and Gemini do not currently accept an MCP
/// config path on their CLI, so their native project configuration locations
/// are the only portable way to expose a per-run HTTP server.
pub struct RestorableJsonConfig {
    path: PathBuf,
    original: Option<Vec<u8>>,
    original_mode: Option<u32>,
    _lock: OwnedMutexGuard<()>,
}

impl RestorableJsonConfig {
    pub async fn install(path: impl Into<PathBuf>, config: &Value) -> Result<Self, String> {
        let path = path.into();
        let lock = workspace_config_lock().lock_owned().await;
        let original = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "failed to read MCP client configuration {}: {error}",
                    path.display()
                ));
            }
        };
        let original_mode = original.as_ref().and_then(|_| file_mode(&path));
        let parent = path
            .parent()
            .ok_or_else(|| format!("MCP client configuration has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create MCP config directory {}: {error}",
                parent.display()
            )
        })?;
        let bytes = serde_json::to_vec_pretty(config)
            .map_err(|error| format!("failed to serialize MCP client configuration: {error}"))?;
        let install_result = fs::write(&path, bytes).and_then(|_| set_private_mode(&path));
        if let Err(error) = install_result {
            if let Err(rollback_error) = restore_json_file(&path, &original, original_mode) {
                tracing::warn!(
                    path = %path.display(),
                    %rollback_error,
                    "failed to roll back MCP client configuration after install failure"
                );
            }
            return Err(format!(
                "failed to install MCP client configuration {}: {error}",
                path.display()
            ));
        }
        Ok(Self {
            path,
            original,
            original_mode,
            _lock: lock,
        })
    }
}

fn restore_json_file(
    path: &Path,
    original: &Option<Vec<u8>>,
    original_mode: Option<u32>,
) -> std::io::Result<()> {
    match original {
        Some(bytes) => {
            fs::write(path, bytes)?;
            if let Some(mode) = original_mode {
                set_file_mode(path, mode)?;
            }
            Ok(())
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
    }
}

impl Drop for RestorableJsonConfig {
    fn drop(&mut self) {
        let restore_result = restore_json_file(&self.path, &self.original, self.original_mode);
        if let Err(error) = restore_result {
            tracing::warn!(path = %self.path.display(), %error, "failed to restore run-scoped MCP client configuration");
        }
    }
}

/// Create a private temporary JSON file. The returned `TempDir` must be kept
/// alive until the child process exits.
pub fn write_private_temp_json(prefix: &str, config: &Value) -> Result<(TempDir, PathBuf), String> {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .map_err(|error| format!("failed to create run-scoped MCP config directory: {error}"))?;
    let path = dir.path().join("mcp-config.json");
    let bytes = serde_json::to_vec(config)
        .map_err(|error| format!("failed to serialize run-scoped MCP config: {error}"))?;
    fs::write(&path, bytes).map_err(|error| {
        format!(
            "failed to write run-scoped MCP config {}: {error}",
            path.display()
        )
    })?;
    set_private_mode(&path).map_err(|error| {
        format!(
            "failed to protect run-scoped MCP config {}: {error}",
            path.display()
        )
    })?;
    Ok((dir, path))
}

fn workspace_config_lock() -> Arc<Mutex<()>> {
    static LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    Arc::clone(LOCK.get_or_init(|| Arc::new(Mutex::new(()))))
}

#[cfg(unix)]
fn file_mode(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Option<u32> {
    None
}

fn set_private_mode(path: &Path) -> std::io::Result<()> {
    set_file_mode(path, 0o600)
}

#[cfg(unix)]
fn set_file_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_file_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn client_transports_use_their_native_shapes() {
        let claude = claude_mcp_server("http://localhost/mcp", "token");
        assert_eq!(claude["type"], "http");
        assert_eq!(claude["headers"]["Authorization"], "Bearer token");

        let cursor = cursor_mcp_server("http://localhost/mcp", "token");
        assert!(cursor.get("type").is_none());
        assert_eq!(cursor["url"], "http://localhost/mcp");

        let gemini = gemini_mcp_server("http://localhost/mcp", "token");
        assert_eq!(gemini["httpUrl"], "http://localhost/mcp");

        let opencode = opencode_mcp_config("paperclip", "http://localhost/mcp", "token");
        assert_eq!(opencode["mcp"]["servers"]["paperclip"]["type"], "remote");
        assert_eq!(opencode["mcp"]["servers"]["paperclip"]["oauth"], false);
    }

    #[test]
    fn codex_override_uses_env_backed_authorization() {
        let args = codex_mcp_overrides("paperclip", "http://localhost/mcp");
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("mcp_servers.\"paperclip\".url="));
        assert!(args[3].contains("env_http_headers.Authorization"));
        assert!(args[3].contains("PAPERCLIP_TOOL_GATEWAY_AUTHORIZATION"));
    }

    #[test]
    fn snippets_cover_supported_external_clients() {
        let snippets = named_gateway_client_snippets("parrot", "public-id");
        assert_eq!(
            named_gateway_endpoint_path("public-id"),
            "/api/mcp/gateways/public-id"
        );
        let clients: Vec<&str> = snippets
            .as_array()
            .expect("snippets array")
            .iter()
            .filter_map(|entry| entry.get("client").and_then(Value::as_str))
            .collect();
        assert_eq!(
            clients,
            vec![
                "claude_desktop",
                "vscode",
                "claude_code",
                "codex",
                "cursor",
                "gemini_cli",
                "opencode",
            ]
        );
        assert!(snippets[3]["configToml"]
            .as_str()
            .unwrap()
            .contains("mcp_servers"));
    }

    #[test]
    fn merge_mcp_server_preserves_unrelated_settings() {
        let merged = merge_mcp_server(
            Some(br#"{"model":"demo","mcpServers":{"old":{"url":"http://old"}}}"#),
            "mcpServers",
            "paperclip",
            cursor_mcp_server("http://new/mcp", "token"),
        )
        .expect("valid config");
        assert_eq!(merged["model"], "demo");
        assert_eq!(merged["mcpServers"]["old"]["url"], "http://old");
        assert_eq!(merged["mcpServers"]["paperclip"]["url"], "http://new/mcp");
    }

    #[test]
    fn merge_opencode_mcp_server_preserves_unrelated_settings() {
        let merged = merge_opencode_mcp_server(
            Some(br#"{"model":"demo","mcp":{"servers":{"old":{"type":"local"}}}}"#),
            "paperclip",
            "http://new/mcp",
            "token",
        )
        .expect("valid OpenCode config");
        assert_eq!(merged["model"], "demo");
        assert_eq!(merged["mcp"]["servers"]["old"]["type"], "local");
        assert_eq!(merged["mcp"]["servers"]["paperclip"]["type"], "remote");
    }

    #[tokio::test]
    async fn restorable_json_config_restores_original_file() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(".cursor").join("mcp.json");
        fs::create_dir_all(path.parent().expect("config parent")).expect("config directory");
        fs::write(&path, br#"{"mcpServers":{"local":{"url":"http://local"}}}"#)
            .expect("original config");
        let original = fs::read(&path).expect("read original config");

        {
            let _guard = RestorableJsonConfig::install(
                &path,
                &json!({"mcpServers":{"paperclip":{"url":"http://gateway"}}}),
            )
            .await
            .expect("install run-scoped config");
            assert_ne!(fs::read(&path).expect("read temporary config"), original);
        }

        assert_eq!(fs::read(&path).expect("read restored config"), original);
    }
}
