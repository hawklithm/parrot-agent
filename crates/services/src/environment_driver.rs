use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use models::{Environment, EnvironmentDriver, LocalEnvironmentConfig, SshEnvironmentConfig, SandboxEnvironmentConfig};

/// Environment probe result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProbeResult {
    pub ok: bool,
    pub driver: String,
    pub summary: String,
    pub details: Option<serde_json::Value>,
}

/// Lease acquisition result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseAcquisitionResult {
    pub lease_id: Uuid,
    pub provider: String,
    pub connection_info: serde_json::Value,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Environment driver trait
#[async_trait]
pub trait EnvironmentDriverTrait: Send + Sync {
    /// Probe environment connectivity and health
    async fn probe(&self, config: &DriverConfig) -> Result<EnvironmentProbeResult, String>;
    
    /// Acquire a lease on this environment
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        config: &DriverConfig,
    ) -> Result<LeaseAcquisitionResult, String>;
    
    /// Release a lease on this environment
    async fn release_lease(
        &self,
        lease_id: Uuid,
        provider_lease_id: Option<String>,
    ) -> Result<(), String>;
    
    /// Ensure environment is ready for use
    async fn ensure_ready(&self, config: &DriverConfig) -> Result<bool, String>;
}

/// Driver configuration enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DriverConfig {
    Local(LocalEnvironmentConfig),
    Ssh(SshEnvironmentConfig),
    Sandbox(SandboxEnvironmentConfig),
    Plugin(PluginDriverConfig),
}

/// Plugin driver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDriverConfig {
    pub plugin_key: String,
    pub driver_key: String,
    pub driver_config: serde_json::Value,
}

/// Driver registry
pub struct DriverRegistry {
    drivers: HashMap<String, Box<dyn EnvironmentDriverTrait>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }
    
    /// Register a driver
    pub fn register(&mut self, name: String, driver: Box<dyn EnvironmentDriverTrait>) {
        self.drivers.insert(name, driver);
    }
    
    /// Find a driver by name
    pub fn find_driver(&self, name: &str) -> Option<&Box<dyn EnvironmentDriverTrait>> {
        self.drivers.get(name)
    }
}

/// Resolve environment driver config from Environment model
pub fn resolve_environment_driver_config_for_runtime(
    environment: &Environment,
) -> Result<DriverConfig, String> {
    match environment.driver {
        EnvironmentDriver::Local => {
            let config: LocalEnvironmentConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Failed to parse local config: {}", e))?;
            Ok(DriverConfig::Local(config))
        }
        EnvironmentDriver::Ssh => {
            let config: SshEnvironmentConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Failed to parse SSH config: {}", e))?;
            Ok(DriverConfig::Ssh(config))
        }
        EnvironmentDriver::Sandbox => {
            let config: SandboxEnvironmentConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Failed to parse sandbox config: {}", e))?;
            Ok(DriverConfig::Sandbox(config))
        }
        EnvironmentDriver::Plugin => {
            let config: PluginDriverConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Failed to parse plugin config: {}", e))?;
            Ok(DriverConfig::Plugin(config))
        }
    }
}

/// Parse environment driver config by driver type
pub fn parse_environment_driver_config(
    driver: &EnvironmentDriver,
    config_json: &serde_json::Value,
) -> Result<DriverConfig, String> {
    match driver {
        EnvironmentDriver::Local => {
            let config: LocalEnvironmentConfig = serde_json::from_value(config_json.clone())
                .map_err(|e| format!("Failed to parse local config: {}", e))?;
            Ok(DriverConfig::Local(config))
        }
        EnvironmentDriver::Ssh => {
            let config: SshEnvironmentConfig = serde_json::from_value(config_json.clone())
                .map_err(|e| format!("Failed to parse SSH config: {}", e))?;
            Ok(DriverConfig::Ssh(config))
        }
        EnvironmentDriver::Sandbox => {
            let config: SandboxEnvironmentConfig = serde_json::from_value(config_json.clone())
                .map_err(|e| format!("Failed to parse sandbox config: {}", e))?;
            Ok(DriverConfig::Sandbox(config))
        }
        EnvironmentDriver::Plugin => {
            let config: PluginDriverConfig = serde_json::from_value(config_json.clone())
                .map_err(|e| format!("Failed to parse plugin config: {}", e))?;
            Ok(DriverConfig::Plugin(config))
        }
    }
}

/// Resolve driver config for a given DriverConfig enum
pub fn resolve_driver_config(config: &DriverConfig) -> String {
    match config {
        DriverConfig::Local(_) => "local".to_string(),
        DriverConfig::Ssh(_) => "ssh".to_string(),
        DriverConfig::Sandbox(_) => "sandbox".to_string(),
        DriverConfig::Plugin(_) => "plugin".to_string(),
    }
}
