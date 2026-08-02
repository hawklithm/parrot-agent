use async_trait::async_trait;
use super::{EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult, DriverError};
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// SSH environment driver implementation
pub struct SshDriver;

impl SshDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentDriverTrait for SshDriver {
    async fn probe(&self, environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        let config = ssh_config(environment)?;
        let target = format!("{}@{}", config.username, config.host);
        let output = timeout(Duration::from_secs(10), Command::new("ssh")
            .args(["-p", &config.port.to_string(), "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &target, "true"])
            .output())
            .await
            .map_err(|_| DriverError::ConnectionError("SSH probe timed out".to_string()))?
            .map_err(|e| DriverError::ConnectionError(format!("cannot run ssh: {e}")))?;
        if !output.status.success() {
            return Err(DriverError::ConnectionError(String::from_utf8_lossy(&output.stderr).trim().to_string()));
        }
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: EnvironmentDriver::Ssh,
            summary: format!("SSH environment accessible at {}", target),
        })
    }

    async fn acquire_lease(
        &self,
        environment: &ExecutionEnvironment,
        _workspace_id: Option<String>,
        _metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        self.probe(environment).await.map_err(|e| DriverError::LeaseAcquisitionFailed(e.to_string()))?;
        Err(DriverError::LeaseAcquisitionFailed("SSH lease lifecycle is managed by the runtime service; the legacy driver cannot allocate leases".to_string()))
    }

    async fn release_lease(
        &self,
        _environment: &ExecutionEnvironment,
        _lease_id: Uuid,
    ) -> Result<(), DriverError> {
        Err(DriverError::ConnectionError("SSH lease lifecycle is managed by the runtime service".to_string()))
    }

    async fn ensure_ready(&self, environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        self.probe(environment).await.map(|_| ())
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Ssh
    }
}

fn ssh_config(environment: &ExecutionEnvironment) -> Result<super::SshDriverConfig, DriverError> {
    match super::resolve_driver_config(environment).map_err(DriverError::ConfigError)? {
        super::DriverConfig::Ssh(config) => Ok(config),
        _ => Err(DriverError::ConfigError("SSH driver received non-SSH configuration".to_string())),
    }
}
