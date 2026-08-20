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
    Http,
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
            AdapterType::Http => write!(f, "http"),
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
            "http" => Ok(AdapterType::Http),
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

/// Test environment check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEnvironmentCheck {
    pub name: String,
    pub status: String, // "pass" | "fail" | "warn"
    pub message: Option<String>,
}

/// Test environment result (aligned with Paperclip's AdapterEnvironmentTestResult)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEnvironmentResult {
    #[serde(rename = "adapterType")]
    pub adapter_type: String,
    pub status: String, // "pass" | "fail" | "partial"
    pub checks: Vec<AdapterEnvironmentCheck>,
    #[serde(rename = "testedAt")]
    pub tested_at: String, // ISO 8601 timestamp
}

/// Test environment context (aligned with Paperclip's AdapterEnvironmentTestContext)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEnvironmentTestContext {
    #[serde(rename = "companyId")]
    pub company_id: String,
    #[serde(rename = "adapterType")]
    pub adapter_type: String,
    pub config: serde_json::Value,
    #[serde(rename = "executionTarget")]
    pub execution_target: Option<serde_json::Value>,
    #[serde(rename = "environmentName")]
    pub environment_name: Option<String>,
    pub deployment: Option<serde_json::Value>,
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
    async fn test_environment(&self, ctx: &AdapterEnvironmentTestContext) -> AdapterResult<TestEnvironmentResult>;

    /// Detect available model from configuration
    async fn detect_model(&self, config: &serde_json::Value) -> AdapterResult<DetectModelResult>;

    /// Check if adapter supports instructions bundle
    /// Check if adapter supports instructions bundle
    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport;

    /// Get agent configuration documentation (optional)
    fn agent_configuration_doc(&self) -> Option<&str> {
        None
    }

    /// Get instructions path key (config key for instructions file path)
    fn instructions_path_key(&self) -> &str {
        "instructionsFilePath"
    }

    /// Get static model list (optional, prefer list_models for dynamic)
    fn models(&self) -> &[ModelInfo] {
        &[]
    }

    /// Get config schema (optional)
    fn get_config_schema(&self) -> Option<serde_json::Value> {
        None
    }

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

    /// Optional lifecycle hook when an agent is approved/hired
    async fn on_hire_approved(
        &self,
        _payload: serde_json::Value,
        _adapter_config: &serde_json::Value,
    ) -> AdapterResult<serde_json::Value> {
        // Default: no-op, return empty success
        Ok(serde_json::json!({}))
    }

    /// Whether this adapter exposes the Skills API (list/sync skills).
    fn supports_skills(&self) -> bool {
        false
    }

    /// Whether this adapter can issue local-agent JWTs for agent authentication.
    fn supports_local_agent_jwt(&self) -> bool {
        false
    }

    /// Whether this adapter exposes model profiles.
    fn supports_model_profiles(&self) -> bool {
        false
    }

    /// Whether this adapter speaks the Agent Client Protocol (ACP).
    fn supports_acp(&self) -> bool {
        false
    }

    /// Whether runtime skills must be materialized on disk for this adapter.
    fn requires_materialized_runtime_skills(&self) -> bool {
        false
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

    /// List all model profiles from all registered adapters
    pub async fn list_adapter_model_profiles(&self) -> Vec<ModelProfile> {
        let mut profiles = Vec::new();
        for adapter in self.adapters.values() {
            if let Ok(adapter_profiles) = adapter.get_model_profiles(&serde_json::json!({})).await {
                profiles.extend(adapter_profiles);
            }
        }
        profiles
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 创建并注册所有内置适配器
pub fn create_default_server_adapter_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    // 注册所有内置适配器
    registry.register(Box::new(ProcessAdapter::new()));
    registry.register(Box::new(HttpAdapter::new()));
    registry.register(Box::new(ClaudeLocalAdapter::new()));
    registry.register(Box::new(CodexLocalAdapter::new()));

    registry
}

/// Claude Local adapter
pub struct ClaudeLocalAdapter {
    #[allow(dead_code)]
    label: String,
}

impl ClaudeLocalAdapter {
    pub fn new() -> Self {
        Self {
            label: "Claude Local".to_string(),
        }
    }
}

impl Default for ClaudeLocalAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServerAdapterModule for ClaudeLocalAdapter {
    fn adapter_type(&self) -> AdapterType {
        AdapterType::ClaudeLocal
    }

    fn label(&self) -> &str {
        "Claude Local"
    }

    async fn list_models(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "claude-sonnet-4".to_string(),
                label: "Claude Sonnet 4".to_string(),
                context_window: Some(200000),
                max_output_tokens: Some(8192),
            },
            ModelInfo {
                id: "claude-opus-4".to_string(),
                label: "Claude Opus 4".to_string(),
                context_window: Some(200000),
                max_output_tokens: Some(8192),
            },
        ])
    }

    async fn get_model_profiles(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>> {
        Ok(vec![])
    }

    async fn test_environment(&self, ctx: &AdapterEnvironmentTestContext) -> AdapterResult<TestEnvironmentResult> {
        let command = ctx.config.get("command").and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()).unwrap_or("claude");
        let mut probe = tokio::process::Command::new(command);
        probe.arg("--version");
        if let Some(cwd) = ctx.config.get("cwd").and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()) {
            probe.current_dir(cwd);
        }
        let mut checks = Vec::new();
        let probe_result = tokio::time::timeout(std::time::Duration::from_secs(5), probe.output()).await;
        match probe_result {
            Ok(Ok(output)) if output.status.success() => checks.push(AdapterEnvironmentCheck {
                name: "claude_cli_resolvable".to_string(), status: "pass".to_string(),
                message: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            }),
            Ok(Ok(output)) => checks.push(AdapterEnvironmentCheck {
                name: "claude_cli_unhealthy".to_string(), status: "fail".to_string(),
                message: Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            }),
            Ok(Err(error)) => checks.push(AdapterEnvironmentCheck {
                name: "claude_cli_missing".to_string(), status: "fail".to_string(),
                message: Some(format!("Claude command '{}' is not executable: {}", command, error)),
            }),
            Err(_) => checks.push(AdapterEnvironmentCheck {
                name: "claude_cli_probe_timeout".to_string(), status: "fail".to_string(),
                message: Some("Claude CLI version probe timed out after 5 seconds".to_string()),
            }),
        }
        if let Some(model) = ctx.config.get("model").and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()) {
            checks.push(AdapterEnvironmentCheck { name: "model_configured".to_string(), status: "pass".to_string(), message: Some(format!("Model configured: {}", model)) });
        }
        let status = if checks.iter().any(|check| check.status == "fail") { "fail" } else { "pass" };

        Ok(TestEnvironmentResult {
            adapter_type: ctx.adapter_type.clone(),
            status: status.to_string(),
            checks,
            tested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn detect_model(&self, _config: &serde_json::Value) -> AdapterResult<DetectModelResult> {
        Ok(DetectModelResult {
            model_id: Some("claude-sonnet-4".to_string()),
            confidence: 1.0,
            source: "claude_local".to_string(),
        })
    }

    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport {
        InstructionsBundleSupport {
            supported: true,
            max_files: Some(100),
            max_size_bytes: Some(10 * 1024 * 1024), // 10MB
        }
    }

    fn supports_skills(&self) -> bool {
        true
    }

    fn supports_local_agent_jwt(&self) -> bool {
        true
    }

    fn supports_model_profiles(&self) -> bool {
        true
    }

    fn supports_acp(&self) -> bool {
        true
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }
}

/// Codex Local adapter
#[allow(dead_code)]
pub struct CodexLocalAdapter {
    label: String,
}

impl CodexLocalAdapter {
 pub fn new() -> Self {
        Self {
            label: "Codex Local".to_string(),
        }
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
        "Codex Local"
    }

    async fn list_models(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "gpt-5.6-sol".to_string(),
        label: "GPT-5.6 Sol".to_string(),
                context_window: Some(200000),
                max_output_tokens: Some(16384),
            },
        ])
    }

    async fn get_model_profiles(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>> {
        Ok(vec![])
    }

    async fn test_environment(&self, ctx: &AdapterEnvironmentTestContext) -> AdapterResult<TestEnvironmentResult> {
        let checks = if let Some(model) = ctx.config.get("model").and_then(|v| v.as_str()) {
            vec![
                AdapterEnvironmentCheck {
                    name: "adapter_available".to_string(),
                    status: "pass".to_string(),
                    message: Some("Codex Local adapter is available".to_string()),
                },
                AdapterEnvironmentCheck {
                    name: "model_configured".to_string(),
                    status: "pass".to_string(),
                    message: Some(format!("Model configured: {}", model)),
                },
            ]
        } else {
            vec![AdapterEnvironmentCheck {
                name: "adapter_available".to_string(),
                status: "pass".to_string(),
                message: Some("Codex Local adapter is available".to_string()),
            }]
        };

        Ok(TestEnvironmentResult {
            adapter_type: ctx.adapter_type.clone(),
            status: "pass".to_string(),
            checks,
            tested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn detect_model(&self, _config: &serde_json::Value) -> AdapterResult<DetectModelResult> {
        Ok(DetectModelResult {
            model_id: Some("gpt-5.6-sol".to_string()),
            confidence: 1.0,
            source: "codex_local".to_string(),
        })
    }

    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport {
        InstructionsBundleSupport {
            supported: true,
            max_files: Some(100),
            max_size_bytes: Some(10 * 1024 * 1024), // 10MB
        }
    }

    fn supports_skills(&self) -> bool {
        true
    }

    fn supports_local_agent_jwt(&self) -> bool {
        true
    }

    fn supports_model_profiles(&self) -> bool {
        true
    }

    fn supports_acp(&self) -> bool {
        true
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
    }
}


/// HTTP webhook adapter. Invocation is implemented by `HttpExecutor`; this
/// registry module owns configuration discovery and environment diagnostics.
pub struct HttpAdapter;

impl HttpAdapter { pub fn new() -> Self { Self } }
impl Default for HttpAdapter { fn default() -> Self { Self::new() } }

#[async_trait]
impl ServerAdapterModule for HttpAdapter {
    fn adapter_type(&self) -> AdapterType { AdapterType::Http }
    fn label(&self) -> &str { "HTTP Webhook" }
    async fn list_models(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>> { Ok(Vec::new()) }
    async fn get_model_profiles(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>> { Ok(Vec::new()) }
    async fn test_environment(&self, ctx: &AdapterEnvironmentTestContext) -> AdapterResult<TestEnvironmentResult> {
        let config = ctx.config.as_object();
        let Some(url_value) = config.and_then(|value| value.get("url")).and_then(|value| value.as_str()).filter(|value| !value.trim().is_empty()) else {
            return Ok(TestEnvironmentResult { adapter_type: ctx.adapter_type.clone(), status: "fail".to_string(), checks: vec![AdapterEnvironmentCheck { name: "http_url_missing".to_string(), status: "fail".to_string(), message: Some("HTTP adapter requires a URL".to_string()) }], tested_at: chrono::Utc::now().to_rfc3339() });
        };
        let parsed = match reqwest::Url::parse(url_value) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => url,
            _ => return Ok(TestEnvironmentResult { adapter_type: ctx.adapter_type.clone(), status: "fail".to_string(), checks: vec![AdapterEnvironmentCheck { name: "http_url_invalid".to_string(), status: "fail".to_string(), message: Some("URL must use http or https".to_string()) }], tested_at: chrono::Utc::now().to_rfc3339() }),
        };
        let method = config.and_then(|value| value.get("method")).and_then(|value| value.as_str()).unwrap_or("POST").to_uppercase();
        let mut checks = vec![AdapterEnvironmentCheck { name: "http_url_valid".to_string(), status: "pass".to_string(), message: Some(parsed.to_string()) }, AdapterEnvironmentCheck { name: "http_method_configured".to_string(), status: "pass".to_string(), message: Some(method) }];
        let probe = reqwest::Client::new().head(parsed).timeout(std::time::Duration::from_secs(3)).send().await;
        match probe {
            Ok(response) if response.status().is_success() || response.status().as_u16() == 405 || response.status().as_u16() == 501 => checks.push(AdapterEnvironmentCheck { name: "http_endpoint_probe_ok".to_string(), status: "pass".to_string(), message: Some(format!("HTTP {}", response.status())) }),
            Ok(response) => checks.push(AdapterEnvironmentCheck { name: "http_endpoint_probe_unexpected_status".to_string(), status: "warn".to_string(), message: Some(format!("HTTP {}", response.status())) }),
            Err(error) => checks.push(AdapterEnvironmentCheck { name: "http_endpoint_probe_failed".to_string(), status: "warn".to_string(), message: Some(error.to_string()) }),
        }
        Ok(TestEnvironmentResult { adapter_type: ctx.adapter_type.clone(), status: "pass".to_string(), checks, tested_at: chrono::Utc::now().to_rfc3339() })
    }
    async fn detect_model(&self, _config: &serde_json::Value) -> AdapterResult<DetectModelResult> { Ok(DetectModelResult { model_id: None, confidence: 0.0, source: "http".to_string() }) }
    fn supports_instructions_bundle(&self) -> InstructionsBundleSupport { InstructionsBundleSupport { supported: false, max_files: None, max_size_bytes: None } }
}

/// Process adapter (default local process adapter)
#[allow(dead_code)]
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
        "Process"
    }

    async fn list_models(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelInfo>> {
        Ok(vec![])
    }

    async fn get_model_profiles(&self, _config: &serde_json::Value) -> AdapterResult<Vec<ModelProfile>> {
        Ok(vec![])
    }
    async fn test_environment(&self, ctx: &AdapterEnvironmentTestContext) -> AdapterResult<TestEnvironmentResult> {
        // TODO: Integrate with EnvironmentRuntimeService
        // - acquire_run_lease()
        // - test basic connectivity
        // - release_run_lease()

        Ok(TestEnvironmentResult {
            adapter_type: ctx.adapter_type.clone(),
            status: "pass".to_string(),
            checks: vec![AdapterEnvironmentCheck {
                name: "adapter_available".to_string(),
                status: "pass".to_string(),
                message: Some("Process adapter is available".to_string()),
            }],
            tested_at: chrono::Utc::now().to_rfc3339(),
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

    fn supports_skills(&self) -> bool {
        false
    }

    fn supports_local_agent_jwt(&self) -> bool {
        false
    }

    fn supports_model_profiles(&self) -> bool {
        false
    }

    fn supports_acp(&self) -> bool {
        false
    }

    fn requires_materialized_runtime_skills(&self) -> bool {
        false
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
        assert_eq!(adapter.label(), "Process");

        let models = adapter.list_models(&serde_json::json!({})).await.unwrap();
        assert_eq!(models.len(), 0);

        let ctx = AdapterEnvironmentTestContext {
            company_id: "test-company".to_string(),
            adapter_type: "process".to_string(),
            config: serde_json::json!({}),
            execution_target: None,
            environment_name: None,
            deployment: None,
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, "pass");

        let bundle_support = adapter.supports_instructions_bundle();
        assert!(bundle_support.supported);
    }

    #[tokio::test]
    async fn claude_environment_probe_rejects_missing_command() {
        let adapter = ClaudeLocalAdapter::new();
        let ctx = AdapterEnvironmentTestContext {
            company_id: "test-company".to_string(),
            adapter_type: "claude_local".to_string(),
            config: serde_json::json!({"command": "parrot-command-that-does-not-exist"}),
            execution_target: None,
            environment_name: None,
            deployment: None,
        };
        let result = adapter.test_environment(&ctx).await.unwrap();
        assert_eq!(result.status, "fail");
        assert!(result.checks.iter().any(|check| check.name == "claude_cli_missing"));
    }

    #[test]
    fn test_adapter_capabilities_aligned_with_paperclip() {
        let claude = ClaudeLocalAdapter::new();
        // claude_local 支持 skills / local-agent JWT / model profiles / ACP，
        // 但不要求物料化运行时 skills。
        assert!(claude.supports_skills());
        assert!(claude.supports_local_agent_jwt());
        assert!(claude.supports_model_profiles());
        assert!(claude.supports_acp());
        assert!(!claude.requires_materialized_runtime_skills());

        let codex = CodexLocalAdapter::new();
        assert!(codex.supports_skills());
        assert!(codex.supports_local_agent_jwt());
        assert!(codex.supports_model_profiles());
        assert!(codex.supports_acp());
        assert!(!codex.requires_materialized_runtime_skills());

        let process = ProcessAdapter::new();
        assert!(!process.supports_skills());
        assert!(!process.supports_local_agent_jwt());
        assert!(!process.supports_model_profiles());
        assert!(!process.supports_acp());
        assert!(!process.requires_materialized_runtime_skills());
    }
}
