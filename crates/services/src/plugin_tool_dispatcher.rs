use serde_json::Value;

pub fn declared_tool(manifest: &Value, tool: &str) -> bool { manifest.get("tools").and_then(Value::as_array).is_some_and(|items| items.iter().any(|item| item.as_str() == Some(tool) || item.get("name").and_then(Value::as_str) == Some(tool))) }
pub fn declared_action(manifest: &Value, action: &str) -> bool { manifest.get("actions").and_then(Value::as_array).is_some_and(|items| items.iter().any(|item| item.as_str() == Some(action) || item.get("name").and_then(Value::as_str) == Some(action))) }
