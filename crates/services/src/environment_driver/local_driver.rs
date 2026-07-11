use async_trait::async_trait;
use super::{EnvironmentDriverTrait, EnvironmentProbeResult, LeaseAcquisitionResult, DriverError};
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Local environment driver implementation
pub struct LocalDriver;

impl LocalDriver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentDriverTrait for LocalDriver {
    async fn probe(&self, _environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError> {
        // TODO: Implement local environment probe (check git, node, etc.)
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: EnvironmentDriver::Local,
            summary: "Local environment ready".to_string(),
            details: None,
        })
    }

    async fn acquire_lease(
        &self,
        _environment: &ExecutionEnvironment,
        workspace_id: Option<String>,
        _metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError> {
        // TODO: Implement local lease acquisition
        Ok(LeaseAcquisitionResult {
            lease_id: Uuid::new_v4(),
            provider: "local".to_string(),
            connection_info: serde_json::json!({
                "type": "local",
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
        // TODO: Implement local lease release
        Ok(())
    }

    async fn ensure_ready(&self, _environment: &ExecutionEnvironment) -> Result<(), DriverError> {
        // TODO: Implement local environment readiness check
        Ok(())
    }

    fn driver_type(&self) -> EnvironmentDriver {
        EnvironmentDriver::Local
    }
}
