use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// Issue Comment Actor Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "comment_actor_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CommentActorType {
    User,
    Agent,
    System,
}

// Issue Comment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueComment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub body: String,
    pub actor_type: CommentActorType,
    pub actor_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub metadata: Option<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Add Comment Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCommentInput {
    pub body: String,
    pub reopen_requested: Option<bool>,
    pub metadata: Option<JsonValue>,
}

// Thread Interaction Type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "interaction_type", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum InteractionType {
    Question,
    Clarification,
    Approval,
    Feedback,
}

// Thread Interaction
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadInteraction {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub interaction_type: InteractionType,
    pub actor_type: CommentActorType,
    pub actor_id: Option<Uuid>,
    pub body: String,
    pub metadata: Option<JsonValue>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by_type: Option<CommentActorType>,
    pub resolved_by_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// Create Thread Interaction Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInteractionInput {
    pub interaction_type: InteractionType,
    pub body: String,
    pub metadata: Option<JsonValue>,
}

// Resolve Thread Interaction Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveInteractionInput {
    pub resolution_note: Option<String>,
}
