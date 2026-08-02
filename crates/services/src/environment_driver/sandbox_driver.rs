use async_trait::async_trait;
use super::{EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult, DriverError};
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Sandbox environment driver implementation
pub struct SandboxDriver;

impl SandboxDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentDriverTrait for SandboxDriver {
    async fn probe(&self, _environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        let config = sandbox_config(_environment)?;
        Err(DriverError::ConnectionError(format!("sandbox provider '{}' is not registered; configure a plugin provider before using the legacy driver", config.provider)))
    }

    async fn acquire_lease(
        &self,
        _environment: &ExecutionEnvironment,
        workspace_id: Option<String>,
        _metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        let config = sandbox_config(environment)?;
        Err(DriverError::LeaseAcquisitionFailed(format!("sandbox provider '{}' is not registered; no instance was created", config.provider)))
    }

    async fn release_lease(
        &self,
        _environment: &ExecutionEnvironment,
        _lease_id: Uuid,
    ) -> Result<(), DriverError> {
        Err(DriverError::ConnectionError("sandbox lease lifecycle is managed by the registered plugin provider".to_string()))
    }

    async fn ensure_ready(&self, environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        self.probe(environment).await.map(|_| ())
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Sandbox
    }
}

fn sandbox_config(environment: &ExecutionEnvironment) -> Result<super::SandboxDriverConfig, DriverError> {
    match super::resolve_driver_config(environment).map_err(DriverError::ConfigError)? {
        super::DriverConfig::Sandbox(config) => Ok(config),
        _ => Err(DriverError::ConfigError("sandbox driver received non-sandbox configuration".to_string())),
    }
}
