use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 内置Agent标识枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInAgentKey {
    /// 反思教练（Reflection Coach）
    ReflectionCoach,
    /// 学习助手（Learning Assistant）
    LearningAssistant,
    /// 简报生成器（Briefs Generator）
    BriefsGenerator,
}

impl BuiltInAgentKey {
    /// 获取所有内置Agent键
    pub fn all() -> Vec<Self> {
        vec![
            Self::ReflectionCoach,
            Self::LearningAssistant,
            Self::BriefsGenerator,
        ]
    }

    /// 转换为字符串键
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReflectionCoach => "reflection_coach",
            Self::LearningAssistant => "learning_assistant",
            Self::BriefsGenerator => "briefs_generator",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "reflection_coach" => Some(Self::ReflectionCoach),
            "learning_assistant" => Some(Self::LearningAssistant),
            "briefs_generator" => Some(Self::BriefsGenerator),
            _ => None,
        }
    }
}

impl std::fmt::Display for BuiltInAgentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 内置Agent状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInAgentStatus {
    /// 未初始化（未创建Agent记录）
    NotProvisioned,
    /// 待审批（公司需要董事会审批）
    PendingApproval,
    /// 需要配置（Agent已创建，但缺少adapter配置）
    NeedsSetup,
    /// 就绪（可正常使用）
    Ready,
    /// 已暂停（pausedAt字段不为空）
    Paused,
}

impl BuiltInAgentStatus {
    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "not_provisioned" => Some(Self::NotProvisioned),
            "pending_approval" => Some(Self::PendingApproval),
            "needs_setup" => Some(Self::NeedsSetup),
            "ready" => Some(Self::Ready),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }
}

/// 内置Agent定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInAgentDefinition {
    /// 唯一标识键
    pub key: BuiltInAgentKey,
    /// 显示名称
    pub display_name: String,
    /// 功能标识列表（用于特性门控）
    pub feature_keys: Vec<String>,
    /// 简短用途说明
    pub short_purpose: String,
    /// 默认指令内容
    pub default_instructions: String,
    /// 默认角色
    pub default_role: models::AgentRole,
    /// 默认标题（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_title: Option<String>,
    /// 默认图标（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_icon: Option<String>,
    /// 默认权限（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_permissions: Option<models::AgentPermissions>,
    /// 默认状态（idle/paused）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_status: Option<models::AgentStatus>,
    /// 默认上级（single_root_agent表示找到公司唯一的根Agent）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_manager: Option<String>,
    /// 允许的适配器类型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_adapter_types: Option<Vec<String>>,
    /// 默认月度预算（美分）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_budget_monthly_cents: Option<i32>,
    /// 资源包定义（指令+技能+例程）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<BuiltInAgentBundleDefinition>,
}

/// 内置Agent资源包定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInAgentBundleDefinition {
    /// 库存版本（如 "v1.0.0"）
    pub stock_version: String,
    /// 指令文件定义
    pub instructions: BundleInstructionsDefinition,
    /// 技能定义
    pub skill: BundleSkillDefinition,
    /// 例程定义
    pub routine: BundleRoutineDefinition,
}

/// 指令文件定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInstructionsDefinition {
    /// 入口文件路径
    pub entry_file: String,
    /// 文件映射（路径 -> 内容）
    pub files: HashMap<String, String>,
}

/// 技能定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSkillDefinition {
    /// 技能键
    pub skill_key: String,
    /// 显示名称
    pub display_name: String,
    /// URL slug
    pub slug: String,
    /// 规范键（用于去重）
    pub canonical_key: String,
    /// 文件映射（路径 -> 内容）
    pub files: HashMap<String, String>,
}

