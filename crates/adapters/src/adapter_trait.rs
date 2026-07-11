use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Adapter 类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    Process,
    ClaudeLocal,
    Cursor,
    OpenCode,
    CodexLocal,
}

impl std::fmt::Display for AdapterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterType::Process => write!(f, "process"),
            AdapterType::ClaudeLocal => write!(f, "claude_local"),
            AdapterType::Cursor => write!(f, "cursor"),
            AdapterType::OpenCode => write!(f, "opencode"),
            AdapterType::CodexLocal => write!(f, "codex_local"),
        }
    }
}

impl std::str::FromStr for AdapterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "process" => Ok(AdapterType::Process),
            "claude_local" => Ok(AdapterType::ClaudeLocal),
            "cursor" => Ok(AdapterType::Cursor),
            "opencode" => Ok(AdapterType::OpenCode),
            "codex_local" => Ok(AdapterType::CodexLocal),
            _ => Err(format!("Unknown adapter type: {}", s)),
        }
    }
}

/// Model 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
}

/// 环境测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEnvironmentResult {
    pub success: bool,
    pub message: String,
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// 环境测试输入参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEnvironmentInput {
    /// 适配器配置（如API Key、连接信息等）
    pub adapter_config: serde_json::Value,

    /// 环境ID（可选，用于指定特定环境）
    pub environment_id: Option<String>,

    /// 是否需要租约（默认false，仅连通性测试）
    #[serde(default)]
    pub with_lease: bool,

    /// 是否需要实例化工作区（默认false）
    #[serde(default)]
    pub with_workspace: bool,
}

/// ServerAdapter trait - 定义适配器接口
#[async_trait]
pub trait ServerAdapterModule: Send + Sync {
    /// 获取适配器类型
    fn adapter_type(&self) -> AdapterType;

    /// 获取适配器标签
    fn label(&self) -> &str;

    /// 获取支持的模型列表
    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>>;

    /// 测试环境连通性
    ///
    /// # 参数
    /// - config: 适配器配置（向后兼容，简单测试场景）
    ///
    /// # 返回
    /// - Ok(TestEnvironmentResult): 测试结果
    /// - Err: 测试失败
    async fn test_environment(&self, config: &serde_json::Value) -> anyhow::Result<TestEnvironmentResult>;

    /// 测试环境连通性（增强版，支持租约与工作区）
    ///
    /// # 参数
    /// - input: 测试输入参数（包含配置、环境ID、租约/工作区选项）
    ///
    /// # 返回
    /// - Ok(TestEnvironmentResult): 测试结果
    /// - Err: 测试失败
    ///
    /// # 默认实现
    /// 默认调用 test_environment(config)，子类可重写以支持租约/工作区
    async fn test_environment_enhanced(&self, input: TestEnvironmentInput) -> anyhow::Result<TestEnvironmentResult> {
        // 默认实现：忽略租约和工作区选项，仅测试配置
        self.test_environment(&input.adapter_config).await
    }

    /// 是否支持指令集
    fn supports_instructions_bundle(&self) -> bool {
        false
    }

    /// 检测可用模型
    async fn detect_model(&self, config: &serde_json::Value) -> anyhow::Result<Option<String>> {
        let _ = config;
        Ok(None)
    }
}
