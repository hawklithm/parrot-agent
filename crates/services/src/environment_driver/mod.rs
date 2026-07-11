pub mod config;
pub mod registry;

pub use config::*;
pub use registry::DriverRegistry;

use async_trait::async_trait;
use models::{ExecutionEnvironment, EnvironmentDriver};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("Driver not found: {0:?}")]
    DriverNotFound(EnvironmentDriver),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Probe failed: {0}")]
    ProbeFailed(String),

    #[error("Lease acquisition failed: {0}")]
    LeaseAcquisitionFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentProbeResult {
    pub ok: bool,
    pub driver: EnvironmentDriver,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseAcquisitionResult {
    pub lease_id: Uuid,
    pub provider: String,
    pub connection_info: JsonValue,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait EnvironmentDriverTrait: Send + Sync {
    async fn probe(&self, environment: &ExecutionEnvironment) -> Result<EnvironmentProbeResult, DriverError>;

    async fn acquire_lease(
        &self,
        environment: &ExecutionEnvironment,
        workspace_id: Option<String>,
        metadata: Option<JsonValue>,
    ) -> Result<LeaseAcquisitionResult, DriverError>;

    async fn release_lease(
        &self,
        environment: &ExecutionEnvironment,
        lease_id: Uuid,
    ) -> Result<(), DriverError>;

    async fn ensure_ready(&self, environment: &ExecutionEnvironment) -> Result<(), DriverError>;

    fn driver_type(&self) -> EnvironmentDriver;
}
