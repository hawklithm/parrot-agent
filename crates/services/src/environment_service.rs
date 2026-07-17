use async_trait::async_trait;
use repositories::EnvironmentRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use models::execution_environment::{ExecutionEnvironment as Environment, EnvironmentStatus, EnvironmentDriver, CreateEnvironmentInput, UpdateEnvironmentInput};
use crate::errors::ServiceError;

/// Input for leasing an environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseEnvironmentInput {
    pub environment_id: Uuid,
    pub leased_by_agent_id: Uuid,
    pub lease_duration_seconds: i64,
}

/// Environment Service trait
#[async_trait]
pub trait EnvironmentService: Send + Sync {
    /// Create a new environment
    async fn create(&self, company_id: Uuid, input: CreateEnvironmentInput) -> Result<Environment, ServiceError>;

    /// Get environment by ID
    async fn get(&self, id: Uuid) -> Result<Environment, ServiceError>;

    /// Get environment by name
    async fn get_by_name(&self, name: &str) -> Result<Environment, ServiceError>;

    /// Update environment
    async fn update(&self, id: Uuid, input: UpdateEnvironmentInput) -> Result<Environment, ServiceError>;

    /// Delete environment (soft delete - archived)
    async fn delete(&self, id: Uuid) -> Result<(), ServiceError>;

    /// List environments by status
    async fn list_by_status(&self, status: EnvironmentStatus) -> Result<Vec<Environment>, ServiceError>;

    /// List all environments
    async fn list_all(&self) -> Result<Vec<Environment>, ServiceError>;

    /// Validate environment config based on driver
    fn validate_config(&self, driver: EnvironmentDriver, config: &serde_json::Value) -> Result<(), ServiceError>;

    // --- P1: Environment 补齐 (E11-E16) ---

    /// E11: Get environment capabilities
    async fn get_capabilities(&self, company_id: Uuid) -> Result<serde_json::Value, ServiceError>;

