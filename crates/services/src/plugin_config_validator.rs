use serde_json::Value;

pub fn validate_config(manifest: &Value, config: &Value) -> Result<(), String> {
    if !config.is_object() { return Err("plugin config must be a JSON object".into()); }
    if let Some(required) = manifest.get("configSchema").and_then(|s|s.get("required")).and_then(Value::as_array) {
        for key in required.iter().filter_map(Value::as_str) { if config.get(key).is_none() { return Err(format!("missing required plugin config: {key}")); } }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn non_object_config_is_rejected() {
        let err = validate_config(&json!({}), &json!([])).unwrap_err();
        assert!(err.contains("JSON object"));
    }

    #[test]
    fn missing_required_config_key_is_rejected() {
        let manifest = json!({ "configSchema": { "required": ["apiKey"] } });
        let err = validate_config(&manifest, &json!({ "other": 1 })).unwrap_err();
        assert!(err.contains("apiKey"));
    }

    #[test]
    fn valid_config_passes() {
        let manifest = json!({ "configSchema": { "required": ["apiKey"] } });
        assert!(validate_config(&manifest, &json!({ "apiKey": "k" })).is_ok());
    }

    #[test]
    fn no_required_schema_passes() {
        assert!(validate_config(&json!({}), &json!({ "x": 1 })).is_ok());
    }
}
