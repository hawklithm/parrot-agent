use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    ConfigFieldSchema as Field, TestEnvironmentContext,
};
use crate::adapter_registry::ServerAdapterModule;

/// Built-in Cursor Cloud adapter.
///
/// Remote adapter that uses Cursor Cloud agents via the Cursor SDK.
pub struct CursorCloudAdapter;

impl CursorCloudAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CursorCloudAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for CursorCloudAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::CursorCloud
    }

    fn label(&self) -> &str {
        "Cursor Cloud"
    }

    fn models(&self) -> Vec<AdapterModel> {
        // Models are discovered from the Cursor account via the SDK
        Vec::new()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "cursor_cloud".to_string(),
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
                    key: "repoUrl".to_string(),
                    label: "Repository URL".to_string(),
                    description: Some("Git repository URL Cursor should open.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
                Field {
                    key: "runtimeEnvType".to_string(),
                    label: "Runtime Environment Type".to_string(),
                    description: Some("cloud | pool | machine".to_string()),
                    field_type: "string".to_string(),
                    default_value: Some(serde_json::json!("cloud")),
                    options: None,
                    required: false,
                },
                Field {
                    key: "model".to_string(),
                    label: "Model".to_string(),
                    description: Some("Cursor model id; omit to use the account default.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: false,
                },
                Field {
                    key: "env.CURSOR_API_KEY".to_string(),
                    label: "Cursor API Key".to_string(),
                    description: Some("Cursor API key. Required for cloud authentication.".to_string()),
                    field_type: "string".to_string(),
                    default_value: None,
                    options: None,
                    required: true,
                },
            ],
        }
    }

    fn get_runtime_command_spec(
        &self,
        _config: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<models::AdapterRuntimeCommandSpec> {
        // Cloud adapter doesn't have a local command spec
        None
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# cursor_cloud agent configuration

Adapter: cursor_cloud

Use when:
- You want Paperclip to run Cursor Cloud Agents through the official Cursor SDK
- You want durable remote Cursor agent sessions across Paperclip heartbeats
- You want Paperclip to keep task state while Cursor handles remote code execution

Don't use when:
- You need local agent execution; use cursor instead.
- Cursor Cloud SDK is not configured.

Required fields:
- repoUrl: Git repository URL Cursor should open
- env.CURSOR_API_KEY: Cursor API key

Optional fields:
- runtimeEnvType: cloud | pool | machine
- runtimeEnvName: named cloud/pool/machine target
- model: Cursor model id; omit to use the account default
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_cursor_cloud_adapter_basic() {
        let adapter = CursorCloudAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::CursorCloud);
        assert_eq!(adapter.label(), "Cursor Cloud");
        assert!(adapter.supports_instructions_bundle());
        assert!(!adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_cursor_cloud_adapter_test_environment() {
        let adapter = CursorCloudAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: std::collections::HashMap::new(),
            runtime_config: std::collections::HashMap::new(),
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "cursor_cloud");
    }

    #[test]
    fn test_cursor_cloud_adapter_models() {
        let adapter = CursorCloudAdapter::new();
        let models = adapter.models();
        assert!(models.is_empty());
    }

    #[test]
    fn test_cursor_cloud_adapter_config_schema() {
        let adapter = CursorCloudAdapter::new();
        let schema = adapter.get_config_schema();
        assert_eq!(schema.fields.len(), 4);
        assert!(schema.fields.iter().any(|f| f.key == "repoUrl"));
        assert!(schema.fields.iter().any(|f| f.key == "env.CURSOR_API_KEY"));
    }

    #[test]
    fn test_cursor_cloud_adapter_runtime_command_spec() {
        let adapter = CursorCloudAdapter::new();
        let spec = adapter.get_runtime_command_spec(&std::collections::HashMap::new());
        assert!(spec.is_none());
    }
}
