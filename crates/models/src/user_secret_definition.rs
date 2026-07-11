use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User secret definition (company-level definition of required user secrets)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSecretDefinition {
    pub id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub provider: String,
    pub managed_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_guidance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_agent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_user_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSecretValue {
    pub id: Uuid,
    pub company_id: Uuid,
    pub user_id: Uuid,
    pub user_secret_definition_id: Uuid,
    pub key: String,
    pub name: String,
    pub provider: String,
    pub status: String,
    pub managed_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_config_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
    pub latest_version: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rotated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSecretCoverageSummary {
    pub definition_id: Uuid,
    pub configured_count: i32,
    pub missing_count: i32,
    pub inactive_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyUserSecretEntry {
    pub definition: UserSecretDefinition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<UserSecretValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserSecretDefinitionRequest {
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_managed_mode")]
    pub managed_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_guidance: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserSecretDefinitionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_guidance: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertUserSecretRequest {
    pub definition_id: Uuid,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBinding {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub config_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_key: Option<String>,
    pub required: bool,
}

fn default_provider() -> String {
    "local_encrypted".to_string()
}

fn default_managed_mode() -> String {
    "managed".to_string()
}
