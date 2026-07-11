use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Case status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum CaseStatus {
    Draft,
    InProgress,
    InReview,
    Approved,
    Done,
    Cancelled,
}

/// Case issue link role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum CaseIssueLinkRole {
    Origin,
    Work,
    Reference,
}

/// Case core structure
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
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
    pub fields: serde_json::Value, // JSONB custom fields
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create case input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCaseInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub case_type: String,
    pub key: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<serde_json::Value>,
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
}

/// Update case input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCaseInput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<serde_json::Value>,
    pub project_id: Option<Uuid>,
    pub parent_case_id: Option<Uuid>,
}

/// Case-Issue link (many-to-many)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CaseIssueLink {
    pub id: Uuid,
    pub company_id: Uuid,
    pub case_id: Uuid,
    pub issue_id: Uuid,
    pub role: CaseIssueLinkRole,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Case document
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDocument {
    pub id: Uuid,
    pub case_id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub content: String,
    pub locked_by_agent_id: Option<Uuid>,
    pub locked_by_user_id: Option<Uuid>,
    pub locked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl CaseDocument {
    /// Check if document is currently locked
    pub fn is_locked(&self) -> bool {
        self.locked_at.is_some() && (self.locked_by_agent_id.is_some() || self.locked_by_user_id.is_some())
    }
    
    /// Check if locked by specific actor
    pub fn is_locked_by(&self, agent_id: Option<Uuid>, user_id: Option<Uuid>) -> bool {
        if !self.is_locked() {
            return false;
        }
        match (agent_id, user_id) {
            (Some(aid), _) => self.locked_by_agent_id == Some(aid),
            (_, Some(uid)) => self.locked_by_user_id == Some(uid),
            _ => false,
        }
    }
}

/// Case event kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum CaseEventKind {
    Created,
    Updated,
    StatusChanged,
    DocumentRevised,
    IssueLinked,
    IssueUnlinked,
}

/// Case event
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CaseEvent {
    pub id: Uuid,
    pub case_id: Uuid,
    pub company_id: Uuid,
    pub kind: CaseEventKind,
    pub metadata: Option<serde_json::Value>,
    pub actor_agent_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub actor_type: Option<String>,
    pub actor_id: Option<Uuid>,
    pub actor_run_id: Option<Uuid>,
    pub payload: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Case detail with all related data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseDetail {
    #[serde(flatten)]
    pub case: Case,
    pub labels: Vec<String>,
    pub issue_links: Vec<CaseIssueLink>,
    pub documents: Vec<CaseDocument>,
    pub attachments: Vec<Uuid>, // Attachment IDs
    pub parent_case: Option<Box<Case>>,
}

/// Case query filter
#[derive(Debug, Clone, Default)]
pub struct CaseQueryFilter {
    pub status: Option<CaseStatus>,
    pub case_type: Option<String>,
    pub project_id: Option<Uuid>,
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
}

/// Upsert case input (create or update)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCaseInput {
    pub company_id: Uuid,
    pub project_id: Option<Uuid>,
    pub case_type: String,
    pub key: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<serde_json::Value>,
    pub parent_case_id: Option<Uuid>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
}

impl CaseStatus {
    /// Check if the status is terminal (no more work)
    pub fn is_terminal(&self) -> bool {
        matches!(self, CaseStatus::Done | CaseStatus::Cancelled)
    }
}
