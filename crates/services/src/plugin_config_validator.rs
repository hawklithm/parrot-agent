use serde_json::Value;

pub fn validate_config(manifest: &Value, config: &Value) -> Result<(), String> {
    if !config.is_object() { return Err("plugin config must be a JSON object".into()); }
    if let Some(required) = manifest.get("configSchema").and_then(|s|s.get("required")).and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) { if config.get(key).is_none() { return Err(format!("missing required plugin config: {key}")); } }
    }
    Ok(())
}
