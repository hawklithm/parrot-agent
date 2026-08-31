use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PluginCapabilities {
    pub tools: Vec<Value>,
    pub actions: Vec<Value>,
    pub jobs: Vec<Value>,
    pub ui_contributions: Vec<Value>,
    pub capabilities: Vec<String>,
}

/// Parse and validate a plugin manifest.
///
/// Capabilities are validated against Paperclip's canonical
/// `PLUGIN_CAPABILITIES` (PLUGIN_SPEC §15) — an unknown capability is a hard
/// error so a plugin written for a newer host fails loudly at load time
/// instead of silently running with misunderstood permissions. Manifests that
/// contribute `tools` must also declare `agent.tools.register` (§11).
pub fn parse_manifest(manifest: &Value) -> Result<PluginCapabilities, String> {
    if !manifest.is_object() { return Err("plugin manifest must be an object".into()); }
    let array = |key: &str| manifest.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let capabilities: Vec<String> = manifest.get("capabilities").and_then(Value::as_array).map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default();

    for cap in &capabilities {
        crate::plugin_capabilities::parse_capability(cap)
            .map_err(|e| format!("invalid plugin manifest: {e}"))?;
    }
    let tools = array("tools");
    if !tools.is_empty()
        && !crate::plugin_capabilities::has_capability(&capabilities, "agent.tools.register")
    {
        return Err(
            "invalid plugin manifest: tools declared without 'agent.tools.register' capability"
                .into(),
        );
    }

    Ok(PluginCapabilities { tools, actions: array("actions"), jobs: array("jobs"), ui_contributions: manifest.get("uiContributions").or_else(||manifest.get("ui_contributions")).and_then(Value::as_array).cloned().unwrap_or_default(), capabilities })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_manifest_with_known_capabilities() {
        let manifest = json!({
            "tools": [{"name": "search", "displayName": "S", "description": "d", "parametersSchema": {}}],
            "jobs": [{"key": "sync"}],
            "capabilities": ["issues.read", "issue.comments.create_human_attributed", "secrets.read-ref", "agent.tools.register"],
        });
        let parsed = parse_manifest(&manifest).unwrap();
        assert_eq!(parsed.capabilities.len(), 4);
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.jobs.len(), 1);
    }

    #[test]
    fn tools_require_agent_tools_register() {
        let ok = json!({
            "capabilities": ["agent.tools.register"],
            "tools": [{"name": "search"}]
        });
        assert!(parse_manifest(&ok).is_ok());

        let missing = json!({ "capabilities": ["issues.read"], "tools": [{"name": "search"}] });
        let err = parse_manifest(&missing).unwrap_err();
        assert!(err.contains("agent.tools.register"), "got: {err}");
    }

    #[test]
    fn unknown_capability_is_rejected() {
        let manifest = json!({ "capabilities": ["issues.read", "totally.made.up"] });
        let err = parse_manifest(&manifest).unwrap_err();
        assert!(err.contains("totally.made.up"), "got: {err}");
    }

    #[test]
    fn empty_capabilities_and_no_tools_is_valid() {
        let parsed = parse_manifest(&json!({})).unwrap();
        assert!(parsed.capabilities.is_empty());
        assert!(parsed.tools.is_empty());
    }

    #[test]
    fn non_object_manifest_is_rejected() {
        assert!(parse_manifest(&json!([])).is_err());
    }
}
