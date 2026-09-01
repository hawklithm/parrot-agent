use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema, TestEnvironmentContext,
};

use crate::adapter_registry::ServerAdapterModule;

/// Built-in Codex Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Codex CLI or an
/// OpenAI API key is not available. Runtime execution performs the actual
/// environment/authentication checks separately.
pub struct CodexLocalAdapter;

impl CodexLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("gpt-5.6", "GPT-5.6"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6-luna", "GPT-5.6 Luna"),
            ("gpt-5.4", "GPT-5.4"),
            ("gpt-5.4-mini", "GPT-5.4 Mini"),
            ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
            ("gpt-5", "GPT-5"),
            ("o3", "o3"),
            ("o4-mini", "o4-mini"),
            ("gpt-5-mini", "GPT-5 Mini"),
            ("gpt-5-nano", "GPT-5 Nano"),
            ("o3-mini", "o3-mini"),
            ("codex-mini-latest", "Codex Mini"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for CodexLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for CodexLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::CodexLocal
    }

    fn label(&self) -> &str {
        "Codex"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "codex_local".to_string(),
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
                    description: Some("Codex CLI executable path. Defaults to `codex`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("codex")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Model id passed to `codex exec --model`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the codex process.".to_string()),
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
            .unwrap_or("codex")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "codex --version".to_string(),
            install_command: Some("npm install -g @openai/codex".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# codex_local agent configuration

Adapter: codex_local

The runtime uses the locally installed Codex CLI. No API key is required in
adapterConfig when the CLI is already authenticated.

Fields:
- model: model passed to `codex exec --model`
- command: optional executable override; defaults to `codex`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables

The default invocation is:
`codex exec --model <model> <prompt>`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_codex_adapter_basic() {
        let adapter = CodexLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::CodexLocal);
        assert_eq!(adapter.label(), "Codex");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_codex_adapter_test_environment() {
        let adapter = CodexLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "codex_local");
    }

    #[test]
    fn test_codex_adapter_models() {
        let adapter = CodexLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gpt-5.6"));
        assert!(models.iter().any(|m| m.id == "codex-mini-latest"));
    }

    #[test]
    fn test_codex_adapter_config_schema() {
        let adapter = CodexLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
        assert!(schema.fields.iter().any(|f| f.key == "cwd"));
        assert!(schema.fields.iter().any(|f| f.key == "env"));
    }

    #[test]
    fn test_codex_adapter_runtime_command_spec() {
        let adapter = CodexLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "codex");
        assert_eq!(spec.detect_command, "codex --version");
        assert_eq!(spec.install_command, Some("npm install -g @openai/codex".to_string()));
    }
}
