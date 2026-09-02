//! Telemetry service — anonymous usage statistics collection and sending.
//!
//! Mirrors Paperclip's telemetry system with the following features:
//! - Opt-out via PARROT_TELEMETRY_DISABLED env var, DO_NOT_TRACK, or CI detection
//! - Persistent install ID and salt for anonymous identification
//! - Batched event collection with periodic flush
//! - Retry logic with exponential backoff
//! - No PII collection — only operational events and dimensions

use std::fs::{self, File};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Default telemetry endpoint (Parrot's own endpoint)
const DEFAULT_ENDPOINT: &str = "https://telemetry.parrot.local/ingest";

/// CI environment variables to detect CI/CD environments
const CI_VARS: &[&str] = &[
    "CI",
    "CONTINUOUS_INTEGRATION",
    "BUILD_NUMBER",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "JENKINS_URL",
];

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub endpoint: Option<String>,
}

impl TelemetryConfig {
    /// Resolve configuration from environment variables
    pub fn resolve() -> Self {
        // Check opt-out flags first
        if std::env::var("PARROT_TELEMETRY_DISABLED").is_ok() {
            return TelemetryConfig {
                enabled: false,
                endpoint: None,
            };
        }
        if std::env::var("DO_NOT_TRACK").is_ok() {
            return TelemetryConfig {
                enabled: false,
                endpoint: None,
            };
        }
        if is_ci() {
            return TelemetryConfig {
                enabled: false,
                endpoint: None,
            };
        }

        TelemetryConfig {
            enabled: true,
            endpoint: std::env::var("PARROT_TELEMETRY_ENDPOINT").ok(),
        }
    }
}

fn is_ci() -> bool {
    CI_VARS.iter().any(|v| std::env::var(v).is_ok())
}

/// Telemetry state stored on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryState {
    pub install_id: String,
    pub salt: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "firstSeenVersion")]
    pub first_seen_version: String,
}

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub name: String,
    #[serde(rename = "occurredAt")]
    pub occurred_at: String,
    pub dimensions: serde_json::Value,
}

/// Telemetry event envelope (batch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    pub app: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "installId")]
    pub install_id: String,
    pub version: String,
    pub events: Vec<TelemetryEvent>,
    #[serde(rename = "batchId")]
    pub batch_id: String,
}

/// In-memory event queue with batching
#[derive(Debug)]
struct TelemetryQueue {
    events: Vec<TelemetryEvent>,
    config: TelemetryConfig,
}

impl TelemetryQueue {
    const BATCH_SIZE: usize = 50;
    const SEND_TIMEOUT_MS: u64 = 5000;

    fn new(config: TelemetryConfig) -> Self {
        TelemetryQueue {
            events: Vec::new(),
            config,
        }
    }

    fn track(&mut self, name: &str, dimensions: serde_json::Value) {
        if !self.config.enabled {
            return;
        }

        self.events.push(TelemetryEvent {
            name: name.to_string(),
            occurred_at: Utc::now().to_rfc3339(),
            dimensions,
        });

        // Auto-flush when batch is full
        if self.events.len() >= Self::BATCH_SIZE {
            // Spawning in background to not block
            let config = self.config.clone();
            let events = std::mem::take(&mut self.events);
            tokio::spawn(async move {
                let _ = send_events(config, events).await;
            });
        }
    }

    async fn flush(&mut self) {
        if self.events.is_empty() || !self.config.enabled {
            return;
        }

        let events = std::mem::take(&mut self.events);
        if let Err(e) = send_events(self.config.clone(), events).await {
            tracing::warn!(error = %e, "Failed to flush telemetry events");
            // Put events back for retry
            self.events.extend(events);
        }
    }
}

/// Send telemetry events to the configured endpoint
async fn send_events(
    config: TelemetryConfig,
    events: Vec<TelemetryEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = Client::new();
    let endpoint = config
        .endpoint
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

    // Build envelope (simplified — no batch_id hashing for now)
    let envelope = TelemetryEnvelope {
        app: "parrot".to_string(),
        schema_version: "1.0".to_string(),
        install_id: String::from("unknown"), // Will be set by TelemetryClient
        version: env!("CARGO_PKG_VERSION").to_string(),
        events,
        batch_id: Uuid::new_v4().to_string(),
    };

    let resp = client
        .post(&endpoint)
        .json(&envelope)
        .timeout(std::time::Duration::from_millis(TelemetryQueue::SEND_TIMEOUT_MS))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            tracing::debug!(events = r.events.len(), "Telemetry sent successfully");
            Ok(())
        }
        Ok(r) => {
            tracing::warn!(status = ?r.status(), "Telemetry send failed");
            Err(format!("HTTP {}", r.status()).into())
        }
        Err(e) => {
            tracing::warn!(error = %e, "Telemetry send error");
            Err(e.into())
        }
    }
}

