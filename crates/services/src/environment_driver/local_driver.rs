use async_trait::async_trait;
use super::{EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult, DriverError};
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde_json::Value as JsonValue;
use uuid::Uuid;
use std::path::PathBuf;

/// Local environment driver implementation
pub struct LocalDriver;

impl LocalDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentDriverTrait for LocalDriver {
    async fn probe(&self, environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        let root = workspace_root(environment)?;
        if !root.is_dir() {
            return Err(DriverError::ProbeFailed(format!("local workspace does not exist: {}", root.display())));
        }
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: EnvironmentDriver::Local,
            summary: format!("local workspace available at {}", root.display()),
        })
    }

    async fn acquire_lease(
        &self,
        environment: &ExecutionEnvironment,
        workspace_id: Option<String>,
        _metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        let root = workspace_root(environment)?;
        if !root.is_dir() {
            return Err(DriverError::LeaseAcquisitionFailed(format!("local workspace does not exist: {}", root.display())));
        }
        Ok(LeaseAcquisitionResult {
            lease_id: Uuid::new_v4(),
            provider: "local".to_string(),
            connection_info: serde_json::json!({
                "type": "local",
                "workspace_id": workspace_id,
                "workspace_root": root,
            }),
            expires_at: None,
        })
    }

    async fn release_lease(
        &self,
        _environment: &ExecutionEnvironment,
        _lease_id: Uuid,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    async fn ensure_ready(&self, environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        workspace_root(environment)
            .map(|_| ())
            .map_err(|error| DriverError::ConnectionError(error.to_string()))
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Local
    }
}

fn workspace_root(environment: &ExecutionEnvironment) -> Result<PathBuf, DriverError> {
    let config = super::resolve_driver_config(environment)
        .map_err(DriverError::ConfigError)?;
    let super::DriverConfig::Local(config) = config else {
        return Err(DriverError::ConfigError("local driver received non-local configuration".to_string()));
    };
    let root = config.workspace_root
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().map_err(|e| DriverError::Internal(e.to_string()))?);
    std::fs::canonicalize(&root)
        .map_err(|e| DriverError::ProbeFailed(format!("cannot access local workspace {}: {e}", root.display())))
}
