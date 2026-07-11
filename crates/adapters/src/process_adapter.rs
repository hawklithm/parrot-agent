use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use super::adapter_trait::{AdapterType, ModelInfo, ServerAdapterModule, TestEnvironmentResult};

/// Process Adapter - 基础的本地进程适配器
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
        "Process (Local)"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        // Process 适配器返回空列表或本地可用模型
        Ok(vec![])
    }

    async fn test_environment(&self, _config: &serde_json::Value) -> anyhow::Result<TestEnvironmentResult> {
        // 基础连通性测试 - Process 适配器始终可用
        Ok(TestEnvironmentResult {
            success: true,
            message: "Process adapter is available".to_string(),
            details: Some({
                let mut map = HashMap::new();
                map.insert("adapter_type".to_string(), json!("process"));
                map.insert("local".to_string(), json!(true));
                map
            }),
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_process_adapter() {
        let adapter = ProcessAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::Process);
        assert_eq!(adapter.label(), "Process (Local)");

        let models = adapter.list_models().await.unwrap();
        assert_eq!(models.len(), 0);

        let result = adapter.test_environment(&json!({})).await.unwrap();
        assert!(result.success);
    }
}