/// 例程定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRoutineDefinition {
    /// 例程键
    pub routine_key: String,
    /// 标题
    pub title: String,
    /// 描述
    pub description: String,
    /// 状态
    pub status: RoutineStatus,
    /// 优先级
    pub priority: RoutinePriority,
    /// 并发策略
    pub concurrency_policy: RoutineConcurrencyPolicy,
    /// 补偿策略
    pub catch_up_policy: RoutineCatchUpPolicy,
    /// 变量列表
    pub variables: Vec<RoutineVariable>,
    /// 触发器列表
    pub triggers: Vec<RoutineTrigger>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineStatus {
    Active,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutinePriority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineConcurrencyPolicy {
    AlwaysEnqueue,
    CoalesceIfActive,
    SkipIfActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineCatchUpPolicy {
    EnqueueMissedWithCap,
    SkipMissed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineVariable {
    pub key: String,
    pub label: String,
    pub default_value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineTrigger {
    pub kind: String, // "schedule"
    pub label: Option<String>,
    pub enabled: bool,
    pub cron_expression: String,
    pub timezone: String,
}

/// 内置Agent元数据注册表
pub struct BuiltInAgentMetadataRegistry {
    definitions: HashMap<BuiltInAgentKey, BuiltInAgentDefinition>,
}

impl BuiltInAgentMetadataRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        let mut registry = Self {
            definitions: HashMap::new(),
        };

        // 注册所有内置Agent定义
        registry.register_default_definitions();

        registry
    }

    /// 注册默认的内置Agent定义
    fn register_default_definitions(&mut self) {
        // Reflection Coach
        self.definitions.insert(
            BuiltInAgentKey::ReflectionCoach,
            BuiltInAgentDefinition {
                key: BuiltInAgentKey::ReflectionCoach,
                display_name: "Reflection Coach".to_string(),
                feature_keys: vec!["built_in_agents".to_string()],
                short_purpose: "Helps team members reflect on their work and growth".to_string(),
                default_instructions: "You are Paperclip's built-in Reflection Coach.".to_string(),
                default_role: models::AgentRole::General,
                default_title: Some("Reflection Coach".to_string()),
                default_icon: Some("🪞".to_string()),
                default_permissions: None,
                default_status: Some(models::AgentStatus::Idle),
                default_manager: Some("single_root_agent".to_string()),
                allowed_adapter_types: Some(vec!["claude_local".to_string(), "process".to_string()]),
                default_budget_monthly_cents: Some(50000), // $500
                bundle: None, // TODO: 实现bundle定义
            },
        );

        // Learning Assistant
        self.definitions.insert(
            BuiltInAgentKey::LearningAssistant,
            BuiltInAgentDefinition {
                key: BuiltInAgentKey::LearningAssistant,
                display_name: "Learning Assistant".to_string(),
                feature_keys: vec!["built_in_agents".to_string()],
                short_purpose: "Helps onboard new team members and answer questions".to_string(),
                default_instructions: "You are Paperclip's built-in Learning Assistant.".to_string(),
                default_role: models::AgentRole::General,
                default_title: Some("Learning Assistant".to_string()),
                default_icon: Some("📚".to_string()),
                default_permissions: None,
                default_status: Some(models::AgentStatus::Idle),
                default_manager: Some("single_root_agent".to_string()),
                allowed_adapter_types: Some(vec!["claude_local".to_string(), "process".to_string()]),
                default_budget_monthly_cents: Some(30000), // $300
                bundle: None,
            },
        );

        // Briefs Generator
        self.definitions.insert(
            BuiltInAgentKey::BriefsGenerator,
            BuiltInAgentDefinition {
                key: BuiltInAgentKey::BriefsGenerator,
                display_name: "Briefs Generator".to_string(),
                feature_keys: vec!["built_in_agents".to_string()],
                short_purpose: "Generates periodic team briefs and summaries".to_string(),
                default_instructions: "You are Paperclip's built-in Briefs Generator.".to_string(),
                default_role: models::AgentRole::General,
                default_title: Some("Briefs Generator".to_string()),
                default_icon: Some("📄".to_string()),
                default_permissions: None,
                default_status: Some(models::AgentStatus::Idle),
                default_manager: Some("single_root_agent".to_string()),
                allowed_adapter_types: Some(vec!["claude_local".to_string(), "process".to_string()]),
                default_budget_monthly_cents: Some(20000), // $200
                bundle: None,
            },
        );
    }

    /// 获取内置Agent定义
    pub fn get_definition(&self, key: BuiltInAgentKey) -> Option<&BuiltInAgentDefinition> {
        self.definitions.get(&key)
    }

    /// 列举所有内置Agent定义
    pub fn list_definitions(&self) -> Vec<&BuiltInAgentDefinition> {
        self.definitions.values().collect()
    }

    /// 检查是否存在指定的内置Agent
    pub fn contains(&self, key: BuiltInAgentKey) -> bool {
        self.definitions.contains_key(&key)
    }
}

impl Default for BuiltInAgentMetadataRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 推导内置Agent的状态
pub fn derive_built_in_agent_status(
    agent: Option<&models::Agent>,
    _approval: Option<&models::Approval>,
) -> BuiltInAgentStatus {
    // 如果Agent不存在，返回未初始化
    if agent.is_none() {
        return BuiltInAgentStatus::NotProvisioned;
    }

    let agent = agent.unwrap();

    // 检查是否等待审批
    if agent.status == models::AgentStatus::PendingApproval {
        return BuiltInAgentStatus::PendingApproval;
    }

    // 检查是否暂停
    if agent.status == models::AgentStatus::Paused {
        return BuiltInAgentStatus::Paused;
    }

    // 检查adapter配置是否完整
    if has_complete_adapter_config(&agent.adapter_type, &agent.adapter_config.0) {
        return BuiltInAgentStatus::Ready;
    }

    // 配置不完整，需要设置
    BuiltInAgentStatus::NeedsSetup
}

/// 检查adapter配置是否完整
fn has_complete_adapter_config(adapter_type: &str, adapter_config: &serde_json::Value) -> bool {
    match adapter_type {
        "claude_local" | "process" => {
            // 对于claude_local和process适配器，检查是否有API key或模型配置
            if let Some(obj) = adapter_config.as_object() {
                // 简化检查：只要有env配置就认为完整
                obj.contains_key("env") || obj.contains_key("model")
            } else {
                false
            }
        }
        _ => {
            // 其他适配器类型默认认为配置完整
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_built_in_agent_key_parsing() {
        assert_eq!(
            BuiltInAgentKey::from_str("reflection_coach"),
            Some(BuiltInAgentKey::ReflectionCoach)
        );
        assert_eq!(
            BuiltInAgentKey::from_str("learning_assistant"),
            Some(BuiltInAgentKey::LearningAssistant)
        );
        assert_eq!(BuiltInAgentKey::from_str("invalid"), None);
    }

    #[test]
    fn test_built_in_agent_key_display() {
        assert_eq!(BuiltInAgentKey::ReflectionCoach.to_string(), "reflection_coach");
    }

    #[test]
    fn test_registry_initialization() {
        let registry = BuiltInAgentMetadataRegistry::new();
        assert_eq!(registry.list_definitions().len(), 3);
        assert!(registry.contains(BuiltInAgentKey::ReflectionCoach));
        assert!(registry.contains(BuiltInAgentKey::LearningAssistant));
        assert!(registry.contains(BuiltInAgentKey::BriefsGenerator));
    }

    #[test]
    fn test_get_definition() {
        let registry = BuiltInAgentMetadataRegistry::new();
        let def = registry.get_definition(BuiltInAgentKey::ReflectionCoach).unwrap();
        assert_eq!(def.display_name, "Reflection Coach");
        assert_eq!(def.default_budget_monthly_cents, Some(50000));
    }

    #[test]
    fn test_derive_status_not_provisioned() {
        let status = derive_built_in_agent_status(None, None);
        assert_eq!(status, BuiltInAgentStatus::NotProvisioned);
    }

    #[test]
    fn test_has_complete_adapter_config() {
        let config_with_env = serde_json::json!({
            "env": {
                "OPENAI_API_KEY": "sk-test"
            }
        });
        assert!(has_complete_adapter_config("claude_local", &config_with_env));

        let empty_config = serde_json::json!({});
        assert!(!has_complete_adapter_config("claude_local", &empty_config));
    }
}
