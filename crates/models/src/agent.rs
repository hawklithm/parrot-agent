use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

/// Agent 状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Running,
    Paused,
    PendingApproval,
    Terminated,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Idle
    }
}

/// Agent 角色枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Ceo,
    Vp,
    Manager,
    Researcher,
    General,
}

impl Default for AgentRole {
    fn default() -> Self {
        Self::General
    }
}

/// Trust Preset 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustPreset {
    Restricted,
    Standard,
    Elevated,
}

/// Trust Authorization Policy 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustAuthorizationPolicy {
    Manual,
    AutoApprove,
}

/// Agent 权限结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissions {
    pub can_create_agents: bool,
    pub can_create_skills: bool,
    pub trust_preset: TrustPreset,
    pub authorization_policy: TrustAuthorizationPolicy,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        Self {
            can_create_agents: false,
            can_create_skills: false,
            trust_preset: TrustPreset::Standard,
            authorization_policy: TrustAuthorizationPolicy::Manual,
        }
    }
}

/// Agent 元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_built_in: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_in_key: Option<String>,
    /// 指令包在适配器运行时中的路径（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions_path: Option<String>,
    /// 指令包内容（文件树），存储于 agent 元数据内
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions_bundle: Option<serde_json::Value>,
}

/// Skill source enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    Company,
    Global,
    Agent,
}

/// Agent skill sync mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSkillSyncMode {
    Auto,
    Manual,
}

/// Agent skill entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillEntry {
    pub skill_id: String,
    pub source: SkillSource,
    pub enabled: bool,
}

/// Agent skill snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillSnapshot {
    pub skills: Vec<AgentSkillEntry>,
    pub sync_mode: AgentSkillSyncMode,
    pub last_synced_at: Option<DateTime<Utc>>,
}

/// Agent 实体
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub role: AgentRole,
    pub status: AgentStatus,
    pub adapter_type: String,
    pub adapter_config: Json<serde_json::Value>,
    pub runtime_config: Json<serde_json::Value>,
    pub permissions: Json<AgentPermissions>,
    pub metadata: Json<AgentMetadata>,
    pub budget_monthly_cents: i32,
    pub reports_to: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Agent 配置版本
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigRevision {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub snapshot: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Agent 运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeState {
    pub agent_id: Uuid,
    pub status: AgentStatus,
    pub is_healthy: bool,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub current_task_id: Option<Uuid>,
}

/// Agent 任务会话
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskSession {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

/// 审批记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub status: String,
    pub requested_by: Uuid,
    pub requested_by_user_id: Option<Uuid>,
    pub requested_by_agent_id: Option<Uuid>,
    pub approved_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// 状态转换定义
#[derive(Debug, Clone, Copy)]
pub struct StateTransition {
    pub from: AgentStatus,
    pub to: AgentStatus,
    pub trigger: TransitionTrigger,
}

/// 状态转换触发器
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTrigger {
    TaskAssigned,
    HeartbeatTimeout,
    BudgetExhausted,
    ManualPause,
    ManualResume,
    ApprovalGranted,
    ManualTerminate,
}

/// Agent 状态机
#[derive(Debug)]
pub struct AgentStateMachine {
    transitions: Vec<StateTransition>,
}

impl AgentStateMachine {
    /// 创建默认状态机
    pub fn new() -> Self {
        let transitions = vec![
            StateTransition {
                from: AgentStatus::Idle,
                to: AgentStatus::Running,
                trigger: TransitionTrigger::TaskAssigned,
            },
            StateTransition {
                from: AgentStatus::Running,
                to: AgentStatus::Paused,
                trigger: TransitionTrigger::HeartbeatTimeout,
            },
            StateTransition {
                from: AgentStatus::Running,
                to: AgentStatus::Paused,
                trigger: TransitionTrigger::BudgetExhausted,
            },
            StateTransition {
                from: AgentStatus::Running,
                to: AgentStatus::Paused,
                trigger: TransitionTrigger::ManualPause,
            },
            StateTransition {
                from: AgentStatus::Paused,
                to: AgentStatus::Running,
                trigger: TransitionTrigger::ManualResume,
            },
            StateTransition {
                from: AgentStatus::PendingApproval,
                to: AgentStatus::Idle,
                trigger: TransitionTrigger::ApprovalGranted,
            },
            StateTransition {
                from: AgentStatus::Idle,
                to: AgentStatus::Terminated,
                trigger: TransitionTrigger::ManualTerminate,
            },
            StateTransition {
                from: AgentStatus::Running,
                to: AgentStatus::Terminated,
                trigger: TransitionTrigger::ManualTerminate,
            },
            StateTransition {
                from: AgentStatus::Paused,
                to: AgentStatus::Terminated,
                trigger: TransitionTrigger::ManualTerminate,
            },
        ];

        Self { transitions }
    }

    /// 验证状态转换是否合法
    pub fn validate_transition(&self, from: AgentStatus, to: AgentStatus) -> bool {
        self.transitions
            .iter()
            .any(|t| t.from == from && t.to == to)
    }

    /// 获取允许的下一个状态列表
    pub fn allowed_next_states(&self, current: AgentStatus) -> Vec<AgentStatus> {
        self.transitions
            .iter()
            .filter(|t| t.from == current)
            .map(|t| t.to)
            .collect()
    }

    /// 根据触发器获取目标状态
    pub fn transition_by_trigger(
        &self,
        from: AgentStatus,
        trigger: TransitionTrigger,
    ) -> Option<AgentStatus> {
        self.transitions
            .iter()
            .find(|t| t.from == from && t.trigger == trigger)
            .map(|t| t.to)
    }
}

impl Default for AgentStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_valid_transitions() {
        let sm = AgentStateMachine::new();

        assert!(sm.validate_transition(AgentStatus::Idle, AgentStatus::Running));
        assert!(sm.validate_transition(AgentStatus::Running, AgentStatus::Paused));
        assert!(sm.validate_transition(
            AgentStatus::PendingApproval,
            AgentStatus::Idle
        ));
    }

    #[test]
    fn test_state_machine_invalid_transitions() {
        let sm = AgentStateMachine::new();

        assert!(!sm.validate_transition(AgentStatus::Terminated, AgentStatus::Running));
        assert!(!sm.validate_transition(AgentStatus::Idle, AgentStatus::Paused));
    }

    #[test]
    fn test_allowed_next_states() {
        let sm = AgentStateMachine::new();

        let next = sm.allowed_next_states(AgentStatus::Running);
        assert!(next.contains(&AgentStatus::Paused));
        assert!(next.contains(&AgentStatus::Terminated));
    }

    #[test]
    fn test_transition_by_trigger() {
        let sm = AgentStateMachine::new();

        assert_eq!(
            sm.transition_by_trigger(AgentStatus::Idle, TransitionTrigger::TaskAssigned),
            Some(AgentStatus::Running)
        );

        assert_eq!(
            sm.transition_by_trigger(AgentStatus::Running, TransitionTrigger::ManualPause),
            Some(AgentStatus::Paused)
        );
    }
}
