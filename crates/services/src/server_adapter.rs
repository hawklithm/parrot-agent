use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Adapter not found: {0}")]
    AdapterNotFound(String),

    #[error("Model not supported: {0}")]
    ModelNotSupported(String),

    #[error("Environment test failed: {0}")]
    EnvironmentTestFailed(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub type AdapterResult<T> = Result<T, AdapterError>;

/// Adapter type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    ClaudeLocal,
    Cursor,
    Opencode,
    Process,
    CodexLocal,
    OpenaiCompatible,
}

impl std::fmt::Display for AdapterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterType::ClaudeLocal => write!(f, "claude_local"),
            AdapterType::Cursor => write!(f, "cursor"),
            AdapterType::Opencode => write!(f, "opencode"),
            AdapterType::Process => write!(f, "process"),
            AdapterType::CodexLocal => write!(f, "codex_local"),
            AdapterType::OpenaiCompatible => write!(f, "openai_compatible"),
        }
    }
}

impl std::str::FromStr for AdapterType {
    type Err = AdapterError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude_local" => Ok(AdapterType::ClaudeLocal),
            "cursor" => Ok(AdapterType::Cursor),
            "opencode" => Ok(AdapterType::Opencode),
            "process" => Ok(AdapterType::Process),
            "codex_local" => Ok(AdapterType::CodexLocal),
            "openai_compatible" => Ok(AdapterType::OpenaiCompatible),
            _ => Err(AdapterError::AdapterNotFound(s.to_string())),
        }
    }
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub context_window: Option<usize>,
    pub max_output_tokens: Option<usize>,
}

/// Model profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub model_id: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub pricing: Option<ModelPricing>,
}

/// Model pricing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub currency: String,
}

/// Test environment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEnvironmentResult {
    pub ok: bool,
    pub adapter_type: String,
    pub summary: String,
    pub details: HashMap<String, serde_json::Value>,
    pub detected_model: Option<String>,
}

/// Detect model result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectModelResult {
    pub model_id: Option<String>,
    pub confidence: f64,
    pub source: String,
}

/// Instructions bundle support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionsBundleSupport {
    pub supported: bool,
    pub max_files: Option<usize>,
    pub max_size_bytes: Option<usize>,
}

/// Server adapter trait
#[async_trait]
pub trait ServerAdapterModule: Send + Sync {
    /// Get adapter type
    fn adapter_type(&self) -> AdapterType;

    /// Get adapter label
    fn label(&self) -> &str;

