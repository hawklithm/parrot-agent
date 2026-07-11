use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

// Document (shared between Issue and Case)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Document {
    pub id: Uuid,
    pub company_id: Uuid,
    pub content: String,
    pub content_type: Option<String>,
    pub locked_by_type: Option<String>, // "agent", "user"
    pub locked_by_id: Option<Uuid>,
    pub locked_at: Option<DateTime<Utc>>,
    pub locked_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn is_locked(&self) -> bool {
        self.locked_by_id.is_some()
    }
}

// Issue Document Link
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueDocument {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub document_id: Uuid,
    pub key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Create/Update Document Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertDocumentInput {
    pub key: String,
    pub content: String,
    pub content_type: Option<String>,
}

// Lock Document Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockDocumentInput {
    pub locked_by_type: String, // "agent", "user", "system"
    pub locked_by_id: Uuid,
    pub run_id: Option<Uuid>,
}

// Document Revision (for Case documents)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentRevision {
    pub id: Uuid,
    pub document_id: Uuid,
    pub revision_number: i32,
    pub content: String,
    pub created_by_type: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// Annotation Thread Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "annotation_thread_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AnnotationThreadStatus {
    Open,
    Resolved,
}

// Annotation Thread
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnnotationThread {
    pub id: Uuid,
    pub company_id: Uuid,
    pub document_id: Uuid,
    pub position: JsonValue, // JSONB - position data (line, column, range, etc.)
    pub status: AnnotationThreadStatus,
    pub created_by_type: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub resolved_by_type: Option<String>,
    pub resolved_by_id: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Annotation Comment
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnnotationComment {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub body: String,
    pub actor_type: String, // "agent", "user"
    pub actor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// Create Annotation Thread Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAnnotationThreadInput {
    pub position: JsonValue,
    pub initial_comment: String,
}

// Update Annotation Thread Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAnnotationThreadInput {
    pub status: Option<AnnotationThreadStatus>,
}

// Add Annotation Comment Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAnnotationCommentInput {
    pub body: String,
}

// Work Product
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueWorkProduct {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub artifact: JsonValue, // JSONB - flexible artifact data
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Create Work Product Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWorkProductInput {
    pub name: String,
    pub description: Option<String>,
    pub artifact: JsonValue,
}

// Update Work Product Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkProductInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub artifact: Option<JsonValue>,
}

// Attachment (via Asset)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Attachment {
    pub id: Uuid,
    pub company_id: Uuid,
    pub parent_type: String, // "issue", "case"
    pub parent_id: Uuid,
    pub asset_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_by_type: Option<String>,
    pub created_by_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Label
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Label {
    pub id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Create Label Input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLabelInput {
    pub name: String,
    pub color: Option<String>,
}

// Issue Label Link
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IssueLabel {
    pub id: Uuid,
    pub company_id: Uuid,
    pub issue_id: Uuid,
    pub label_id: Uuid,
    pub created_at: DateTime<Utc>,
}
