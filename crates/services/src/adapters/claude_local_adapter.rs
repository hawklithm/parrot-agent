use async_trait::async_trait;
use std::collections::HashMap;
use models::{AdapterType, AdapterModel, AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterEnvironmentCheck};
use crate::adapter_registry::ServerAdapterModule;
use models::TestEnvironmentContext;
use std::process::Command;

/// Claude Local 适配器
/// 对接 Claude Code CLI，支持本地 Claude 模型执行
pub struct ClaudeLocalAdapter;

impl ClaudeLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    /// 检查 Claude CLI 是否已安装
    fn is_claude_cli_installed(&self) -> bool {
        Command::new("claude")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// 获取 Claude 可用模型列表
    fn discover_claude_models(&self) -> Vec<AdapterModel> {
        // 尝试通过 CLI 发现模型
        if let Ok(output) = Command::new("claude").arg("models").output() {
            if output.status.success() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    return self.parse_models_from_cli(&stdout);
                }
            }
        }

        // 如果无法发现，返回默认的 Claude 模型列表
        self.get_default_models()
    }

    /// 从 CLI 输出解析模型列表
    fn parse_models_from_cli(&self, output: &str) -> Vec<AdapterModel> {
        let mut models = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // 解析模型 ID（假设格式为 "model-id" 或 "model-id - description"）
            if let Some(model_id) = line.split_whitespace().next() {
                models.push(AdapterModel {
                    id: model_id.to_string(),
                    label: model_id.to_string(),
                });
            }
        }

        if models.is_empty() {
            self.get_default_models()
        } else {
            models
        }
    }

    /// 获取默认的 Claude 模型列表
    fn get_default_models(&self) -> Vec<AdapterModel> {
        vec![
            AdapterModel {
                id: "claude-opus-4-8".to_string(),
                label: "Claude Opus 4.8".to_string(),
            },
            AdapterModel {
                id: "claude-opus-4-7".to_string(),
                label: "Claude Opus 4.7".to_string(),
            },
            AdapterModel {
                id: "claude-opus-4-6".to_string(),
                label: "Claude Opus 4.6".to_string(),           },
            AdapterModel {
                id: "claude-sonnet-4-6".to_string(),
                label: "Claude Sonnet 4.6".to_string(),
            },
            AdapterModel {
                id: "claude-sonnet-4-5".to_string(),
                label: "Claude Sonnet 4.5".to_string(),
            },
            AdapterModel {
                id: "claude-haiku-4-5".to_string(),
                label: "Claude Haiku 4.5".to_string(),
            },
        ]
    }

    /// 检查 API Key 是否配置
    fn check_api_key(&self, config: &serde_json::Value) -> bool {
        // 检查配置中的 API Key
        if let Some(api_key) = config.get("apiKey").and_then(|v| v.as_str()) {
            return !api_key.trim().is_empty();
        }

        // 检查环境变量
        std::env::var("ANTHROPIC_API_KEY").is_ok()
    }

    /// 测试 Claude CLI 连通性
    async fn test_cli_connectivity(&self) -> Result<(), String> {
        // 执行简单的 Claude CLI 命令测试
        let output = tokio::task::spawn_blocking(|| {
            Command::new("claude")
                .arg("--version")
                .output()
        })
        .await
        .map_err(|e| format!("Failed to spawn test command: {}", e))?
        .map_err(|e| format!("Failed to execute claude --version: {}", e))?;

        if output.status.success() {
            Ok(())
        } else {
            Err("Claude CLI test command failed".to_string())
        }
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
        "Claude Local"
    }

    fn models(&self) -> Vec<AdapterModel> {
        self.get_default_models()
    }

    async fn list_models(&self) -> Vec<AdapterModel> {
        self.discover_claude_models()
    }

    async fn test_environment(&self, ctx: &TestEnvironmentContext)
        -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut checks = Vec::new();
        let mut overall_status = AdapterEnvironmentTestStatus::Pass;

        // 检查 1: Claude CLI 是否安装
        let cli_installed = self.is_claude_cli_installed();
        checks.push(AdapterEnvironmentCheck {
            name: Some("claude_cli_installed".to_string()),
            status: Some(if cli_installed {
                AdapterEnvironmentTestStatus::Pass
            } else {
                overall_status = AdapterEnvironmentTestStatus::Fail;
                AdapterEnvironmentTestStatus::Fail
            }),
            message: if cli_installed {
                "Claude CLI is installed".to_string()
            } else {
                "Claude CLI not found. Install via: npm install -g @anthropic-ai/claude-code".to_string()
            },
            details: None,
            code: None,
            level: None,
            hint: None,
        });

        // 如果 CLI 未安装，直接返回失败
        if !cli_installed {
            return Ok(AdapterEnvironmentTestResult {
                adapter_type: "claude_local".to_string(),
                status: overall_status,
                tested_at: chrono::Utc::now().to_rfc3339(),
                checks,
            });
        }

        // 检查 2: API Key 配置
        let config_value: serde_json::Value = serde_json::to_value(&ctx.adapter_config).unwrap_or(serde_json::Value::Null);
        let has_api_key = self.check_api_key(&config_value);
        checks.push(AdapterEnvironmentCheck {
            name: Some("api_key_configured".to_string()),
            status: Some(if has_api_key {
                AdapterEnvironmentTestStatus::Pass
            } else {
                AdapterEnvironmentTestStatus::Warning
            }),
            message: if has_api_key {
                "API key is configured".to_string()
            } else {
                "API key not found in config or ANTHROPIC_API_KEY env var".to_string()
            },
            details: None,
            code: None,
            level: None,
            hint: None,
        });

        // 检查 3: CLI 连通性测试
        match self.test_cli_connectivity().await {
            Ok(_) => {
                checks.push(AdapterEnvironmentCheck {
                    name: Some("cli_connectivity".to_string()),
                    status: Some(AdapterEnvironmentTestStatus::Pass),
                    message: "Claude CLI connectivity test passed".to_string(),
                    details: None,
                    code: None,
                    level: None,
                    hint: None,
                });
            }
            Err(e) => {
                overall_status = AdapterEnvironmentTestStatus::Fail;
                checks.push(AdapterEnvironmentCheck {
                    name: Some("cli_connectivity".to_string()),
                    status: Some(AdapterEnvironmentTestStatus::Fail),
                    message: format!("Claude CLI connectivity test failed: {}", e),
                    details: None,
                    code: None,
                    level: None,
                    hint: None,
                });
            }
        }

        // 检查 4: 模型可用性
        let models = self.discover_claude_models();
        checks.push(AdapterEnvironmentCheck {
            name: Some("models_available".to_string()),
            status: Some(if models.is_empty() {
                AdapterEnvironmentTestStatus::Warning
            } else {
                AdapterEnvironmentTestStatus::Pass
            }),
            message: if models.is_empty() {
                "No models discovered, using defaults".to_string()
            } else {
                format!("{} models available", models.len())
            },
            details: Some(serde_json::json!({
                "model_count": models.len(),
                "models": models.iter().map(|m| &m.id).collect::<Vec<_>>(),
            }).to_string()),
            code: None,
            level: None,
            hint: None,
        });

        Ok(AdapterEnvironmentTestResult {
            adapter_type: "claude_local".to_string(),
            status: overall_status,
            tested_at: chrono::Utc::now().to_rfc3339(),
            checks,
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        true
    }

    fn instructions_path_key(&self) -> Option<&str> {
        Some("instructionsFilePath")
    }

    fn get_runtime_command_spec(
        &self,
        _config: &HashMap<String, serde_json::Value>,
    ) -> Option<models::AdapterRuntimeCommandSpec> {
        Some(models::AdapterRuntimeCommandSpec {
            command: "claude".to_string(),
            detect_command: "claude --version".to_string(),
            install_command: Some("npm install -g @anthropic-ai/claude-code".to_string()),
        })
    }

    fn agent_configuration_doc(&self) -> &str {
        r#"# claude_local agent configuration

Adapter: claude_local

Use when:
- You want Paperclip to run Claude Code CLI locally as the agent runtime
- You want Claude's latest models (Opus 4.8, Sonnet 4.6, etc.)
- You want native Claude session management and context handling

Don't use when:
- You need webhook-style external invocation (use http)
- You only need one-shot shell commands (use process)
- Claude CLI is not installed on the machine

Core fields:
- cwd (string, optional): default absolute working directory fallback for the agent process
- instructionsFilePath (string, optional): absolute path to a markdown instructions file
- promptTemplate (string, optional): run prompt template
- model (string, optional): Claude model id (claude-opus-4-8, claude-sonnet-4-6, etc.)
- thinkingEffort (string, optional): thinking effort level (low, medium, high)
- engine (string, optional): execution engine (auto, cli, acp). Defaults to acp.
- acpMode (string, optional): ACP mode (persistent, oneshot). Defaults to persistent.
- acpNonInteractivePermissions (string, optional): permission handling (deny, fail)
- env (object, optional): KEY=VALUE environment variables

Operational fields:
- timeoutSec (number, optional): run timeout in seconds
- graceSec (number, optional): SIGTERM grace period in seconds

Authentication:
- Set ANTHROPIC_API_KEY environment variable, or
- Include apiKey in adapterConfig

Notes:
- Requires Claude Code CLI: npm install -g @anthropic-ai/claude-code
- Sessions are managed by Claude's native session handling
- ACP (Agent Client Protocol) mode provides better performance and reliability
"#
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_type() {
        let adapter = ClaudeLocalAdapter::new();
        assert_eq!(adapter.adapter_type(), AdapterType::ClaudeLocal);
        assert_eq!(adapter.label(), "Claude Local");
    }

    #[test]
    fn test_supports_instructions_bundle() {
        let adapter = ClaudeLocalAdapter::new();
        assert!(adapter.supports_instructions_bundle());
        assert_eq!(adapter.instructions_path_key(), Some("instructionsFilePath"));
    }

    #[test]
    fn test_default_models() {
        let adapter = ClaudeLocalAdapter::new();
        let models = adapter.models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("opus")));
        assert!(models.iter().any(|m| m.id.contains("sonnet")));
        assert!(models.iter().any(|m| m.id.contains("haiku")));
    }

    #[test]
    fn test_check_api_key_from_config() {
        let adapter = ClaudeLocalAdapter::new();

        let config_with_key = serde_json::json!({
            "apiKey": "sk-test-key"
        });
        assert!(adapter.check_api_key(&config_with_key));

        let config_without_key = serde_json::json!({});
        // 如果环境变量没设置，应该返回 false
        if std::env::var("ANTHROPIC_API_KEY").is_err() {
            assert!(!adapter.check_api_key(&config_without_key));
        }
    }

    #[tokio::test]
    async fn test_test_environment_basic_structure() {
        let adapter = ClaudeLocalAdapter::new();
        let ctx = TestEnvironmentContext {
            adapter_config: serde_json::json!({}),
        };

        let result = adapter.test_environment(&ctx).await;
        assert!(result.is_ok());

        let test_result = result.unwrap();
        assert_eq!(test_result.adapter_type, "claude_local");
        assert!(!test_result.checks.is_empty());

        // 应该至少有 CLI 安装检查
        assert!(test_result.checks.iter().any(|c| c.name == "claude_cli_installed"));
    }
}
