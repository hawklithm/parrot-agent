use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

/// Actor types for authorization decisions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthorizationActor {
    Board { user_id: Uuid, company_id: Uuid },
    Agent { agent_id: Uuid, company_id: Uuid },
    None,
}

/// Source of the actor authentication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "actor_source", rename_all = "snake_case")]
pub enum ActorSource {
    LocalImplicit,
    Session,
    BoardKey,
    AgentKey,
    AgentJwt,
    CloudTenant,
    None,
}

/// Authorization actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationAction {
    // Company-level actions
    CompanyRead,
    CompanyUpdate,
    CompanyDelete,
    UsersInvite,
    JoinsApprove,

    // Project-level actions
    ProjectsCreate,
    ProjectsRead,
    ProjectsUpdate,
    ProjectsDelete,

    // Issue-level actions
    IssuesRead,
    IssuesWrite,
    IssuesDelete,
    IssuesAssign,

    // Agent-level actions
    AgentsHire,
    AgentsFire,
    AgentsDelegate,
    TasksAssign,

    // Routine-level actions
    RoutinesCreate,
    RoutinesUpdate,
    RoutinesExecute,

    // Custom action
    Custom(String),
}

/// Reason for authorization decision
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    // Allow reasons
    AllowSelfResource,
    AllowMembershipRole,
    AllowPermissionGrant,
    AllowIssueMentionGrant,
    AllowPublicResource,
    AllowOwnerImplicit,

    // Deny reasons
    DenyNoMembership,
    DenyInsufficientRole,
    DenyNoPermissionGrant,
    DenyLowTrustBoundary,
    DenyResourceNotFound,
    DenyInvalidScope,
    DenyArchivedMembership,
}

/// Authorization decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationDecision {
    pub allowed: bool,
    pub action: AuthorizationAction,
    pub explanation: Option<String>,
    pub code: Option<String>,
    pub reason: DecisionReason,
    pub grant: Option<PermissionGrantInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrantInfo {
    pub grant_id: Uuid,
    pub permission_key: String,
    pub scope: JsonValue,
}

/// Membership role in a company
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "membership_role", rename_all = "snake_case")]
pub enum MembershipRole {
    Owner,
    Admin,
    Operator,
    Viewer,
}

/// Principal type for permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "principal_type", rename_all = "snake_case")]
pub enum PrincipalType {
    User,
    Agent,
}

/// Membership status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "membership_status", rename_all = "snake_case")]
pub enum MembershipStatus {
    Active,
    Archived,
}

/// Auth user model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: String,
    pub email_verified: bool,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Auth session model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
}

/// Board API key model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BoardApiKey {
    pub id: Uuid,
    pub company_id: Uuid,
    pub user_id: Uuid,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// CLI auth challenge model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CliAuthChallenge {
    pub id: Uuid,
    pub challenge_code: String,
    pub user_id: Option<Uuid>,
    pub approved: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Instance user role model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InstanceUserRole {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Permission grant model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PrincipalPermissionGrant {
    pub id: Uuid,
    pub company_id: Uuid,
    pub principal_type: PrincipalType,
    pub principal_id: Uuid,
    pub permission_key: String,
    pub scope: JsonValue,
    pub granted_by_user_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AuthorizationActor {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn company_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { company_id, .. } => Some(*company_id),
            Self::Agent { company_id, .. } => Some(*company_id),
            Self::None => None,
        }
    }

    pub fn user_id(&self) -> Option<Uuid> {
        match self {
            Self::Board { user_id, .. } => Some(*user_id),
            _ => None,
        }
    }

    pub fn agent_id(&self) -> Option<Uuid> {
        match self {
            Self::Agent { agent_id, .. } => Some(*agent_id),
            _ => None,
        }
    }
}

impl AuthorizationDecision {
    pub fn allow(action: AuthorizationAction, reason: DecisionReason) -> Self {
        Self {
            allowed: true,
            action,
            explanation: None,
            code: None,
            reason,
            grant: None,
        }
    }

    pub fn deny(action: AuthorizationAction, reason: DecisionReason, explanation: String) -> Self {
        Self {
            allowed: false,
            action,
            explanation: Some(explanation),
            code: Some("FORBIDDEN".to_string()),
            reason,
            grant: None,
        }
    }

    pub fn with_grant(mut self, grant: PermissionGrantInfo) -> Self {
        self.grant = Some(grant);
        self
    }
}

impl MembershipRole {
    pub fn can_manage_company(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    pub fn can_invite_users(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Operator)
    }

    pub fn can_manage_projects(&self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Operator)
    }
}
