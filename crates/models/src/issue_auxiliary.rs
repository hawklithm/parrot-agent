use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Work product
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkProduct {
    pub id: Uuid,
    pub issue_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Create work product input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkProductInput {
    pub name: String,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
}

/// Update work product input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkProductInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub artifact: Option<serde_json::Value>,
}

/// Attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: Uuid,
    pub parent_type: String, // "issue" | "case"
    pub parent_id: Uuid,
    pub company_id: Uuid,
    pub asset_id: Option<Uuid>,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Upload attachment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadAttachmentInput {
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub content: Vec<u8>,
}
