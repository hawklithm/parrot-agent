use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in OpenClaw Gateway adapter.
///
/// Remote adapter that connects to an OpenClaw gateway via WebSocket.
pub struct OpenclawGatewayAdapter;

impl OpenclawGatewayAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OpenclawGatewayAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for OpenclawGatewayAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::OpenclawGateway
    }

    fn label(&self) -> &str {
        "OpenClaw Gateway"
    }

    fn models(&self) -> Vec<AdapterModel> {
        // No static catalog — models are discovered from the gateway
        Vec::new()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "openclaw_gateway".to_string(),
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
                    key: "url".to_string(),
                    label: "Gateway URL".to_string(),
                    description: Some("OpenClaw gateway WebSocket URL (ws:// or wss://).".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
                Field {
                    key: "authToken".to_string(),
                    label: "Auth Token".to_string(),
                    description: Some("Shared gateway token override.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "password".to_string(),
                    label: "Password".to_string(),
                    description: Some("Gateway shared password, if configured.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "clientId".to_string(),
                    label: "Client ID".to_string(),
                    description: Some("Gateway client id (default: gateway-client).".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("gateway-client")),
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
        // Gateway adapter doesn't have a local command spec
        None
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# openclaw_gateway agent configuration

Adapter: openclaw_gateway

Use when:
- You want Paperclip to invoke OpenClaw over the Gateway WebSocket protocol.
- You want native gateway auth/connect semantics instead of HTTP /v1/responses.

Don't use when:
- You only expose OpenClaw HTTP endpoints.
- Your deployment does not permit outbound WebSocket access from the Paperclip server.

Required fields:
- url: OpenClaw gateway WebSocket URL (ws:// or wss://)

Optional fields:
- authToken: shared gateway token override
- password: gateway shared password
- clientId: gateway client id (default: gateway-client)
- scopes: gateway scopes (default: ["operator.admin"])
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_openclaw_gateway_adapter_basic() {
        let adapter = OpenclawGatewayAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::OpenclawGateway);
        assert_eq!(adapter.label(), "OpenClaw Gateway");
        assert!(adapter.supports_instructions_bundle());
        assert!(!adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_openclaw_gateway_adapter_test_environment() {
        let adapter = OpenclawGatewayAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "openclaw_gateway");
    }

    #[test]
    fn test_openclaw_gateway_adapter_models() {
        let adapter = OpenclawGatewayAdapter::new();
        let models = adapter.models();
        assert!(models.is_empty());
    }

    #[test]
    fn test_openclaw_gateway_adapter_config_schema() {
        let adapter = OpenclawGatewayAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "url"));
        assert!(schema.fields.iter().any(|f| f.key == "authToken"));
    }

    #[test]
    fn test_openclaw_gateway_adapter_runtime_command_spec() {
        let adapter = OpenclawGatewayAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new());
        assert!(spec.is_none());
    }
}
