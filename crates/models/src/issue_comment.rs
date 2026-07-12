use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Issue comment author type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum IssueCommentAuthorType {
    Agent,
    User,
    System,
}

/// Issue comment presentation kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueCommentPresentationKind {
    Standard,
    SystemNotice,
    WarningBanner,
    ErrorAlert,
}

/// Issue comment presentation tone
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueCommentPresentationTone {
    Neutral,
    Positive,
    Warning,
    Critical,
}

/// Issue comment presentation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCommentPresentation {
    pub kind: IssueCommentPresentationKind,
    pub tone: IssueCommentPresentationTone,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub details_default_open: bool,
}

/// Issue comment metadata section row
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IssueCommentMetadataRow {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "code")]
    Code {
        code: String,
        language: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "key_value")]
    KeyValue {
        key: String,
        value: String,
    },
}

/// Issue comment metadata section
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCommentMetadataSection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub rows: Vec<IssueCommentMetadataRow>,
}

/// Issue comment metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueCommentMetadata {
    pub version: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<Uuid>,
    pub sections: Vec<IssueCommentMetadataSection>,
}

/// Issue comment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueComment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub author_type: IssueCommentAuthorType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_run_id: Option<Uuid>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<sqlx::types::Json<IssueCommentPresentation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<sqlx::types::Json<IssueCommentMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by_user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_by_run_id: Option<Uuid>,
    pub follow_up_requested: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Thread interaction kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadInteractionKind {
    Question,
    Approval,
    Decision,
    Blocker,
}

/// Thread interaction status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThreadInteractionStatus {
    Pending,
    Accepted,
    Rejected,
    Cancelled,
    Resolved,
}

/// Thread interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueThreadInteraction {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub kind: ThreadInteractionKind,
    pub status: ThreadInteractionStatus,
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
}
