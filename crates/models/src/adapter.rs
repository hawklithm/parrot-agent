use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    ClaudeLocal,
    CodexLocal,
    Cursor,
    CursorCloud,
    GeminiLocal,
    GrokLocal,
    HermesGateway,
    HermesLocal,
    OpenclawGateway,
    OpencodeLocal,
    PiLocal,
    Process,
    Http,
    AcpxLocal, // retired, tombstone for clear errors
}

impl AdapterType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterType::ClaudeLocal => "claude_local",
            AdapterType::CodexLocal => "codex_local",
            AdapterType::Cursor => "cursor",
            AdapterType::CursorCloud => "cursor_cloud",
            AdapterType::GeminiLocal => "gemini_local",
            AdapterType::GrokLocal => "grok_local",
            AdapterType::HermesGateway => "hermes_gateway",
            AdapterType::HermesLocal => "hermes_local",
            AdapterType::OpenclawGateway => "openclaw_gateway",
            AdapterType::OpencodeLocal => "opencode_local",
            AdapterType::PiLocal => "pi_local",
            AdapterType::Process => "process",
            AdapterType::Http => "http",
            AdapterType::AcpxLocal => "acpx_local",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "claude_local" => Some(AdapterType::ClaudeLocal),
            "codex_local" => Some(AdapterType::CodexLocal),
            "cursor" => Some(AdapterType::Cursor),
            "cursor_cloud" => Some(AdapterType::CursorCloud),
            "gemini_local" => Some(AdapterType::GeminiLocal),
            "grok_local" => Some(AdapterType::GrokLocal),
            "hermes_gateway" => Some(AdapterType::HermesGateway),
            "hermes_local" => Some(AdapterType::HermesLocal),
            "openclaw_gateway" => Some(AdapterType::OpenclawGateway),
            "opencode_local" => Some(AdapterType::OpencodeLocal),
            "pi_local" => Some(AdapterType::PiLocal),
            "process" => Some(AdapterType::Process),
            "http" => Some(AdapterType::Http),
            "acpx_local" => Some(AdapterType::AcpxLocal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterModel {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterModelProfileDefinition {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterEnvironmentCheckLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterEnvironmentCheck {
    pub code: String,
    pub level: AdapterEnvironmentCheckLevel,
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterEnvironmentTestStatus {
    Pass,
    Fail,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterEnvironmentTestResult {
    pub adapter_type: String,
    pub status: AdapterEnvironmentTestStatus,
    pub tested_at: String,
    pub checks: Vec<AdapterEnvironmentCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestEnvironmentContext {
    pub company_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub adapter_config: HashMap<String, serde_json::Value>,
    pub runtime_config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterRuntimeCommandSpec {
    pub command: String,
    pub detect_command: String,
    pub install_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFieldOption {
    pub label: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFieldSchema {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<Vec<ConfigFieldOption>>,
    pub description: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfigSchema {
    pub fields: Vec<ConfigFieldSchema>,
}
