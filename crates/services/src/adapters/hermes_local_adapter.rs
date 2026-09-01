use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Hermes Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Hermes CLI is not
/// installed. Runtime execution performs the actual environment/authentication
/// checks separately.
pub struct HermesLocalAdapter;

impl HermesLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("auto", "Auto (resolve from ~/.hermes/config.yaml)"),
            ("openrouter", "OpenRouter"),
            ("nous/hermes-3", "Nous Hermes 3"),
            ("openai-codex/gpt-4o", "OpenAI Codex GPT-4o"),
            ("anthropic/claude-3-5-sonnet", "Anthropic Claude 3.5 Sonnet"),
            ("zai/glm-4", "Z.AI GLM-4"),
            ("kimi-coding/kimi-k2.5", "Kimi K2.5"),
            ("minimax/minimax-01", "MiniMax 01"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for HermesLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for HermesLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::HermesLocal
    }

    fn label(&self) -> &str {
        "Hermes Agent"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "hermes_local".to_string(),
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
                    description: Some("Hermes CLI executable path. Defaults to `hermes`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("hermes")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Model id passed to Hermes. Defaults to `auto`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("auto")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the hermes process.".to_string()),
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
            .unwrap_or("hermes")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "hermes --version".to_string(),
            install_command: Some("pip install hermes-agent".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# hermes_local agent configuration

Adapter: hermes_local

The runtime uses the locally installed Hermes Agent CLI. Hermes supports any
model via any provider.

Fields:
- model: model id passed to Hermes. Defaults to `auto` (resolve from ~/.hermes/config.yaml)
- command: optional executable override; defaults to `hermes`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables

The default invocation is:
`hermes chat --provider auto --model <model> <prompt>`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_hermes_adapter_basic() {
        let adapter = HermesLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::HermesLocal);
        assert_eq!(adapter.label(), "Hermes Agent");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_hermes_adapter_test_environment() {
        let adapter = HermesLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "hermes_local");
    }

    #[test]
    fn test_hermes_adapter_models() {
        let adapter = HermesLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "auto"));
        assert!(models.iter().any(|m| m.id == "openrouter"));
    }

    #[test]
    fn test_hermes_adapter_config_schema() {
        let adapter = HermesLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
    }

    #[test]
    fn test_hermes_adapter_runtime_command_spec() {
        let adapter = HermesLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "hermes");
        assert_eq!(spec.detect_command, "hermes --version");
        assert!(spec.install_command.is_some());
    }
}
