use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PluginCapabilities {
    pub tools: Vec<Value>,
    pub actions: Vec<Value>,
    pub jobs: Vec<Value>,
    pub ui_contributions: Vec<Value>,
    pub capabilities: Vec<String>,
}

pub fn parse_manifest(manifest: &Value) -> Result<PluginCapabilities, String> {
    if !manifest.is_object() { return Err("plugin manifest must be an object".into()); }
    let array = |key: &str| manifest.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let capabilities = manifest.get("capabilities").and_then(Value::as_array).map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();
    Ok(PluginCapabilities { tools: array("tools"), actions: array("actions"), jobs: array("jobs"), ui_contributions: manifest.get("uiContributions").or_else(||manifest.get("ui_contributions")).and_then(Value::as_array).cloned().unwrap_or_default(), capabilities })
}
