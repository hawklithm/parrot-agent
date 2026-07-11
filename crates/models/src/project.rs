use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "project_status", rename_all = "snake_case")]
pub enum ProjectStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "execution_workspace_policy", rename_all = "snake_case")]
pub enum ExecutionWorkspacePolicy {
    Shared,
    IsolatedPerIssue,
    IsolatedPerAgent,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub company_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub status: ProjectStatus,
    pub lead_agent_id: Option<Uuid>,
    pub target_date: Option<DateTime<Utc>>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub env: Option<JsonValue>,
    pub pause_reason: Option<String>,
    pub paused_at: Option<DateTime<Utc>>,
    pub execution_workspace_policy: ExecutionWorkspacePolicy,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectInput {
    pub company_id: Uuid,
    pub goal_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub lead_agent_id: Option<Uuid>,
    pub target_date: Option<DateTime<Utc>>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub env: Option<JsonValue>,
    pub execution_workspace_policy: Option<ExecutionWorkspacePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProjectInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<ProjectStatus>,
    pub lead_agent_id: Option<Uuid>,
    pub target_date: Option<DateTime<Utc>>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub env: Option<JsonValue>,
    pub pause_reason: Option<String>,
    pub execution_workspace_policy: Option<ExecutionWorkspacePolicy>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectWorkspace {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub config: JsonValue,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceInput {
    pub project_id: Uuid,
    pub name: String,
    pub config: JsonValue,
    pub is_primary: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "membership_state", rename_all = "snake_case")]
pub enum MembershipState {
    Joined,
    Left,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ProjectMembership {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Uuid,
    pub user_id: Uuid,
    pub state: MembershipState,
    pub starred_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AgentMembership {
    pub id: Uuid,
    pub company_id: Uuid,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    pub state: MembershipState,
    pub starred_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMemberships {
    pub project_memberships: Vec<ProjectMembershipWithProject>,
    pub agent_memberships: Vec<AgentMembershipWithAgent>,
    pub starred_project_ids: Vec<Uuid>,
    pub starred_agent_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMembershipWithProject {
    pub membership: ProjectMembership,
    pub project: Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMembershipWithAgent {
    pub membership: AgentMembership,
    pub agent_name: String,
    pub agent_status: String,
}
