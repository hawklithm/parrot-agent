use async_trait::async_trait;
use models::{
    AdapterEnvironmentTestResult, AdapterEnvironmentTestStatus, AdapterModel, AdapterType,
    TestEnvironmentContext,
};

use crate::adapter_registry::ServerAdapterModule;

/// Built-in Codex Local adapter.
///
/// Paperclip exposes the built-in model catalog even when the Codex CLI or an
/// OpenAI API key is not available. Runtime execution performs the actual
/// environment/authentication checks separately.
pub struct CodexLocalAdapter;

impl CodexLocalAdapter {
    pub fn new() -> Self {
        Self
    }

    fn default_models() -> Vec<AdapterModel> {
        [
            ("gpt-5.6", "gpt-5.6"),
            ("gpt-5.6-sol", "gpt-5.6-sol"),
            ("gpt-5.6-terra", "gpt-5.6-terra"),
            ("gpt-5.6-luna", "gpt-5.6-luna"),
            ("gpt-5.4", "gpt-5.4"),
            ("gpt-5.4-mini", "gpt-5.4-mini"),
            ("gpt-5.3-codex-spark", "gpt-5.3-codex-spark"),
            ("gpt-5", "gpt-5"),
            ("o3", "o3"),
            ("o4-mini", "o4-mini"),
            ("gpt-5-mini", "gpt-5-mini"),
            ("gpt-5-nano", "gpt-5-nano"),
            ("o3-mini", "o3-mini"),
            ("codex-mini-latest", "Codex Mini"),
        ]
        .into_iter()
        .map(|(id, label)| AdapterModel {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
    }
}

impl Default for CodexLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for CodexLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::CodexLocal
    }

    fn label(&self) -> &str {
        "Codex"
    }

    fn models(&self) -> Vec<AdapterModel> {
        Self::default_models()
    }

    async fn test_environment(
        &self,
        _ctx: &TestEnvironmentContext,
    ) -> Result<AdapterEnvironmentTestResult, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AdapterEnvironmentTestResult {
            adapter_type: "codex_local".to_string(),
            status: AdapterEnvironmentTestStatus::Pass,
            tested_at: chrono::Utc::now().to_rfc3339(),
            checks: Vec::new(),
        })
    }

    fn supports_instructions_bundle(&self) -> bool {
        true
    }
}
