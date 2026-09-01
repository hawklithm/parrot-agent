use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Cursor adapter.
///
/// Paperclip exposes the built-in model catalog even when the Cursor CLI is not
/// installed. Runtime execution performs the actual environment/authentication
/// checks separately.
pub struct CursorLocalAdapter;

impl CursorLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("auto", "Auto"),
            ("composer-1.5", "Composer 1.5"),
            ("composer-1", "Composer 1"),
            ("gpt-5.3-codex", "GPT-5.3 Codex"),
            ("gpt-5.3-codex-fast", "GPT-5.3 Codex Fast"),
            ("gpt-5.3-codex-high", "GPT-5.3 Codex High"),
            ("gpt-5.3-codex-high-fast", "GPT-5.3 Codex High Fast"),
            ("gpt-5.3-codex-xhigh", "GPT-5.3 Codex XHigh"),
            ("gpt-5.3-codex-xhigh-fast", "GPT-5.3 Codex XHigh Fast"),
            ("gpt-5.2", "GPT-5.2"),
            ("gpt-5.2-codex", "GPT-5.2 Codex"),
            ("gpt-5.2-codex-fast", "GPT-5.2 Codex Fast"),
            ("gpt-5.2-codex-high", "GPT-5.2 Codex High"),
            ("gpt-5.2-codex-high-fast", "GPT-5.2 Codex High Fast"),
            ("opus-4.6-thinking", "Opus 4.6 Thinking"),
            ("opus-4.6", "Opus 4.6"),
            ("opus-4.5", "Opus 4.5"),
            ("opus-4.5-thinking", "Opus 4.5 Thinking"),
            ("sonnet-4.6", "Sonnet 4.6"),
            ("sonnet-4.6-thinking", "Sonnet 4.6 Thinking"),
            ("sonnet-4.5", "Sonnet 4.5"),
            ("gemini-3.1-pro", "Gemini 3.1 Pro"),
            ("gemini-3-pro", "Gemini 3 Pro"),
            ("gemini-3-flash", "Gemini 3 Flash"),
            ("grok", "Grok"),
            ("kimi-k2.5", "Kimi K2.5"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for CursorLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for CursorLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Cursor
    }

    fn label(&self) -> &str {
        "Cursor"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "cursor".to_string(),
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
                    description: Some("Cursor CLI executable path. Defaults to `agent`.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("agent")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Cursor model id.".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("auto")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "cwd".to_string(),
                    label: "Working Directory".to_string(),
                    description: Some("Optional working directory for the agent process.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "env".to_string(),
                    label: "Environment Variables".to_string(),
                    description: Some("Optional KEY=VALUE environment variables (e.g. CURSOR_API_KEY).".to_string()),
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
            .unwrap_or("agent")
            .to_string();
        Some(models::AdapterRuntimeCommandSpec {
            command,
            detect_command: "cursor-agent --version".to_string(),
            install_command: Some("curl https://cursor.com/install -fsS | bash".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# cursor agent configuration

Adapter: cursor

Use when:
- You want Paperclip to run Cursor Agent CLI locally as the agent runtime
- You want Cursor chat session resume across heartbeats via --resume
- You want structured stream output in run logs via --output-format stream-json

Don't use when:
- You need webhook-style external invocation (use openclaw_gateway or http)
- You only need one-shot shell commands (use process)
- Cursor Agent CLI is not installed on the machine

Core fields:
- cwd (string, optional): default absolute working directory fallback
- instructionsFilePath (string, optional): markdown instructions file prepended to the prompt
- promptTemplate (string, optional): run prompt template
- model (string, optional): Cursor model id (e.g. auto, gpt-5.3-codex)
- mode (string, optional): Cursor execution mode passed as --mode (plan|ask)
- command (string, optional): defaults to "agent"
- extraArgs (string[], optional): additional CLI args
- env (object, optional): KEY=VALUE environment variables

Operational fields:
- timeoutSec (number, optional): run timeout in seconds
- graceSec (number, optional): SIGTERM grace period in seconds
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_cursor_adapter_basic() {
        let adapter = CursorLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::Cursor);
        assert_eq!(adapter.label(), "Cursor");
        assert!(adapter.supports_instructions_bundle());
        assert!(adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_cursor_adapter_test_environment() {
        let adapter = CursorLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "cursor");
    }

    #[test]
    fn test_cursor_adapter_models() {
        let adapter = CursorLocalAdapter::new();
        let models = adapter.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "auto"));
        assert!(models.iter().any(|m| m.id == "gpt-5.3-codex"));
    }

    #[test]
    fn test_cursor_adapter_config_schema() {
        let adapter = CursorLocalAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "command"));
        assert!(schema.fields.iter().any(|f| f.key == "model"));
    }

    #[test]
    fn test_cursor_adapter_runtime_command_spec() {
        let adapter = CursorLocalAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new()).unwrap();
        assert_eq!(spec.command, "agent");
        assert_eq!(spec.detect_command, "cursor-agent --version");
        assert!(spec.install_command.is_some());
    }
}
