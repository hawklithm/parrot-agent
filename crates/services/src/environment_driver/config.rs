use models::{ExecutionEnvironment, EnvironmentDriver};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Configuration for different driver types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "driver", rename_all = "lowercase")]
pub enum DriverConfig {
    Local(LocalDriverConfig),
    Ssh(SshDriverConfig),
    Sandbox(SandboxDriverConfig),
    Plugin(PluginDriverConfig),
}

/// Local environment configuration (minimal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalDriverConfig {
    #[serde(default)]
    pub workspace_root: Option<String>,
}

/// SSH environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshDriverConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub remote_workspace_path: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_secret_ref: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_hosts: Option<String>,

    #[serde(default = "default_strict_host_key_checking")]
    pub strict_host_key_checking: bool,
}

fn default_strict_host_key_checking() -> bool {
    true
}

/// Sandbox environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxDriverConfig {
    pub provider: String,
    pub image: String,

    #[serde(default)]
    pub reuse_lease: bool,

    #[serde(default)]
    pub stream_run_logs: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<ResourceLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_mb: Option<u32>,
}

/// Plugin environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDriverConfig {
    pub plugin_key: String,
    pub driver_key: String,
    pub driver_config: JsonValue,
}

/// Resolve driver configuration from an ExecutionEnvironment
pub fn resolve_driver_config(environment: &ExecutionEnvironment) -> Result<DriverConfig, String> {
    match environment.driver {
        EnvironmentDriver::Local => {
            let config: LocalDriverConfig = serde_json::from_value(environment.config.clone())
                .unwrap_or_else(|_| LocalDriverConfig { workspace_root: None });
            Ok(DriverConfig::Local(config))
        }
        EnvironmentDriver::Ssh => {
            let config: SshDriverConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Invalid SSH config: {}", e))?;
            Ok(DriverConfig::Ssh(config))
        }
        EnvironmentDriver::Sandbox => {
            let config: SandboxDriverConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Invalid Sandbox config: {}", e))?;
            Ok(DriverConfig::Sandbox(config))
        }
        EnvironmentDriver::Plugin => {
            let config: PluginDriverConfig = serde_json::from_value(environment.config.clone())
                .map_err(|e| format!("Invalid Plugin config: {}", e))?;
            Ok(DriverConfig::Plugin(config))
        }
    }
}

/// Parse environment driver config from JSON
pub fn parse_environment_driver_config(
    driver: EnvironmentDriver,
    config: &JsonValue,
) -> Result<DriverConfig, String> {
    match driver {
        EnvironmentDriver::Local => {
            let config: LocalDriverConfig = serde_json::from_value(config.clone())
                .unwrap_or_else(|_| LocalDriverConfig { workspace_root: None });
            Ok(DriverConfig::Local(config))
        }
        EnvironmentDriver::Ssh => {
            let config: SshDriverConfig = serde_json::from_value(config.clone())
                .map_err(|e| format!("Invalid SSH config: {}", e))?;
            Ok(DriverConfig::Ssh(config))
        }
        EnvironmentDriver::Sandbox => {
            let config: SandboxDriverConfig = serde_json::from_value(config.clone())
                .map_err(|e| format!("Invalid Sandbox config: {}", e))?;
            Ok(DriverConfig::Sandbox(config))
        }
        EnvironmentDriver::Plugin => {
            let config: PluginDriverConfig = serde_json::from_value(config.clone())
                .map_err(|e| format!("Invalid Plugin config: {}", e))?;
            Ok(DriverConfig::Plugin(config))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_local_config() {
        let config = json!({});
        let result = parse_environment_driver_config(EnvironmentDriver::Local, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_ssh_config() {
        let config = json!({
            "host": "example.com",
            "port": 22,
            "username": "user",
            "remote_workspace_path": "/home/user/workspace"
        });
        let result = parse_environment_driver_config(EnvironmentDriver::Ssh, &config);
        assert!(result.is_ok());

        if let Ok(DriverConfig::Ssh(ssh_config)) = result {
            assert_eq!(ssh_config.host, "example.com");
            assert_eq!(ssh_config.port, 22);
            assert_eq!(ssh_config.username, "user");
            assert!(ssh_config.strict_host_key_checking);
        }
    }

    #[test]
    fn test_parse_sandbox_config() {
        let config = json!({
            "provider": "e2b",
            "image": "ubuntu:22.04",
            "reuse_lease": true
        });
        let result = parse_environment_driver_config(EnvironmentDriver::Sandbox, &config);
        assert!(result.is_ok());

        if let Ok(DriverConfig::Sandbox(sandbox_config)) = result {
            assert_eq!(sandbox_config.provider, "e2b");
            assert_eq!(sandbox_config.image, "ubuntu:22.04");
            assert!(sandbox_config.reuse_lease);
        }
    }
}
