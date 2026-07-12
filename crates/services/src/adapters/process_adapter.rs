use async_trait::async_trait;
use models::{
    AdapterConfigSchema, AdapterEnvironmentCheck, AdapterEnvironmentCheckLevel,
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel,
    AdapterModelProfileDefinition, AdapterRuntimeCommandSpec, AdapterType,
    TestEnvironmentContext,
};
use std::collections::HashMap;

use crate::adapter_registry::ServerAdapterModule;

pub struct ProcessAdapter;

impl ProcessAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProcessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for ProcessAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Process
    }

    fn label(&self) -> &str {
        "Process (Default)"
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "process".to_string(),
            status: AdapterEnvironmentTestStatus::Pass,
            tested_at: chrono::Utc::now().to_rfc3339(),
            checks: vec![AdapterEnvironmentCheck {
                code: Some("process_available".to_string()),
                level: Some(AdapterEnvironmentCheckLevel::Info),
                message: "Process adapter is always available".to_string(),
                hint: None,
                name: None,
                status: None,
                details: None,
            }],
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        false
    }

    fn supports_local_agent_jwt(&self) -> bool {
        false
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# Process Adapter

Adapter: process

The process adapter is the default fallback adapter. It provides basic agent execution
capabilities without requiring external dependencies or configuration.

## Features

- No external dependencies required
- Always available
- Basic execution environment
- No special configuration needed

## Configuration

The process adapter requires no configuration.

## Usage

The process adapter is automatically used when no other adapter is specified or available.
"#
    }

    fn get_config_schema(&self) -> AdapterConfigSchema {
        AdapterConfigSchema { fields: Vec::new() }
    }

    fn get_runtime_command_spec(
        &self,
        _config: &HashMap<String, serde_json::Value>,
    ) -> Option<AdapterRuntimeCommandSpec> {
        None
    }

    fn models(&self) -> Vec<AdapterModel> {
        Vec::new()
    }

    fn model_profiles(&self) -> Vec<AdapterModelProfileDefinition> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_process_adapter_basic() {
        let adapter = ProcessAdapter::new();

        assert_eq!(adapter.adapter_type(), AdapterType::Process);
        assert_eq!(adapter.label(), "Process (Default)");
        assert!(!adapter.supports_instructions_bundle());
        assert!(!adapter.supports_local_agent_jwt());
    }

    #[tokio::test]
    async fn test_process_adapter_test_environment() {
        let adapter = ProcessAdapter::new();
        let ctx = TestEnvironmentContext {
            company_id: Uuid::new_v4(),
            agent_id: None,
            adapter_config: HashMap::new(),
            runtime_config: HashMap::new(),
        };

        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, AdapterEnvironmentTestStatus::Pass);
        assert_eq!(result.adapter_type, "process");
        assert_eq!(result.checks.len(), 1);
    }
}
