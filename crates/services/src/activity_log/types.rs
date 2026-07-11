use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub company_id: Uuid,
    pub actor_type: String,
    pub actor_id: Uuid,
    pub action: ActivityAction,
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub metadata: ActivityMetadata,
    pub level: ActivityLevel,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "activity_action", rename_all = "snake_case")]
pub enum ActivityAction {
    Created,
    Updated,
    Deleted,
    CheckedOut,
    Released,
    Approved,
    Rejected,
    Executed,
    Failed,
    Hired,
    Terminated,
    Triggered,
    Completed,
    Acquired,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "resource_type", rename_all = "snake_case")]
pub enum ResourceType {
    Agent,
    Issue,
    Case,
    Routine,
    Goal,
    Approval,
    Environment,
    Workspace,
    Lease,
    Budget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityMetadata {
    pub changes: Option<serde_json::Value>,
    pub related_resources: Vec<RelatedResource>,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedResource {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub relationship: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "activity_level", rename_all = "snake_case")]
pub enum ActivityLevel {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityQuery {
    pub company_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub resource_type: Option<ResourceType>,
    pub resource_id: Option<Uuid>,
    pub actions: Option<Vec<ActivityAction>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityFeed {
    pub activities: Vec<Activity>,
    pub total_count: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationPeriod {
    Hourly,
    Daily,
    Weekly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStats {
    pub period: AggregationPeriod,
    pub resource_type: ResourceType,
    pub action: ActivityAction,
    pub count: i64,
    pub timestamp: DateTime<Utc>,
}
