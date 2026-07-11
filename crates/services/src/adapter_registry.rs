use async_trait::async_trait;
use models::{
    AdapterConfigSchema, AdapterEnvironmentTestResult, AdapterModel, AdapterModelProfileDefinition,
    AdapterRuntimeCommandSpec, AdapterType, TestEnvironmentContext,
};
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait ServerAdapterModule: Send + Sync {
    fn adapter_type(&self) -> AdapterType;

    fn label(&self) -> &str {
        self.adapter_type().as_str()
    }

    fn models(&self) -> Vec<AdapterModel> {
        Vec::new()
    }

    async fn list_models(&self) -> Vec<AdapterModel> {
        self.models()
    }

    async fn test_environment(
        &self,
        ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>>;

    fn supports_instructions_bundle(&self) -> bool {
        false
    }

    fn instructions_path_key(&self) -> Option<&str> {
        None
    }

    fn supports_local_agent_jwt(&self) -> bool {
        false
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }

    fn agent_configuration_doc(&self) -> &str {
        ""
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

    fn model_profiles(&self) -> Vec<AdapterModelProfileDefinition> {
        Vec::new()
    }

    async fn list_model_profiles(&self) -> Vec<AdapterModelProfileDefinition> {
        self.model_profiles()
    }
}

pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn ServerAdapterModule>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn ServerAdapterModule>) {
        let adapter_type = adapter.adapter_type().as_str().to_string();
        self.adapters.insert(adapter_type, adapter);
    }

    pub fn find_server_adapter(&self, adapter_type: &str) -> Option<Arc<dyn ServerAdapterModule>> {
        self.adapters.get(adapter_type).cloned()
    }

    pub fn require_server_adapter(
        &self,
        adapter_type: &str,
    ) -> Result<Arc<dyn ServerAdapterModule>, String> {
        self.find_server_adapter(adapter_type)
            .ok_or_else(|| format!("Unknown adapter type: {}", adapter_type))
    }

    pub fn list_all(&self) -> Vec<Arc<dyn ServerAdapterModule>> {
        self.adapters.values().cloned().collect()
    }

    pub async fn list_adapter_models(&self, adapter_type: &str) -> Vec<AdapterModel> {
        if let Some(adapter) = self.find_server_adapter(adapter_type) {
            adapter.list_models().await
        } else {
            Vec::new()
        }
    }

    pub async fn list_adapter_model_profiles(
        &self,
        adapter_type: &str,
    ) -> Vec<AdapterModelProfileDefinition> {
        if let Some(adapter) = self.find_server_adapter(adapter_type) {
            adapter.list_model_profiles().await
        } else {
            Vec::new()
        }
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAdapter;

    #[async_trait]
    impl ServerAdapterModule for TestAdapter {
        fn adapter_type(&self) -> AdapterType {
            AdapterType::Process
        }

        async fn test_environment(
            &self,
            _ctx: &TestEnvironmentContext,
        ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
            Ok(AdapterEnvironmentTestResult {
                adapter_type: "process".to_string(),
                status: models::AdapterEnvironmentTestStatus::Pass,
                tested_at: chrono::Utc::now().to_rfc3339(),
                checks: Vec::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_find() {
        let mut registry = AdapterRegistry::new();
        let adapter = Arc::new(TestAdapter);

        registry.register(adapter.clone());

        let found = registry.find_server_adapter("process");
        assert!(found.is_some());

        let not_found = registry.find_server_adapter("unknown");
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_registry_list_all() {
        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(TestAdapter));

        let all = registry.list_all();
        assert_eq!(all.len(), 1);
    }
}
