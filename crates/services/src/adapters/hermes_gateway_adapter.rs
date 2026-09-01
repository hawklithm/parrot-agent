use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Hermes Gateway adapter.
///
/// Remote adapter that connects to a Hermes Agent API server via HTTP/SSE.
pub struct HermesGatewayAdapter;

impl HermesGatewayAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HermesGatewayAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for HermesGatewayAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::HermesGateway
    }

    fn label(&self) -> &str {
        "Hermes Gateway"
    }

    fn models(&self) -> Vec<AdapterModel> {
        // No static catalog — models are discovered from the remote server
        Vec::new()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "hermes_gateway".to_string(),
            status: AdapterEnvironmentTestStatus::Pass,
            tested_at: chrono::Utc::now().to_rfc3339(),
            checks: Vec::new(),
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        true
    }

    fn supports_local_agent_jwt(&self) -> bool {
        false
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }

    fn get_config_schema(&self) -> models::AdapterConfigSchema {
        use models::AdapterConfigSchema as Schema;

        Schema {
            fields: vec![
                Field {
                    key: "apiBaseUrl".to_string(),
                    label: "API Base URL".to_string(),
                    description: Some("Hermes API server base URL (e.g. http://127.0.0.1:8642).".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
                Field {
                    key: "apiKey".to_string(),
                    label: "API Key".to_string(),
                    description: Some("Hermes API_SERVER_KEY. Sent as Authorization: Bearer <apiKey>.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
                Field {
                    key: "paperclipApiUrl".to_string(),
                    label: "Paperclip API URL".to_string(),
                    description: Some("Paperclip API URL reachable from the Hermes host.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "sessionKeyStrategy".to_string(),
                    label: "Session Key Strategy".to_string(),
                    description: Some("issue | agent | run | none (default: issue).".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("issue")),
                    options: None,
                    required: false,
                },
            ],
        }
    }

    fn get_runtime_command_spec(
        &self,
        _config: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<models::AdapterRuntimeCommandSpec> {
        // Gateway adapters don't have a local command spec
        None
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# hermes_gateway agent configuration

Adapter: hermes_gateway

Use when:
- Hermes Agent runs on another host or process that exposes the Hermes API server.
- Paperclip should create Hermes runs through POST /v1/runs and observe them through SSE events.
- You need remote Hermes session continuity with X-Hermes-Session-Key.

Don't use when:
- Hermes should run as a local child process on the Paperclip host; use hermes_local instead.
- The Hermes API server is not enabled or is only reachable over an unsafe public HTTP endpoint.

Required fields:
- apiBaseUrl: Hermes API server base URL (e.g. http://127.0.0.1:8642)
- apiKey: Hermes API_SERVER_KEY

Optional fields:
- paperclipApiUrl: Paperclip API URL reachable from the Hermes host
- sessionKeyStrategy: issue | agent | run | none (defaults to issue)
- timeoutSec: adapter timeout in seconds (default 600)
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_hermes_gateway_adapter_basic() {
        let adapter = HermesGatewayAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::HermesGateway);
        assert_eq!(adapter.label(), "Hermes Gateway");
        assert!(adapter.supports_instructions_bundle());
        assert!(!adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_hermes_gateway_adapter_test_environment() {
        let adapter = HermesGatewayAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "hermes_gateway");
    }

    #[test]
    fn test_hermes_gateway_adapter_models() {
        let adapter = HermesGatewayAdapter::new();
        let models = adapter.models();
        assert!(models.is_empty());
    }

    #[test]
    fn test_hermes_gateway_adapter_config_schema() {
        let adapter = HermesGatewayAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "apiBaseUrl"));
        assert!(schema.fields.iter().any(|f| f.key == "apiKey"));
    }

    #[test]
    fn test_hermes_gateway_adapter_runtime_command_spec() {
        let adapter = HermesGatewayAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new());
        assert!(spec.is_none());
    }
}