    /// E12: Probe environment configuration
    async fn probe_config(&self, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, ServiceError>;

    /// E16: Get delete blast radius
    async fn get_delete_blast_radius(&self, id: Uuid) -> Result<serde_json::Value, ServiceError>;
}

/// Default Environment Service Implementation
pub struct DefaultEnvironmentService {
    environment_repo: Arc<dyn EnvironmentRepository>,
}

impl DefaultEnvironmentService {
    pub fn new(environment_repo: Arc<dyn EnvironmentRepository>) -> Self {
        Self { environment_repo }
    }
}

#[async_trait]
impl EnvironmentService for DefaultEnvironmentService {
    async fn create(&self, _company_id: Uuid, input: CreateEnvironmentInput) -> Result<Environment, ServiceError> {
        // Validate config
        self.validate_config(input.driver.clone(), input.config.as_ref().unwrap_or(&serde_json::Value::Null))?;

        self.environment_repo
            .create(input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to create environment: {}", e)))
    }

    async fn get(&self, id: Uuid) -> Result<Environment, ServiceError> {
        self.environment_repo
            .get_by_id(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get environment: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Environment {} not found", id)))
    }

    async fn get_by_name(&self, name: &str) -> Result<Environment, ServiceError> {
        self.environment_repo
            .get_by_name(name)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get environment by name: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Environment '{}' not found", name)))
    }

    async fn update(&self, id: Uuid, input: UpdateEnvironmentInput) -> Result<Environment, ServiceError> {
        // Validate config if provided
        if let Some(ref config) = input.config {
            let environment = self.get(id).await?;
            self.validate_config(environment.driver, config)?;
        }

        self.environment_repo
            .update(id, input)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to update environment: {}", e)))
    }

    async fn delete(&self, id: Uuid) -> Result<(), ServiceError> {
        // Check blast radius before deleting
        let blast_radius = self.environment_repo
            .get_delete_blast_radius(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to check delete blast radius: {}", e)))?;

        if !blast_radius.can_delete {
            return Err(ServiceError::Conflict(format!(
                "Cannot delete environment: {}",
                blast_radius.blocked_reasons.join(", ")
            )));
        }

        self.environment_repo
            .delete(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to delete environment: {}", e)))
    }

    async fn list_by_status(&self, status: EnvironmentStatus) -> Result<Vec<Environment>, ServiceError> {
        self.environment_repo
            .list_by_status(status)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list environments by status: {}", e)))
    }

    async fn list_all(&self) -> Result<Vec<Environment>, ServiceError> {
        self.environment_repo
            .list_all()
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to list all environments: {}", e)))
    }

    fn validate_config(&self, driver: EnvironmentDriver, config: &serde_json::Value) -> Result<(), ServiceError> {
        match driver {
            EnvironmentDriver::Local => {
                // Local environments - minimal validation
                Ok(())
            }
            EnvironmentDriver::Ssh => {
                // SSH environments need host, username
                if !config.get("host").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("SSH environment requires 'host' in config".to_string()));
                }
                if !config.get("username").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("SSH environment requires 'username' in config".to_string()));
                }
                if !config.get("remoteWorkspacePath").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("SSH environment requires 'remoteWorkspacePath' in config".to_string()));
                }
                Ok(())
            }
            EnvironmentDriver::Sandbox => {
                // Sandbox environments need provider and image
                if !config.get("provider").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("Sandbox environment requires 'provider' in config".to_string()));
                }
                if !config.get("image").and_then(|v| v.as_str()).is_some() {
                    return Err(ServiceError::InvalidInput("Sandbox environment requires 'image' in config".to_string()));
                }
                Ok(())
            }
            EnvironmentDriver::Plugin => {
                // Plugin environments - custom validation based on plugin type
                // For now, just check that config is not empty
                if config.is_null() || (config.is_object() && config.as_object().unwrap().is_empty()) {
                    return Err(ServiceError::InvalidInput("Plugin environment requires non-empty config".to_string()));
                }
                Ok(())
            }
        }
    }

    // --- P1: Environment 补齐 Mock 实现 ---

    async fn get_capabilities(&self, company_id: Uuid) -> Result<serde_json::Value, ServiceError> {
        Ok(serde_json::json!({
            "companyId": company_id,
            "environments": [],
            "totalEnvironments": 0,
            "supportedDrivers": ["local", "ssh", "sandbox", "plugin"],
        }))
    }

    async fn probe_config(&self, company_id: Uuid, input: serde_json::Value) -> Result<serde_json::Value, ServiceError> {
        Ok(serde_json::json!({
            "companyId": company_id,
            "probeConfig": input,
            "status": "ok",
            "compatible": true,
        }))
    }

    async fn get_delete_blast_radius(&self, id: Uuid) -> Result<serde_json::Value, ServiceError> {
        // Check repo for actual blast radius
        let blast_radius = self.environment_repo
            .get_delete_blast_radius(id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to check delete blast radius: {}", e)))?;

        Ok(serde_json::json!({
            "environmentId": id,
            "canDelete": blast_radius.can_delete,
            "blockedReasons": blast_radius.blocked_reasons,
            "activeLeases": blast_radius.active_leases,
            "linkedAgents": blast_radius.affected_agents,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ssh_config() {
        let service = DefaultEnvironmentService::new(Arc::new(MockEnvironmentRepository::new()));

        // Valid SSH config
        let valid_ssh = serde_json::json!({
            "host": "example.com",
            "username": "agent",
            "remoteWorkspacePath": "/home/agent/workspace",
            "port": 22
        });
        assert!(service.validate_config(EnvironmentDriver::Ssh, &valid_ssh).is_ok());

        // Missing host
        let missing_host = serde_json::json!({
            "username": "agent",
            "remoteWorkspacePath": "/home/agent/workspace"
        });
        assert!(service.validate_config(EnvironmentDriver::Ssh, &missing_host).is_err());

        // Missing username
        let missing_username = serde_json::json!({
            "host": "example.com",
            "remoteWorkspacePath": "/home/agent/workspace"
        });
        assert!(service.validate_config(EnvironmentDriver::Ssh, &missing_username).is_err());
    }

    #[test]
    fn test_validate_sandbox_config() {
        let service = DefaultEnvironmentService::new(Arc::new(MockEnvironmentRepository::new()));

        // Valid sandbox config
        let valid_sandbox = serde_json::json!({
            "provider": "e2b",
            "image": "ubuntu:22.04",
            "reuseLease": true
        });
        assert!(service.validate_config(EnvironmentDriver::Sandbox, &valid_sandbox).is_ok());

        // Missing provider
        let missing_provider = serde_json::json!({
            "image": "ubuntu:22.04"
        });
        assert!(service.validate_config(EnvironmentDriver::Sandbox, &missing_provider).is_err());

        // Missing image
        let missing_image = serde_json::json!({
            "provider": "e2b"
        });
        assert!(service.validate_config(EnvironmentDriver::Sandbox, &missing_image).is_err());
    }

    #[test]
    fn test_validate_local_config() {
        let service = DefaultEnvironmentService::new(Arc::new(MockEnvironmentRepository::new()));

        // Local config - always valid
        let empty_config = serde_json::json!({});
        assert!(service.validate_config(EnvironmentDriver::Local, &empty_config).is_ok());
    }
}
