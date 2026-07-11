use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent技能快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillSnapshot {
    pub adapter_type: String,
    pub supported: bool,
    pub mode: AgentSkillSyncMode,
    pub desired_skills: Vec<String>,
    pub entries: Vec<AgentSkillEntry>,
    pub warnings: Vec<String>,
}

/// 技能同步模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSkillSyncMode {
    /// 不同步技能
    None,
    /// 自动同步技能
    Auto,
    /// 手动同步技能
    Manual,
}

impl Default for AgentSkillSyncMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// Agent技能条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillEntry {
    pub name: String,
    pub enabled: bool,
    pub source: SkillSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 技能来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    /// 内置技能
    Builtin,
    /// 公司技能
    Company,
    /// 自定义技能
    Custom,
    /// 外部技能
    External,
}

/// Agent期望技能条目（用于创建/更新）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDesiredSkillEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// 技能同步请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillSyncRequest {
    pub desired_skills: Vec<AgentDesiredSkillEntry>,
}
