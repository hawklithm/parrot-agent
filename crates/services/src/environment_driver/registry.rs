use super::{EnvironmentDriverTrait, DriverError};
use models::EnvironmentDriver;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for environment drivers
pub struct DriverRegistry {
    drivers: HashMap<EnvironmentDriver, Arc<dyn EnvironmentDriverTrait>>,
}

impl DriverRegistry {
    /// Create a new empty driver registry
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }

    /// Register a driver implementation
    pub fn register(&mut self, driver: Arc<dyn EnvironmentDriverTrait>) {
        let driver_type = driver.driver_type();
        self.drivers.insert(driver_type, driver);
    }

    /// Find a driver by type
    pub fn find_driver(&self, driver_type: EnvironmentDriver) -> Result<Arc<dyn EnvironmentDriverTrait>, DriverError> {
        self.drivers
            .get(&driver_type)
            .cloned()
            .ok_or(DriverError::DriverNotFound(driver_type))
    }

    /// Check if a driver is registered
    pub fn has_driver(&self, driver_type: EnvironmentDriver) -> bool {
        self.drivers.contains_key(&driver_type)
    }

    /// Get all registered driver types
    pub fn registered_drivers(&self) -> Vec<EnvironmentDriver> {
        self.drivers.keys().copied().collect()
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use models::ExecutionEnvironment;
    use uuid::Uuid;

    struct MockDriver {
        driver_type: EnvironmentDriver,
    }

    #[async_trait]
    impl EnvironmentDriverTrait for MockDriver {
        async fn probe(&self, _environment: &ExecutionEnvironment) -> Result<super::EnvironmentProbeResult, DriverError> {
            Ok(super::EnvironmentProbeResult {
                ok: true,
                driver: self.driver_type,
                summary: "Mock probe successful".to_string(),
                details: None,
            })
        }

        async fn acquire_lease(
            &self,
            _environment: &ExecutionEnvironment,
            _workspace_id: Option<String>,
            _metadata: Option<serde_json::Value>,
        ) -> Result<super::LeaseAcquisitionResult, DriverError> {
            Ok(super::LeaseAcquisitionResult {
                lease_id: Uuid::new_v4(),
                provider: "mock".to_string(),
                connection_info: serde_json::json!({}),
                expires_at: None,
            })
        }

        async fn release_lease(&self, _environment: &ExecutionEnvironment, _lease_id: Uuid) -> Result<(), DriverError> {
            Ok(())
        }

        async fn ensure_ready(&self, _environment: &ExecutionEnvironment) -> Result<(), DriverError> {
            Ok(())
        }

        fn driver_type(&self) -> EnvironmentDriver {
            self.driver_type
        }
    }

    #[test]
    fn test_registry_register_and_find() {
        let mut registry = DriverRegistry::new();
        let driver = Arc::new(MockDriver {
            driver_type: EnvironmentDriver::Local,
        });

        registry.register(driver.clone());

        assert!(registry.has_driver(EnvironmentDriver::Local));
        assert!(registry.find_driver(EnvironmentDriver::Local).is_ok());
        assert!(registry.find_driver(EnvironmentDriver::Ssh).is_err());
    }
}
