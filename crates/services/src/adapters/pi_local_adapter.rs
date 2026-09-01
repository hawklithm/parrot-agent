use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Pi Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Pi CLI is not
/// installed. Runtime execution performs the actual environment/authentication
/// checks separately.
pub struct PiLocalAdapter;

impl PiLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        // Pi supports multiple providers; models are discovered dynamically
        // via `pi --list-models`. Return an empty catalog here — the UI
        // should show "run pi --list-models to discover available models" instead.
        Vec::new()
    }
}

impl Default for PiLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for PiLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::PiLocal
    }

    fn label(&self) -> &str {
        "Pi"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "pi_local".to_string(),
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
                    description: Some("Pi CLI executable path. Defaults to `pi`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("pi")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Model id in provider/model format (e.g. xai/grok-4). Run `pi --list-models` to discover available models.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the pi process.".to_string()),
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
            .unwrap_or("pi")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "pi --version".to_string(),
            install_command: Some("npm install -g @earendil-works/pi-coding-agent@0.74.0".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# pi_local agent configuration

Adapter: pi_local

The runtime uses the locally installed Pi coding agent CLI.

Fields:
- model: model id in provider/model format (e.g. xai/grok-4)
- command: optional executable override; defaults to `pi`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables

The default invocation is:
`pi --provider <provider> --model <model> <prompt>`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_pi_adapter_basic() {
        let adapter = PiLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::PiLocal);
        assert_eq!(adapter.label(), "Pi");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_pi_adapter_test_environment() {
        let adapter = PiLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "pi_local");
    }

    #[test]
    fn test_pi_adapter_models() {
        let adapter = PiLocalAdapter::new();
        // Pi has no static catalog — models are discovered at runtime
        let models = adapter.models();
        assert!(models.is_empty());
    }

    #[test]
    fn test_pi_adapter_config_schema() {
        let adapter = PiLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
    }

    #[test]
    fn test_pi_adapter_runtime_command_spec() {
        let adapter = PiLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "pi");
        assert_eq!(spec.detect_command, "pi --version");
        assert!(spec.install_command.is_some());
    }
}
