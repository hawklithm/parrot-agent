use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Invite token validation and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteToken {
    pub id: Uuid,
    pub company_id: Uuid,
    pub invite_type: String, // "company_join" | "openclaw"
    pub allowed_join_types: String, // "both" | "human" | "agent"
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub is_expired: bool,
    pub is_revoked: bool,
}

/// Onboarding document manifest (Markdown content)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteOnboardingManifest {
    pub has_onboarding_doc: bool,
    pub markdown: Option<String>,
    pub plain_text: Option<String>,
}

/// Company logo metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanyLogo {
    pub content_type: String, // "image/png" | "image/jpeg" | "image/svg+xml"
    pub data: Vec<u8>,
}

/// Skill available in invite scope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteScopedSkill {
    pub name: String,
    pub description: String,
    pub is_paperclip_managed: bool,
}

/// Skill index response (list of skills available in invite scope)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteSkillIndex {
    pub skills: Vec<InviteScopedSkill>,
}

/// Skill details with parameters and examples
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteSkillParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteSkillExample {
    pub title: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteSkillDetails {
    pub name: String,
    pub slug: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<InviteSkillParameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<InviteSkillExample>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_notes: Option<String>,
}
