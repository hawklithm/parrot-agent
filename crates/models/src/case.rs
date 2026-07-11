use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// Case Status Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "case_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Draft,
    InProgress,
    InReview,
    Approved,
    Done,
    Cancelled,
}

impl CaseStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, CaseStatus::Done | CaseStatus::Cancelled)
    }
}

// Case Issue Link Role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "case_issue_link_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CaseIssueLinkRole {
    Origin,    // Issue that created the case
    Work,      // Issue working on the case
    Reference, // Referenced issue
}

// Case Event Kind
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "case_event_kind", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CaseEventKind {
    Created,
    Updated,
    StatusChanged,
    DocumentRevised,
    IssueLinked,
    IssueUnlinked,
}

// Case Model
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Case {
    pub id: Uuid,
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub case_number: i32,
    pub identifier: String,
    pub case_type: String,
    pub key: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub status: CaseStatus,
    pub fields: JsonValue, // JSONB - flexible fields per case type
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Case {
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }
}

// Create Case Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaseInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub case_type: String,
    pub key: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<JsonValue>,
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
}

// Update Case Input
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateCaseInput {
    pub project_id: Option<Uuid>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<JsonValue>,
    pub parent_case_id: Option<Uuid>,
}

// Case Issue Link
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseIssueLink {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub issue_id: Uuid,
    pub role: CaseIssueLinkRole,
    pub created_by_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Case Event (Event Sourcing)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseEvent {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub kind: CaseEventKind,
    pub actor_type: Option<String>, // "agent", "user", "system"
    pub actor_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub payload: JsonValue, // JSONB - event-specific data
    pub created_at: DateTime<Utc>,
}

// Case Document Link (many-to-many with documents table)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseDocument {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub document_id: Uuid,
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Case Attachment Link
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseAttachment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub asset_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Case Label Link
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CaseLabel {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub label_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// Case Query Filter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseQueryFilter {
    pub status: Option<Vec<CaseStatus>>,
    pub case_type: Option<Vec<String>>,
    pub project_id: Option<Uuid>,
    pub parent_case_id: Option<Uuid>,
    pub label_id: Option<Uuid>,
}

// Case Detail (for API responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDetail {
    #[serde(flatten)]
    pub case: Case,
    pub parent: Option<CaseParentRef>,
    pub labels: Vec<super::Label>,
    pub issue_links: Vec<CaseIssueLinkDetail>,
    pub documents: Vec<CaseDocumentRef>,
    pub attachments: Vec<CaseAttachmentRef>,
}

// Case Parent Reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseParentRef {
    pub id: Uuid,
    pub identifier: String,
    pub title: String,
    pub case_type: String,
    pub status: CaseStatus,
}

// Case Issue Link Detail (with issue summary)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseIssueLinkDetail {
    pub id: Uuid,
    pub case_id: Uuid,
    pub issue_id: Uuid,
    pub role: CaseIssueLinkRole,
    pub created_at: DateTime<Utc>,
    pub issue: CaseIssueSummary,
}

// Case Issue Summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseIssueSummary {
    pub id: Uuid,
    pub identifier: Option<String>,
    pub title: String,
    pub status: super::issue::IssueStatus,
}

// Case Document Reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDocumentRef {
    pub key: String,
    pub document_id: Uuid,
}

// Case Attachment Reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseAttachmentRef {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub created_at: DateTime<Utc>,
}

// Case Upsert Logic Inputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertCaseInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub case_type: String,
    pub key: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<JsonValue>,
    pub parent_case_id: Option<Uuid>,
    pub actor_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub upsert: bool, // If true, update existing case with same key
}

// Link Issue to Case Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkIssueToCaseInput {
    pub issue_id: Uuid,
    pub role: CaseIssueLinkRole,
}
