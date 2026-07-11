use serde::{Deserialize, Serialize};

/// Request to generate OpenClaw invite prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawInvitePromptRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_message: Option<String>,
}

/// Response containing generated invite prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawInvitePromptResponse {
    pub prompt: String,
    pub company_name: String,
    pub company_id: uuid::Uuid,
}
