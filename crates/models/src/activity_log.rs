use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ActivityLog {
    pub id: Uuid,
    pub company_id: Uuid,
    pub event_type: String,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub metadata: JsonValue,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateActivityLogInput {
    pub company_id: Uuid,
    pub event_type: String,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivityEventType {
    CompanyCreated,
    CompanyUpdated,
    CompanyArchived,
    ProjectCreated,
    ProjectUpdated,
    ProjectArchived,
    ResourceMembershipStarred,
    ResourceMembershipUnstarred,
    SkillCreated,
    SkillUpdated,
    SkillDeleted,
}

impl ActivityEventType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CompanyCreated => "company.created",
            Self::CompanyUpdated => "company.updated",
            Self::CompanyArchived => "company.archived",
            Self::ProjectCreated => "project.created",
            Self::ProjectUpdated => "project.updated",
            Self::ProjectArchived => "project.archived",
            Self::ResourceMembershipStarred => "resource_membership.starred",
            Self::ResourceMembershipUnstarred => "resource_membership.unstarred",
            Self::SkillCreated => "skill.created",
            Self::SkillUpdated => "skill.updated",
            Self::SkillDeleted => "skill.deleted",
        }
    }
}
