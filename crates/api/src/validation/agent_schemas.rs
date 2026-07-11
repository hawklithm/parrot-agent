use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use models::{AgentRole, AgentStatus};

/// Agent 创建请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentHireSchema {
    /// Agent 名称
    #[garde(length(min = 1, max = 255))]
    pub name: String,

    /// Agent 角色
    #[garde(skip)]
    #[serde(default = "default_role")]
    pub role: AgentRole,

    /// Agent 标题（可选）
    #[garde(skip)]
    pub title: Option<String>,

    /// Agent 图标（可选）
    #[garde(skip)]
    pub icon: Option<String>,

    /// 上级 Agent ID（可选）
    #[garde(skip)]
    pub reports_to: Option<Uuid>,

    /// Agent 能力描述（可选）
    #[garde(skip)]
    pub capabilities: Option<String>,

    /// 期望的技能列表
    #[garde(skip)]
    pub desired_skills: Option<Vec<String>>,

    /// 适配器类型
    #[garde(length(min = 1))]
    pub adapter_type: String,

    /// 适配器配置
    #[garde(skip)]
    #[serde(default = "default_config")]
    pub adapter_config: JsonValue,

    /// 指令包配置
    #[garde(skip)]
    pub instructions_bundle: Option<InstructionsBundleInput>,

    /// 运行时配置
    #[garde(skip)]
    #[serde(default = "default_config")]
    pub runtime_config: JsonValue,

    /// 默认环境 ID
    #[garde(skip)]
    pub default_environment_id: Option<Uuid>,

    /// 月度预算（单位：美分）
    #[garde(range(min = 0))]
    #[serde(default)]
    pub budget_monthly_cents: i32,

    /// Agent 权限配置
    #[garde(skip)]
    pub permissions: Option<AgentPermissionsInput>,

    /// 元数据
    #[garde(skip)]
    pub metadata: Option<JsonValue>,

    /// 来源 Issue ID（用于跟踪创建来源）
    #[garde(skip)]
    pub source_issue_id: Option<Uuid>,

    /// 来源 Issue ID 列表
    #[garde(skip)]
    pub source_issue_ids: Option<Vec<Uuid>>,
}

/// Agent 更新请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentSchema {
    /// Agent 名称
    #[garde(length(min = 1, max = 255))]
    pub name: Option<String>,

    /// Agent 角色
    #[garde(skip)]
    pub role: Option<AgentRole>,

    /// Agent 标题
    #[garde(skip)]
    pub title: Option<Option<String>>,

    /// Agent 图标
    #[garde(skip)]
    pub icon: Option<Option<String>>,

    /// 上级 Agent ID
    #[garde(skip)]
    pub reports_to: Option<Option<Uuid>>,

    /// Agent 能力描述
    #[garde(skip)]
    pub capabilities: Option<Option<String>>,

    /// 期望的技能列表
    #[garde(skip)]
    pub desired_skills: Option<Vec<String>>,

    /// 适配器类型
    #[garde(skip)]
    pub adapter_type: Option<String>,

    /// 适配器配置
    #[garde(skip)]
    pub adapter_config: Option<JsonValue>,

    /// 是否完全替换适配器配置（默认为合并）
    #[garde(skip)]
    pub replace_adapter_config: Option<bool>,

    /// 指令包配置
    #[garde(skip)]
    pub instructions_bundle: Option<UpdateInstructionsBundleInput>,

    /// 运行时配置
    #[garde(skip)]
    pub runtime_config: Option<JsonValue>,

    /// 默认环境 ID
    #[garde(skip)]
    pub default_environment_id: Option<Option<Uuid>>,

    /// 月度预算
    #[garde(range(min = 0))]
    pub budget_monthly_cents: Option<i32>,

    /// 月度花费（只读，用于同步）
    #[garde(range(min = 0))]
    pub spent_monthly_cents: Option<i32>,

    /// Agent 状态
    #[garde(skip)]
    pub status: Option<AgentStatus>,

    /// 元数据
    #[garde(skip)]
    pub metadata: Option<Option<JsonValue>>,
}

/// 适配器环境测试请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct TestAdapterEnvironmentSchema {
    /// 适配器配置
    #[garde(skip)]
    #[serde(default = "default_config")]
    pub adapter_config: JsonValue,

    /// 可选的环境 ID（在该环境中运行测试）
    /// 当省略时，测试在本地 Paperclip 主机上运行
    /// 当提供且环境为非本地（SSH/sandbox）时，测试在该环境内执行
    #[garde(skip)]
    pub environment_id: Option<Uuid>,
}

/// Agent 权限输入
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionsInput {
    pub can_create_agents: Option<bool>,
    pub can_create_skills: Option<bool>,
    pub trust_preset: Option<String>,
    pub authorization_policy: Option<String>,
}

