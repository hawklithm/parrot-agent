use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// 活动操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityAction {
    // Issue actions
    IssueCreated,
    IssueUpdated,
    IssueAssigned,
    IssueReleased,
    IssueCompleted,
    IssueCommented,

    // Agent actions
    AgentHired,
    AgentConfigured,
    AgentTerminated,
    AgentReassigned,

    // Routine actions
    RoutineCreated,
    RoutineTriggered,
    RoutineCompleted,
    RoutinePaused,

    // Environment actions
    EnvironmentProvisioned,
    EnvironmentLeased,
    EnvironmentReleased,
    EnvironmentDeleted,

    // Approval actions
    ApprovalRequested,
    ApprovalApproved,
    ApprovalRejected,

    // Workspace actions
    WorkspaceCreated,
    WorkspaceDeleted,
    WorkspaceAccessed,

    // Cost actions
    CostIncurred,
    BudgetExceeded,

    // Custom image actions
    CustomImageSetupStarted,
    CustomImageSetupCompleted,
    CustomImageSetupFailed,
}

/// 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Issue,
    Agent,
    Routine,
    Environment,
    Workspace,
    Approval,
    CustomImage,
    Company,
    User,
}

/// 活动执行者类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Agent,
    System,
}

/// 活动元数据（用于分类和过滤）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityMetadata {
    /// 活动分类（如 "agent_management", "issue_tracking"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// 活动严重性（info, warning, error）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,

    /// 是否为审计关键活动
    #[serde(default)]
    pub audit_critical: bool,

    /// 额外的结构化数据
    #[serde(flatten)]
    pub extra: JsonValue,
}

/// 活动记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub id: Uuid,
    pub company_id: Uuid,

    /// 执行者类型
    pub actor_type: ActorType,

    /// 执行者 ID（User ID 或 Agent ID）
    pub actor_id: Uuid,

    /// 活动操作
    pub action: ActivityAction,

    /// 资源类型
    pub resource_type: ResourceType,

    /// 资源 ID
    pub resource_id: Uuid,

    /// 活动元数据
    pub metadata: ActivityMetadata,

    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl Activity {
    /// 创建新的活动记录
    pub fn new(
        company_id: Uuid,
        actor_type: ActorType,
        actor_id: Uuid,
        action: ActivityAction,
        resource_type: ResourceType,
        resource_id: Uuid,
        metadata: ActivityMetadata,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            company_id,
            actor_type,
            actor_id,
            action,
            resource_type,
            resource_id,
            metadata,
            created_at: Utc::now(),
        }
    }

    /// 判断是否为敏感活动（需要特殊处理）
    pub fn is_sensitive(&self) -> bool {
        matches!(
            self.action,
            ActivityAction::AgentTerminated
                | ActivityAction::ApprovalRejected
                | ActivityAction::BudgetExceeded
                | ActivityAction::CustomImageSetupFailed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
 #[test]
    fn test_activity_creation() {
        let metadata = ActivityMetadata {
            category: Some("agent_management".to_string()),
            severity: Some("info".to_string()),
            audit_critical: true,
            extra: json!({"reason": "test"}),
        };

        let activity = Activity::new(
            Uuid::new_v4(),
            ActorType::User,
            Uuid::new_v4(),
            ActivityAction::AgentHired,
            ResourceType::Agent,
            Uuid::new_v4(),
            metadata,
        );

        assert_eq!(activity.action, ActivityAction::AgentHired);
        assert_eq!(activity.resource_type, ResourceType::Agent);
        assert!(!activity.is_sensitive());
    }

    #[test]
    fn test_sensitive_activity_detection() {
        let metadata = ActivityMetadata {
            category: None,
            severity: None,
            audit_critical: false,
            extra: json!({}),
        };

        let activity = Activity::new(
            Uuid::new_v4(),
            ActorType::System,
            Uuid::new_v4(),
            ActivityAction::AgentTerminated,
            ResourceType::Agent,
            Uuid::new_v4(),
            metadata,
        );

        assert!(activity.is_sensitive());
    }
}
