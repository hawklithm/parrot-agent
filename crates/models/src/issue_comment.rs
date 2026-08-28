use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Issue comment author type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "comment_actor_type", rename_all = "lowercase")]
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
    pub metadata: Option<sqlx::types::Json<serde_json::Value>>,
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
    Review,
    SuggestTasks,
    AskUserQuestions,
    RequestConfirmation,
    RequestCheckboxConfirmation,
    ItemVerdict,
    Withdraw,
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
    Expired,
}

/// Thread interaction
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct IssueThreadInteraction {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub kind: String, // "question" | "approval" | "suggest_tasks" | "ask_user_questions" | "request_confirmation" | "request_checkbox_confirmation"
    pub status: String, // "pending" | "accepted" | "rejected" | "cancelled" | "resolved" | "expired"
    pub continuation_policy: String, // "wake_assignee" | "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_comment_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by_user_id: Option<String>,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a thread interaction
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadInteractionInput {
    pub kind: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default = "default_continuation_policy")]
    pub continuation_policy: String,
    #[serde(default)]
    pub source_run_id: Option<Uuid>,
    #[serde(default)]
    pub source_comment_id: Option<Uuid>,
}

fn default_continuation_policy() -> String {
    "wake_assignee".to_string()
}

/// Input for accepting a thread interaction
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptThreadInteractionInput {
    #[serde(default)]
    pub response: Option<serde_json::Value>,
}

/// Input for rejecting a thread interaction
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectThreadInteractionInput {
    #[serde(default)]
    pub response: Option<serde_json::Value>,
}

/// Result of accepting an interaction
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptInteractionResult {
    pub interaction: IssueThreadInteraction,
    pub created_issues: Vec<crate::Issue>,
    pub continuation_issue: Option<crate::Issue>,
}

/// Input for answering ask_user_questions interaction
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerQuestionsInput {
    pub answers: Vec<QuestionAnswer>,
    #[serde(default)]
    pub summary_markdown: Option<String>,
}

/// A single question answer
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswer {
    pub question_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_text: Option<String>,
}

/// Input for cancelling ask_user_questions interaction
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelQuestionsInput {
    #[serde(default)]
    pub reason: Option<String>,
}
