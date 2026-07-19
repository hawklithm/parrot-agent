use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: Uuid,
    pub plugin_key: String,
    pub name: String,
    pub version: String,
    pub api_version: i32,
    pub categories: serde_json::Value,
    pub install_order: i32,
    pub status: String,
    pub package_name: Option<String>,
    pub install_path: Option<String>,
    pub manifest: serde_json::Value,
    pub config: serde_json::Value,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
