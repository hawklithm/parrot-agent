use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Asset {
    pub id: Uuid,
    pub company_id: Uuid,
    pub provider: String,
    pub object_key: String,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub original_filename: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAssetInput {
    pub company_id: Uuid,
    pub provider: String,
    pub object_key: String,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub original_filename: Option<String>,
    pub created_by_agent_id: Option<Uuid>,
    pub created_by_user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetContent {
    pub content_type: String,
    pub body: Vec<u8>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePutResult {
    pub provider: String,
    pub object_key: String,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
}

pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024; // 10 MB
