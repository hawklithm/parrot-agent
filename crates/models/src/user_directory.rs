use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User profile information for directory entries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Company user directory entry (active members only)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyUserDirectoryEntry {
    pub principal_id: Uuid,
    pub status: String, // "active"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserProfile>,
}

/// Response for company user directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyUserDirectoryResponse {
    pub users: Vec<CompanyUserDirectoryEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Admin user directory entry (instance-wide)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDirectoryEntry {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub is_instance_admin: bool,
    pub active_company_membership_count: i32,
}

/// Response for admin user directory listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserDirectoryResponse {
    pub users: Vec<AdminUserDirectoryEntry>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Query parameters for user directory search
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDirectoryQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    20
}
