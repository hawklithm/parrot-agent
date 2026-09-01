use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Gemini Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Gemini CLI or
/// Google AI API key is not available. Runtime execution performs the actual
/// environment/authentication checks separately.
pub struct GeminiLocalAdapter;

impl GeminiLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("gemini-2.5-pro", "Gemini 2.5 Pro"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
            ("gemini-2.5-flash-lite", "Gemini 2.5 Flash Lite"),
            ("gemini-2.0-flash", "Gemini 2.0 Flash"),
            ("gemini-2.0-flash-thinking", "Gemini 2.0 Flash Thinking"),
            ("gemini-exp-1206", "Gemini Exp 1206"),
            ("gemini-2.5-flash-preview-09-2025", "Gemini 2.5 Flash Preview"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for GeminiLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for GeminiLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::GeminiLocal
    }

    fn label(&self) -> &str {
        "Gemini"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "gemini_local".to_string(),
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
        use models::ConfigFieldSchema as Field;

        Schema {
            fields: vec![
                Field {
                    key: "command".to_string(),
                    label: "Command".to_string(),
                    description: Some("Gemini CLI executable path. Defaults to `gemini`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("gemini")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Model id passed to Gemini CLI.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the gemini process.".to_string()),
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
            .unwrap_or("gemini")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "gemini --version".to_string(),
            install_command: Some("npm install -g @google/generative-ai-cli".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# gemini_local agent configuration

Adapter: gemini_local

The runtime uses the locally installed Gemini CLI. No API key is required in
adapterConfig when the CLI is already authenticated via `gemini auth login`.

Fields:
- model: model passed to `gemini chat` or `gemini generate`
- command: optional executable override; defaults to `gemini`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables

The default invocation is:
`gemini chat --model <model> --prompt "<prompt>"`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_gemini_adapter_basic() {
        let adapter = GeminiLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::GeminiLocal);
        assert_eq!(adapter.label(), "Gemini");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_gemini_adapter_test_environment() {
        let adapter = GeminiLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "gemini_local");
    }

    #[test]
    fn test_gemini_adapter_models() {
        let adapter = GeminiLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gemini-2.5-pro"));
        assert!(models.iter().any(|m| m.id == "gemini-2.5-flash"));
        assert!(models.iter().any(|m| m.id == "gemini-exp-1206"));
    }

    #[test]
    fn test_gemini_adapter_config_schema() {
        let adapter = GeminiLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
        assert!(schema.fields.iter().any(|f| f.key == "cwd"));
        assert!(schema.fields.iter().any(|f| f.key == "env"));
    }

    #[test]
    fn test_gemini_adapter_runtime_command_spec() {
        let adapter = GeminiLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "gemini");
        assert_eq!(spec.detect_command, "gemini --version");
        assert_eq!(spec.install_command, Some("npm install -g @google/generative-ai-cli".to_string()));
    }
}
