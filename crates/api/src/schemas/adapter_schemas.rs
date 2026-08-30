use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Adapter 信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfoResponse {
    /// 适配器类型
    pub adapter_type: String,

    /// 显示标签
    pub label: String,

    /// 支持的模型列表
    pub models: Vec<AdapterModelResponse>,

    /// 配置 schema
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<serde_json::Value>,

    /// 是否支持指令包
    pub supports_instructions_bundle: bool,

    /// 指令路径配置键名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions_path_key: Option<String>,

    /// Agent 配置文档
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_configuration_doc: Option<String>,

    /// 能力集合（取自真实 adapter capability，对齐 Paperclip `AdapterInfo.capabilities`）
    pub capabilities: AdapterCapabilities,
}

/// Adapter 模型响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterModelResponse {
    /// 模型 ID
    pub id: String,

    /// 显示标签
    pub label: String,
}

/// 测试 Adapter 环境请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAdapterEnvironmentRequest {
    /// 适配器配置
    pub adapter_config: serde_json::Value,

    /// 环境 ID（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<Uuid>,

    /// 是否需要租约
    #[serde(default)]
    pub with_lease: bool,

    /// 是否需要工作空间
    #[serde(default)]
    pub with_workspace: bool,
}

/// 测试 Adapter 环境响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAdapterEnvironmentResponse {
    /// 适配器类型
    pub adapter_type: String,

    /// 测试状态
    pub status: AdapterEnvironmentTestStatus,

    /// 测试时间
    pub tested_at: String,

    /// 检查项列表
    pub checks: Vec<AdapterEnvironmentCheck>,
}

/// Adapter 环境测试状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterEnvironmentTestStatus {
    Pass,
    Warning,
    Fail,
}

/// Adapter 环境检查项
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterEnvironmentCheck {
    /// 检查项名称
    pub name: String,

    /// 检查状态
    pub status: AdapterEnvironmentTestStatus,

    /// 检查消息
    pub message: String,

    /// 详细信息（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// 列出 Adapter 模型响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAdapterModelsResponse {
    /// 适配器类型
    pub adapter_type: String,

    /// 模型列表
    pub models: Vec<AdapterModelResponse>,
}

/// 检测模型请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectModelRequest {
    /// 适配器配置
    pub adapter_config: serde_json::Value,
}

/// 检测模型响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectModelResponse {
    /// 检测到的模型
    pub model: Option<String>,

    /// 检测状态
    pub status: ModelDetectionStatus,

    /// 消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Paperclip-compatible response for the body-less model detection endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAdapterModelResponse {
    pub model: String,
    pub provider: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

/// 模型检测状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelDetectionStatus {
    Success,
    NotFound,
    Error,
}

/// 列出所有 Adapter 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAdaptersResponse {
    /// Adapter 列表
    pub adapters: Vec<AdapterInfoResponse>,
}

/// 全局适配器信息响应（对齐 Paperclip 的 AdapterInfo）
/// 用于 GET /adapters 端点
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalAdapterInfo {
    /// 适配器类型
    #[serde(rename = "type")]
    pub adapter_type: String,

    /// 显示标签
    pub label: String,

    /// 来源：builtin 或 external
    pub source: String,

    /// 模型数量
    pub models_count: usize,

    /// 是否已加载
    pub loaded: bool,

    /// 是否被禁用
    pub disabled: bool,

    /// 能力集合
    pub capabilities: AdapterCapabilities,

    /// 是否为被外部覆盖的内置适配器
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overridden_builtin: Option<bool>,

    /// 内置适配器的外部覆盖是否被暂停
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_paused: Option<bool>,

    /// 版本号
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// npm 包名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,

    /// 是否为本地路径安装
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_local_path: Option<bool>,
}

/// 适配器能力集合
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCapabilities {
    /// 支持指令包
    pub supports_instructions_bundle: bool,

    /// 支持技能
    pub supports_skills: bool,

    /// 支持本地 Agent JWT
    pub supports_local_agent_jwt: bool,

    /// 需要物化的运行时技能
    pub requires_materialized_runtime_skills: bool,

    /// 支持模型配置文件
    pub supports_model_profiles: bool,

    /// 支持 ACP (Agent Control Protocol)
    pub supports_acp: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_info_response_serialization() {
        let response = AdapterInfoResponse {
            adapter_type: "claude_local".to_string(),
            label: "Claude Local".to_string(),
            models: vec![
                AdapterModelResponse {
                    id: "claude-opus-4".to_string(),
                    label: "Claude Opus 4".to_string(),
                },
            ],
            config_schema: None,
            supports_instructions_bundle: true,
            instructions_path_key: Some("instructionsFilePath".to_string()),
            agent_configuration_doc: None,
            capabilities: AdapterCapabilities {
                supports_instructions_bundle: true,
                supports_skills: true,
                supports_local_agent_jwt: false,
                requires_materialized_runtime_skills: false,
                supports_model_profiles: true,
                supports_acp: true,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("claude_local"));
        assert!(json.contains("instructionsFilePath"));
    }

    #[test]
    fn test_test_environment_request_deserialization() {
        let json = r#"{
            "adapterConfig": {"apiKey": "test"},
            "environmentId": "00000000-0000-0000-0000-000000000001",
            "withLease": true
        }"#;

        let request: TestAdapterEnvironmentRequest = serde_json::from_str(json).unwrap();
        assert!(request.with_lease);
        assert!(request.environment_id.is_some());
        assert!(!request.with_workspace);
    }

    #[test]
    fn test_test_environment_status() {
        let status = AdapterEnvironmentTestStatus::Pass;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""pass""#);

        let parsed: AdapterEnvironmentTestStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AdapterEnvironmentTestStatus::Pass);
    }
}
