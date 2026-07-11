use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Remote secret import candidate status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSecretImportCandidateStatus {
    Ready,
    Duplicate,
    Conflict,
}

/// Remote secret import conflict type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportConflict {
    pub field: String, // "provider" | "externalRef" | "managedMode"
    pub remote_value: String,
    pub local_value: String,
}

/// Remote secret import candidate (secret found in external provider)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportCandidate {
    pub name: String,
    pub external_ref: String,
    pub status: RemoteSecretImportCandidateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_secret_id: Option<Uuid>,
    pub conflicts: Vec<RemoteSecretImportConflict>,
}

/// Remote secret import preview request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportPreviewRequest {
    pub provider_config_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    100
}

/// Remote secret import preview result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportPreviewResult {
    pub provider_config_id: Uuid,
    pub provider: String, // "aws_secrets_manager" | "gcp_secret_manager" | "vault"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_token: Option<String>,
    pub candidates: Vec<RemoteSecretImportCandidate>,
}

/// Remote secret import row status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteSecretImportRowStatus {
    Imported,
    Skipped,
    Error,
}

/// Remote secret import row result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportRowResult {
    pub name: Str   pub external_ref: String,
    pub status: RemoteSecretImportRowStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub conflicts: Vec<RemoteSecretImportConflict>,
}

/// Remote secret import request (execute batch import)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportRequest {
    pub provider_config_id: Uuid,
    pub secret_names: Vec<String>, // Select which candidates to import
    #[serde(default)]
    pub overwrite_conflicts: bool,
}

/// Remote secret import result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSecretImportResult {
    pub provider_config_id: Uuid,
    pub provider: String,
    pub imported_count: usize,
    pub skipped_count: usize,
    pub error_count: usize,
    pub results: Vec<RemoteSecretImportRowResult>,
}
