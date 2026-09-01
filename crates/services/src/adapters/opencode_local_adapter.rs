use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in OpenCode adapter.
///
/// Paperclip exposes the built-in model catalog even when the OpenCode CLI is not
/// installed. Runtime execution performs the actual environment/authentication
/// checks separately.
pub struct OpencodeLocalAdapter;

impl OpencodeLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("openai/gpt-5.2-codex", "GPT-5.2 Codex"),
            ("openai/gpt-5.5", "GPT-5.5"),
            ("openai/gpt-5.4", "GPT-5.4"),
            ("openai/gpt-5.4-mini", "GPT-5.4 Mini"),
            ("openai/gpt-5.2", "GPT-5.2"),
            ("openai/gpt-5.1-codex-max", "GPT-5.1 Codex Max"),
            ("openai/gpt-5.1-codex-mini", "GPT-5.1 Codex Mini"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for OpencodeLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for OpencodeLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::OpencodeLocal
    }

    fn label(&self) -> &str {
        "OpenCode"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "opencode_local".to_string(),
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
                    description: Some("OpenCode CLI executable path. Defaults to `opencode`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("opencode")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Model id in provider/model format (e.g. openai/gpt-5.2-codex).".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("openai/gpt-5.2-codex")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the opencode process.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "env".to_string(),
                    label: "Environment Variables".to_string(),
                    description: Some("Optional KEY=VALUE environment variables (e.g. OPENAI_API_KEY).".to_string()),
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
            .unwrap_or("opencode")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "opencode --version".to_string(),
            install_command: Some("npm install -g opencode-ai".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# opencode_local agent configuration

Adapter: opencode_local

The runtime uses the locally installed OpenCode CLI. Requires OPENAI_API_KEY
in the environment or adapter config.

Fields:
- model: model id in provider/model format (e.g. openai/gpt-5.2-codex)
- command: optional executable override; defaults to `opencode`
- engine: `cli`
- cwd: optional working directory
- env: optional environment variables including OPENAI_API_KEY

The default invocation is:
`opencode -m <model> <prompt>`
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_opencode_adapter_basic() {
        let adapter = OpencodeLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::OpencodeLocal);
        assert_eq!(adapter.label(), "OpenCode");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_opencode_adapter_test_environment() {
        let adapter = OpencodeLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "opencode_local");
    }

    #[test]
    fn test_opencode_adapter_models() {
        let adapter = OpencodeLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "openai/gpt-5.2-codex"));
        assert!(models.iter().any(|m| m.id == "openai/gpt-5.5"));
    }

    #[test]
    fn test_opencode_adapter_config_schema() {
        let adapter = OpencodeLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
    }

    #[test]
    fn test_opencode_adapter_runtime_command_spec() {
        let adapter = OpencodeLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "opencode");
        assert_eq!(spec.detect_command, "opencode --version");
        assert!(spec.install_command.is_some());
    }
}
