use async_trait::async_trait;
use super::{EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult, DriverError};
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// SSH environment driver implementation
pub struct SshDriver;

impl SshDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentDriverTrait for SshDriver {
    async fn probe(&self, _environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        // TODO: Implement SSH connection probe
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: EnvironmentDriver::Ssh,
            summary: "SSH environment accessible".to_string(),
            details: None,
        })
    }

    async fn acquire_lease(
        &self,
        _environment: &ExecutionEnvironment,
        workspace_id: Option<String>,
        _metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        // TODO: Implement SSH lease acquisition
        Ok(LeaseAcquisitionResult {
            lease_id: Uuid::new_v4(),
            provider: "ssh".to_string(),
            connection_info: serde_json::json!({
                "type": "ssh",
                "workspace_id": workspace_id,
            }),
            expires_at: None,
        })
    }

    async fn release_lease(
        &self,
        _environment: &ExecutionEnvironment,
        _lease_id: Uuid,
    ) -> Result<(), DriverError> {
        // TODO: Implement SSH lease release
        Ok(())
    }

    async fn ensure_ready(&self, _environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        // TODO: Implement SSH environment readiness check
        Ok(())
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Ssh
    }
}
