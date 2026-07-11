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

/// Local environment driver implementation
pub struct LocalDriver;

impl LocalDriver {
    pub fn new() -> Self {
        Self
    }
    
    async fn check_tool_availability(&self, tool: &str) -> Result<bool, String> {
        let output = tokio::process::Command::new("which")
            .arg(tool)
            .output()
            .await
            .map_err(|e| format!("Failed to check tool {}: {}", tool, e))?;
        
        Ok(output.status.success())
    }
}

#[async_trait]
impl EnvironmentDriverTrait for LocalDriver {
    async fn probe(&self, config: &DriverConfig) -> Result<EnvironmentProbeResult, String> {
        let _local_config = match config {
            DriverConfig::Local(c) => c,
            _ => return Err("Invalid config type for LocalDriver".to_string()),
        };
        
        let tools = vec!["git", "node", "npm"];
        let mut available_tools = Vec::new();
        let mut missing_tools = Vec::new();
        
        for tool in tools {
            if self.check_tool_availability(tool).await? {
                available_tools.push(tool.to_string());
            } else {
                missing_tools.push(tool.to_string());
            }
        }
        
        let ok = missing_tools.is_empty();
        let summary = if ok {
            "All required tools available".to_string()
        } else {
            format!("Missing tools: {}", missing_tools.join(", "))
        };
        
        Ok(EnvironmentProbeResult {
            ok,
            driver: "local".to_string(),
            summary,
            details: Some(serde_json::json!({
                "availableTools": available_tools,
                "missingTools": missing_tools,
            })),
        })
    }
    
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        _config: &DriverConfig,
    ) -> Result<LeaseAcquisitionResult, String> {
        let lease_id = Uuid::new_v4();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
        
        Ok(LeaseAcquisitionResult {
            lease_id,
            provider: "local".to_string(),
            connection_info: serde_json::json!({
                "environmentId": environment_id,
                "type": "local",
            }),
            expires_at: Some(expires_at),
        })
    } async fn release_lease(
        &self,
        _lease_id: Uuid,
        _provider_lease_id: Option<String>,
    ) -> Result<(), String> {
        Ok(())
    }
    
    async fn ensure_ready(&self, config: &DriverConfig) -> Result<bool, String> {
        let probe_result = self.probe(config).await?;
        Ok(probe_result.ok)
    }
}

/// SSH environment driver implementation
pub struct SshDriver;

impl SshDriver {
    pub fn new() -> Self {
        Self
    }
    
    async fn test_ssh_connection(&self, config: &SshEnvironmentConfig) -> Result<bool, String> {
        let mut cokio::process::Command::new("ssh");
        cmd.arg("-o").arg("ConnectTimeout=5")
            .arg("-o").arg("BatchMode=yes");
        
        if !config.strict_host_key_checking {
            cmd.arg("-o").arg("StrictHostKeyChecking=no");
        }
        
        let host_arg = format!("{}@{}", config.username, config.host);
        cmd.arg(&host_arg)
            .arg("echo")
            .arg("ok");
        
        let output = cmd.output().await
            .map_err(|e| format!("Failed to test SSH connection: {}", e))?;
        
        Ok(output.status.success())
    }
}

#[async_trait]
impl EnvironmentDriverTrait for SshDriver {
    async fn probe(&self, config: &DriverConfig) -> Result<EnvironmentProbeResult, String> {
        let ssh_config = match config {
            DriverConfig::Ssh(c) => c,
            _ => return Err("Invalid config type for SshDriver".to_string()),
        };
        
        let connection_ok = self.test_ssh_connection(ssh_config).await?;
        
        if !connection_ok {
            return Ok(EnvironmentProbeResult {
                ok: false,
                driver: "ssh".to_string(),
                summary: format!("Failed to connect to {}@{}", ssh_config.username, ssh_config.host),
                details: Some(serde_json::json!({
                    "host": ssh_config.host,
                    "port": ssh_config.port,
                    "username": ssh_config.username,
                })),
            });
        }
        
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: "ssh".to_string(),
            summary: format!("Successfully connected to {}@{}", ssh_config.username, ssh_config.host),
            details: Some(serde_json::json!({
                "host": ssh_config.host,
                "port": ssh_config.port,
                "username": ssh_config.username,
                "remoteWorkspacePath": ssh_config.remote_workspace_path,
            })),
        })
    }
    
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        config: &DriverConfig,
    ) -> Result<LeaseAcquisitionResult, String> {
        let ssh_config = match config {
            DriverConfig::Ssh(c) => c,
            _ => return Err("Invalid config type for SshDriver".to_string()),
        };
        
        let connection_ok = self.test_ssh_connection(ssh_config).await?;
        if !connection_ok {
            return Err(format!("Failed to establish SSH connection to {}@{}", 
                ssh_config.username, ssh_config.host));
        }
        
        let lease_id = Uuid::new_v4();
        let expires_at = chrono::Utc::now() + chrono::Duration::hours(2);
        
        Ok(LeaseAcquisitionResult {
            lease_id,
            provider: "ssh".to_string(),
            connection_info: serde_json::json!({
                "environmentId": environment_id,
                "type": "ssh",
                "host": ssh_config.host,
                "port": ssh_config.port,
                "username": ssh_config.username,
                "remoteWorkspacePath": ssh_config.remote_workspace_path,
            }),
            expires_at: Some(expires_at),
        })
    }
    
    async fn release_lease(
        &self,
        _lease_id: Uuid,
        _provider_lease_id: Option<String>,
    ) -> Result<(), String> {
        Ok(())
    }
    
    async fn ensure_ready(&self, config: &DriverConfig) -> Result<bool, String> {
        let probe_result = self.probe(config).await?;
        Ok(probe_result.ok)
    }
}
