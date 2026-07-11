use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;

use super::adapter_trait::{AdapterType, ModelInfo, ServerAdapterModule, TestEnvironmentResult};

/// Claude Local Adapter - 对接 Claude Code CLI
pub struct ClaudeLocalAdapter;

impl ClaudeLocalAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ClaudeLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for ClaudeLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::ClaudeLocal
    }

    fn label(&self) -> &str {
        "Claude Code (Local)"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>> {
        // 返回 Claude 可用模型列表
        Ok(vec![
            ModelInfo {
                id: "claude-opus-4".to_string(),
                name: "Claude Opus 4".to_string(),
                provider: Some("anthropic".to_string()),
            },
            ModelInfo {
                id: "claude-sonnet-4".to_string(),
                name: "Claude Sonnet 4".to_string(),
                provider: Some("anthropic".to_string()),
            },
            ModelInfo {
                id: "claude-haiku-4".to_string(),
                name: "Claude Haiku 4".to_string(),
                provider: Some("anthropic".to_string()),
            },
        ])
    }

    async fn test_environment(&self, config: &serde_json::Value) -> anyhow::Result<TestEnvironmentResult> {
        // 验证 API Key 与连通性
        let api_key = config.get("api_key").and_then(|v| v.as_str());

        if api_key.is_none() || api_key.unwrap().is_empty() {
            return Ok(TestEnvironmentResult {
                success: false,
                message: "API key is required for Claude Local adapter".to_string(),
                details: Some({
                    let mut map = HashMap::new();
                    map.insert("error".to_string(), json!("missing_api_key"));
                    map
                }),
            });
        }

        // TODO: 实际调用 Claude API 验证连通性
        // 这里简化处理，假设 API key 格式正确即可
        Ok(TestEnvironmentResult {
            success: true,
            message: "Claude Local adapter configured successfully".to_string(),
            details: Some({
                let mut map = HashMap::new();
                map.insert("adapter_type".to_string(), json!("claude_local"));
                map.insert("api_key_configured".to_string(), json!(true));
                map
            }),
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        true
    }

    async fn detect_model(&self, config: &serde_json::Value) -> anyhow::Result<Option<String>> {
        // 尝试从配置中检测模型
        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            return Ok(Some(model.to_string()));
        }

        // 默认返回 Sonnet
        Ok(Some("claude-sonnet-4".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_claude_local_adapter() {
        let adapter = ClaudeLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::ClaudeLocal);
        assert_eq!(adapter.label(), "Claude Code (Local)");
        assert!(adapter.supports_instructions_bundle());

        let models = adapter.list_models().await.unwrap();
        assert!(models.len() >= 3);
        assert!(models.iter().any(|m| m.id == "claude-opus-4"));
    }

    #[tokio::test]
    async fn test_environment_without_api_key() {
        let adapter = ClaudeLocalAdapter::new();
        let result = adapter.test_environment(&json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.message.contains("API key"));
    }

    #[tokio::test]
    async fn test_environment_with_api_key() {
        let adapter = ClaudeLocalAdapter::new();
        let config = json!({"api_key": "sk-ant-test-key"});
        let result = adapter.test_environment(&config).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_detect_model() {
        let adapter = ClaudeLocalAdapter::new();

        let config = json!({"model": "claude-opus-4"});
        let model = adapter.detect_model(&config).await.unwrap();
        assert_eq!(model, Some("claude-opus-4".to_string()));

        let empty_config = json!({});
        let default_model = adapter.detect_model(&empty_config).await.unwrap();
        assert_eq!(default_model, Some("claude-sonnet-4".to_string()));
    }
}
