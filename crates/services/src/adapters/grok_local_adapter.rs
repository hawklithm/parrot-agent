use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Grok Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Grok CLI is not
/// installed. Runtime execution performs the actual environment/authentication
/// checks separately.
pub struct GrokLocalAdapter;

impl GrokLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("grok-build", "Grok Build"),
            ("grok-2", "Grok 2"),
            ("grok-2-vision", "Grok 2 Vision"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for GrokLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for GrokLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::GrokLocal
    }

    fn label(&self) -> &str {
        "Grok Build"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "grok_local".to_string(),
            status: AdapterEnvironmentTestStatus::Pass,
            tested_at: chrono::Utc::now().to_rfc3339(),
            checks: Vec::new(),
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        true
    }

    fn supports_local_agent_jwt(&self) -> bool {
        true
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }

    fn get_config_schema(&self) -> models::AdapterConfigSchema {
        use models::AdapterConfigSchema as Schema;

        Schema {
            fields: vec![
                Field {
                    key: "command".to_string(),
                    label: "Command".to_string(),
                    description: Some("Grok CLI executable path. Defaults to `grok`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("grok")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Grok model id. Defaults to `grok-build`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("grok-build")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the grok process.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "env".to_string(),
                    label: "Environment Variables".to_string(),
                    description: Some("Optional KEY=VALUE environment variables.".to_string()),
                    field_type: "object".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
            ],
        }
    }

    fn get_runtime_command_spec(
        &self,
        config: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<models::AdapterRuntimeCommandSpec> {
        let command = config
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("grok")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "grok --version".to_string(),
            install_command: Some("npm install -g xai/grok-cli".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# grok_local agent configuration

Adapter: grok_local

The runtime uses the locally installed Grok CLI. Requires authentication via
`grok login`.

Fields:
- model: Grok model id. Defaults to `grok-build`.
- command: optional executable override; defaults to `grok`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables

The default invocation is:
`grok --single --output-format streaming-json <prompt>`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_grok_adapter_basic() {
        let adapter = GrokLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::GrokLocal);
        assert_eq!(adapter.label(), "Grok Build");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_grok_adapter_test_environment() {
        let adapter = GrokLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "grok_local");
    }

    #[test]
    fn test_grok_adapter_models() {
        let adapter = GrokLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "grok-build"));
    }

    #[test]
    fn test_grok_adapter_config_schema() {
        let adapter = GrokLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
    }

    #[test]
    fn test_grok_adapter_runtime_command_spec() {
        let adapter = GrokLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "grok");
        assert_eq!(spec.detect_command, "grok --version");
        assert!(spec.install_command.is_some());
    }
}
