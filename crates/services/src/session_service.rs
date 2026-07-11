use async_trait::async_trait;
use uuid::Uuid;

/// SessionManagementService - Agent会话管理服务接口
#[async_trait]
pub trait SessionManagementService: Send + Sync {
    /// 注册Agent会话
    async fn register_session(&self, agent_id: Uuid, session_data: SessionData) -> Result<Uuid, SessionError>;

    /// 清理Agent会话（Agent终止或重置时调用）
    async fn cleanup_session(&self, agent_id: Uuid) -> Result<(), SessionError>;

    /// 获取会话状态
    async fn get_session_state(&self, agent_id: Uuid) -> Result<Option<SessionState>, SessionError>;

    /// 重置会话运行时状态
    async fn reset_session(&self, agent_id: Uuid) -> Result<(), SessionError>;
}

/// SessionData - 会话初始化数据
#[derive(Debug, Clone)]
pub struct SessionData {
    pub agent_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub environment_id: Option<Uuid>,
    pub metadata: serde_json::Value,
}

/// SessionState - 会话运行时状态
#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: Uuid,
    pub agent_id: Uuid,
    pub status: SessionStatus,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
    pub active_tasks: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Idle,
    Suspended,
    Terminated,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(Uuid),

    #[error("Session already exists: {0}")]
    AlreadyExists(Uuid),

    #[error("Invalid session state: {0}")]
    InvalidState(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),
}

/// SkillService - Agent技能管理服务接口
#[async_trait]
pub trait SkillService: Send + Sync {
    /// 列出可用技能
    async fn list_skills(&self, company_id: Uuid) -> Result<Vec<SkillInfo>, SkillError>;

    /// 绑定技能到Agent
    async fn bind_to_agent(&self, agent_id: Uuid, skill_id: Uuid) -> Result<(), SkillError>;

    /// 解绑技能
    async fn unbind_from_agent(&self, agent_id: Uuid, skill_id: Uuid) -> Result<(), SkillError>;
    /// 物化技能（生成技能执行代码/配置）
    async fn materialize_skill(&self, skill_id: Uuid) -> Result<MaterializedSkill, SkillError>;

    /// 同步Agent技能列表
    async fn sync_agent_skills(&self, agent_id: Uuid) -> Result<Vec<SkillInfo>, SkillError>;
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub skill_type: SkillType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillType {
    BuiltIn,
    Custom,
    External,
}

#[derive(Debug, Clone)]
pub struct MaterializedSkill {
    pub skill_id: Uuid,
    pub code: String,
    pub config: serde_json::Value,
    pub dependencies: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill not found: {0}")]
    NotFound(Uuid),

    #[error("Skill already bound to agent")]
    AlreadyBound,

    #[error("Materialization failed: {0}")]
    MaterializationFailed(String),

    #[error("Repository error: {0}")]
    RepositoryError(String),
}