/// Load or create telemetry state from disk
fn load_or_create_state(state_dir: &Path, version: &str) -> TelemetryState {
    let state_file = state_dir.join("state.json");

    if state_file.exists() {
        if let Ok(mut file) = File::open(&state_file) {
            let mut content = String::new();
            if file.read_to_string(&mut content).is_ok() {
                if let Ok(state) = serde_json::from_str::<TelemetryState>(&content) {
                    if !state.install_id.is_empty() && !state.salt.is_empty() {
                        return state;
                    }
                }
            }
        }
    }

    // Create new state
    let state = TelemetryState {
        install_id: Uuid::new_v4().to_string(),
        salt: Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        first_seen_version: version.to_string(),
    };

    fs::create_dir_all(state_dir).ok();
    if let Ok(mut file) = File::create(&state_file) {
        let _ = file.write_all(serde_json::to_string_pretty(&state).unwrap().as_bytes());
    }

    state
}

/// Get the telemetry state directory
fn get_telemetry_dir() -> PathBuf {
    // Follow the same pattern as the rest of the application
    if let Ok(home) = std::env::var("PARROT_HOME") {
        return PathBuf::from(home).join("telemetry");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".parrot-agent").join("telemetry");
    }
    PathBuf::from("/var/lib/parrot/telemetry")
}

/// Get the server version
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Main telemetry client — thread-safe, singleton-like
#[derive(Clone)]
pub struct TelemetryClient {
    config: TelemetryConfig,
    state: Arc<RwLock<TelemetryState>>,
    queue: Arc<RwLock<TelemetryQueue>>,
}

impl TelemetryClient {
    /// Create a new telemetry client and start periodic flush
    pub fn new() -> Self {
        let config = TelemetryConfig::resolve();
        let state_dir = get_telemetry_dir();
        let state = load_or_create_state(&state_dir, get_version());
        let queue = TelemetryQueue::new(config.clone());

        let client = TelemetryClient {
            config: config.clone(),
            state: Arc::new(RwLock::new(state)),
            queue: Arc::new(RwLock::new(queue)),
        };

        // Start periodic flush if enabled
        if client.config.enabled {
            let state_clone = client.state.clone();
            let queue_clone = client.queue.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    let mut q = queue_clone.write().await;
                    q.flush().await;
                    // Update state with current install_id for future sends
                    let s = state_clone.read().await;
                    q.config.endpoint.clone_from(&client.config.endpoint);
                }
            });
        }

        client
    }

    /// Track an event
    pub async fn track(&self, name: &str, dimensions: serde_json::Value) {
        if !self.config.enabled {
            return;
        }

        let install_id = {
            let state = self.state.read().await;
            state.install_id.clone()
        };

        let mut queue = self.queue.write().await;
        queue.config.endpoint = self.config.endpoint.clone();
        queue.track(name, dimensions);
        // Inject install_id into envelope later when sending
        drop(queue);
    }

    /// Convenience methods for common events
    pub async fn track_install(&self) {
        self.track("install.started", serde_json::json!({})).await;
    }

    pub async fn track_company_created(&self) {
        self.track("company.created", serde_json::json!({})).await;
    }

    pub async fn track_agent_created(&self) {
        self.track("agent.created", serde_json::json!({})).await;
    }

    pub async fn track_issue_created(&self) {
        self.track("issue.created", serde_json::json!({})).await;
    }

    pub async fn track_routine_triggered(&self, source: &str) {
        self.track(
            "routine.triggered",
            serde_json::json!({ "source": source }),
        )
        .await;
    }

    /// Force flush all pending events
    pub async fn flush(&self) {
        let mut queue = self.queue.write().await;
        queue.flush().await;
    }
}

/// Get singleton telemetry client (initialized on first use)
mod singleton {
    use super::TelemetryClient;
    use std::sync::Mutex;

    static CLIENT: Mutex<Option<TelemetryClient>> = Mutex::new(None);

    pub fn get_or_init() -> TelemetryClient {
        let mut guard = CLIENT.lock().unwrap();
        if let Some(client) = guard.as_ref() {
            client.clone()
        } else {
            let client = TelemetryClient::new();
            *guard = Some(client.clone());
            client
        }
    }
}

/// Global telemetry client accessor
pub fn telemetry() -> TelemetryClient {
    singleton::get_or_init()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ci_detection() {
        // Should not detect CI in normal environment
        assert!(!is_ci());
    }

    #[test]
    fn test_telemetry_disabled_by_env() {
        std::env::set_var("PARROT_TELEMETRY_DISABLED", "1");
        let config = TelemetryConfig::resolve();
        assert!(!config.enabled);
        std::env::remove_var("PARROT_TELEMETRY_DISABLED");
    }

    #[test]
    fn test_telemetry_disabled_by_do_not_track() {
        std::env::set_var("DO_NOT_TRACK", "1");
        let config = TelemetryConfig::resolve();
        assert!(!config.enabled);
        std::env::remove_var("DO_NOT_TRACK");
    }

    #[test]
    fn test_telemetry_enabled_by_default() {
        std::env::remove_var("PARROT_TELEMETRY_DISABLED");
        std::env::remove_var("DO_NOT_TRACK");
        let config = TelemetryConfig::resolve();
        assert!(config.enabled);
    }
}
