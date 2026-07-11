use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Case status enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    Draft,
    InProgress,
    InReview,
    Approved,
    Done,
    Cancelled,
}

/// Case issue link role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaseIssueLinkRole {
    Origin,
    Work,
    Reference,
}

/// Case core structure
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Update case input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCaseInput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub status: Option<CaseStatus>,
    pub fields: Option<serde_json::Value>,
}

/// Case-Issue link (many-to-many)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