    /// List supported models
    async fn list_models(&self, config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>>;

    /// Get model profiles
    async fn get_model_profiles(&self, config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>>;

    /// Test environment connectivity and configuration
    async fn test_environment(&self, config: &serde_json::Value) -> AdapterResult<TestEnvironmentResult>;

    /// Detect available model from configuration
    async fn detect_model(&self, config: &serde_json::Value) -> AdapterResult<DetectModelResult>;

    /// Check if adapter supports instructions bundle
    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport;

    /// Normalize adapter configuration for persistence
    fn normalize_config(&self, config: serde_json::Value) -> AdapterResult<serde_json::Value> {
        Ok(config)
    }

    /// Apply default configuration for new agents
    fn apply_create_defaults(&self, config: serde_json::Value) -> AdapterResult<serde_json::Value> {
        Ok(config)
    }

    /// Return provider quota windows when this adapter can expose them.  The
    /// default keeps adapters without a quota API out of the aggregate result.
    async fn get_quota_windows(&self) -> AdapterResult<Vec<crate::cost_service::QuotaWindow>> {
        Ok(Vec::new())
    }
}

/// Adapter registry
pub struct AdapterRegistry {
    adapters: HashMap<AdapterType, Box<dyn ServerAdapterModule>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register an adapter
    pub fn register(&mut self, adapter: Box<dyn ServerAdapterModule>) {
        let adapter_type = adapter.adapter_type();
        self.adapters.insert(adapter_type, adapter);
    }

    /// Find adapter by type
    pub fn find_adapter(&self, adapter_type: AdapterType) -> AdapterResult<&dyn ServerAdapterModule> {
        self.adapters
            .get(&adapter_type)
            .map(|boxed| &**boxed)
            .ok_or_else(|| AdapterError::AdapterNotFound(adapter_type.to_string()))
    }

    /// List all registered adapters
    pub fn list_all(&self) -> Vec<AdapterType> {
        self.adapters.keys().copied().collect()
    }

    pub fn adapters(&self) -> Vec<&dyn ServerAdapterModule> {
        self.adapters.values().map(|adapter| adapter.as_ref()).collect()
    }

    /// Check if adapter is registered
    pub fn has_adapter(&self, adapter_type: AdapterType) -> bool {
        self.adapters.contains_key(&adapter_type)
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Process adapter (default local process adapter)
pub struct ProcessAdapter {
    label: String,
}

impl ProcessAdapter {
    pub fn new() -> Self {
        Self {
            label: "Local Process".to_string(),
        }
    }
}

impl Default for ProcessAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for ProcessAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::Process
    }

    fn label(&self) -> &str {
        &self.label
    }

    async fn list_models(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>> {
        // Process adapter doesn't have specific models
        Ok(vec![])
    }

    async fn get_model_profiles(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>> {
        Ok(vec![])
    }

    async fn test_environment(&self, _config: &serde_json::Value) -> AdapterResult<TestEnvironmentResult> {
        // TODO: Integrate with EnvironmentRuntimeService
        // - acquire_run_lease()
        // - test basic connectivity
        // - release_run_lease()

        Ok(TestEnvironmentResult {
            ok: true,
            adapter_type: self.adapter_type().to_string(),
            summary: "Process adapter is available".to_string(),
            details: HashMap::new(),
            detected_model: None,
        })
    }

    async fn detect_model(&self, _config: &serde_json::Value) -> AdapterResult<DetectModelResult> {
        Ok(DetectModelResult {
            model_id: None,
            confidence: 0.0,
            source: "process".to_string(),
        })
    }

    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport {
        InstructionsBundleSupport {
            supported: true,
            max_files: Some(100),
            max_size_bytes: Some(10 * 1024 * 1024), // 10MB
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_type_display() {
        assert_eq!(AdapterType::ClaudeLocal.to_string(), "claude_local");
        assert_eq!(AdapterType::Process.to_string(), "process");
    }

    #[test]
    fn test_adapter_type_from_str() {
        assert_eq!(
            "claude_local".parse::<AdapterType>().unwrap(),
            AdapterType::ClaudeLocal
        );
        assert_eq!(
            "process".parse::<AdapterType>().unwrap(),
            AdapterType::Process
        );
        assert!("invalid".parse::<AdapterType>().is_err());
    }

    #[test]
    fn test_adapter_registry() {
        let mut registry = AdapterRegistry::new();
        let adapter = Box::new(ProcessAdapter::new());

        registry.register(adapter);

        assert!(registry.has_adapter(AdapterType::Process));
        assert!(!registry.has_adapter(AdapterType::ClaudeLocal));

        let found = registry.find_adapter(AdapterType::Process).unwrap();
        assert_eq!(found.adapter_type(), AdapterType::Process);
    }

    #[tokio::test]
    async fn test_process_adapter() {
        let adapter = ProcessAdapter::new();

        assert_eq!(adapter.adapter_type(), AdapterType::Process);
        assert_eq!(adapter.label(), "Local Process");

        let models = adapter.list_models(&serde_json::json!({})).await.unwrap();
        assert_eq!(models.len(), 0);

        let result = adapter.test_environment(&serde_json::json!({})).await.unwrap();
        assert!(result.ok);

        let bundle_support = adapter.supports_instructions_bundle();
        assert!(bundle_support.supported);
    }
}