/// 指令包输入（创建时）
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct InstructionsBundleInput {
    /// 入口文件路径（相对于 files 中的键）
    #[garde(skip)]
    pub entry_file: Option<String>,

    /// 指令文件映射（路径 -> 内容）
    #[garde(dive)]
    #[garde(custom(validate_files_not_empty))]
    pub files: std::collections::HashMap<String, String>,
}

/// 指令包更新输入
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstructionsBundleInput {
    /// 指令包模式
    #[garde(skip)]
    pub mode: Option<InstructionsBundleMode>,

    /// 根路径
    #[garde(skip)]
    pub root_path: Option<Option<String>>,

    /// 入口文件
    #[garde(skip)]
    pub entry_file: Option<String>,

    /// 是否清除旧的 prompt template
    #[garde(skip)]
    pub clear_legacy_prompt_template: Option<bool>,
}

/// 指令包模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionsBundleMode {
    Managed,
    External,
}

// ========== 默认值函数 ==========

fn default_role() -> AgentRole {
    AgentRole::General
}

fn default_config() -> JsonValue {
    serde_json::json!({})
}

// ========== 自定义验证函数 ==========

fn validate_files_not_empty(
    files: &std::collections::HashMap<String, String>,
    _context: &(),
) -> garde::Result {
    if files.is_empty() {
        return Err(garde::Error::new("instructionsBundle.files must contain at least one file"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;

    #[test]
    fn test_create_agent_hire_schema_valid() {
        let schema = CreateAgentHireSchema {
            name: "Test Agent".to_string(),
            role: AgentRole::General,
            title: Some("Test Title".to_string()),
            icon: None,
            reports_to: None,
            capabilities: None,
            desired_skills: None,
            adapter_type: "claude_local".to_string(),
            adapter_config: serde_json::json!({"model": "claude-opus-4"}),
            instructions_bundle: None,
            runtime_config: serde_json::json!({}),
            default_environment_id: None,
            budget_monthly_cents: 10000,
            permissions: None,
            metadata: None,
            source_issue_id: None,
            source_issue_ids: None,
        };

        assert!(schema.validate(&()).is_ok());
    }

    #[test]
    fn test_create_agent_hire_schema_empty_name() {
        let schema = CreateAgentHireSchema {
            name: "".to_string(),
            role: AgentRole::General,
            title: None,
            icon: None,
            reports_to: None,
            capabilities: None,
            desired_skills: None,
            adapter_type: "claude_local".to_string(),
            adapter_config: serde_json::json!({}),
            instructions_bundle: None,
            runtime_config: serde_json::json!({}),
            default_environment_id: None,
            budget_monthly_cents: 0,
            permissions: None,
            metadata: None,
            source_issue_id: None,
            source_issue_ids: None,
        };

        assert!(schema.validate(&()).is_err());
    }

    #[test]
    fn test_create_agent_hire_schema_negative_budget() {
        let schema = CreateAgentHireSchema {
            name: "Test Agent".to_string(),
            role: AgentRole::General,
            title: None,
            icon: None,
            reports_to: None,
            capabilities: None,
            desired_skills: None,
            adapter_type: "claude_local".to_string(),
            adapter_config: serde_json::json!({}),
            instructions_bundle: None,
            runtime_config: serde_json::json!({}),
            default_environment_id: None,
            budget_monthly_cents: -100,
            permissions: None,
            metadata: None,
            source_issue_id: None,
            source_issue_ids: None,
        };

        assert!(schema.validate(&()).is_err());
    }

    #[test]
    fn test_update_agent_schema_partial() {
        let schema = UpdateAgentSchema {
            name: Some("Updated Name".to_string()),
            role: None,
            title: None,
            icon: None,
            reports_to: None,
            capabilities: None,
            desired_skills: None,
            adapter_type: None,
            adapter_config: None,
            replace_adapter_config: None,
            instructions_bundle: None,
            runtime_config: None,
            default_environment_id: None,
            budget_monthly_cents: None,
            spent_monthly_cents: None,
            status: None,
            metadata: None,
        };

        assert!(schema.validate(&()).is_ok());
    }

    #[test]
    fn test_instructions_bundle_empty_files() {
        let bundle = InstructionsBundleInput {
            entry_file: Some("main.md".to_string()),
            files: std::collections::HashMap::new(),
        };

        assert!(bundle.validate(&()).is_err());
    }

    #[test]
    fn test_instructions_bundle_valid() {
        let mut files = std::collections::HashMap::new();
        files.insert("main.md".to_string(), "# Instructions".to_string());

        let bundle = InstructionsBundleInput {
            entry_file: Some("main.md".to_string()),
            files,
        };

        assert!(bundle.validate(&()).is_ok());
    }

    #[test]
    fn test_test_adapter_environment_schema_default() {
        let schema = TestAdapterEnvironmentSchema {
            adapter_config: serde_json::json!({}),
            environment_id: None,
        };

        assert!(schema.validate(&()).is_ok());
    }
}
