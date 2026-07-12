use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Available skill (minimal metadata for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableSkill {
    pub name: String,
    pub description: String,
    pub is_paperclip_managed: bool,
}

/// Skill index entry (metadata for skill registry)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillIndexEntry {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub category: Option<String>,
    pub is_paperclip_managed: bool,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Skill example (usage demonstration)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExample {
    pub title: String,
    pub description: Option<String>,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
}

/// Skill parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

/// Skill details (full documentation)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetails {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub is_paperclip_managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<SkillParameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<SkillExample>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_example: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// Response for available skills endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableSkillsResponse {
    pub skills: Vec<AvailableSkill>,
}

/// Response for skill index endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillIndexResponse {
    pub skills: Vec<SkillIndexEntry>,
}

/// Alias for SkillDetails (backwards compatibility)
pub type SkillDetail = SkillDetails;
