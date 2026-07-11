use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use models::{
    RuntimeLease, EnvironmentLeaseStatus, EnvironmentLeasePolicy,
    CreateRuntimeLeaseInput, UpdateRuntimeLeaseInput,
};
use repositories::{
    EnvironmentRepository, RuntimeLeaseRepository, RepositoryError,
};
use crate::environment_driver::{DriverRegistry, DriverError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LeaseServiceError {
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Driver error: {0}")]
    Driver(#[from] DriverError),

    #[error("Environment not found: {0}")]
    EnvironmentNotFound(Uuid),

    #[error("Lease not found: {0}")]
    LeaseNotFound(Uuid),

    #[error("Environment not available: {0}")]
    EnvironmentNotAvailable(String),

    #[error("Lease expired: {0}")]
    LeaseExpired(Uuid),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Request to acquire a lease
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcquireLeaseRequest {
    pub environment_id: Uuid,
    pub workspace_id: Option<String>,
    pub issue_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub policy: EnvironmentLeasePolicy,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Lease policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeasePolicy {
    pub heartbeat_interval: Duration,
    pub max_ttl: Option<Duration>,
    pub auto_release_on_expire: bool,
}

impl Default for LeasePolicy {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::seconds(30),
            max_ttl: Some(Duration::hours(2)),
            auto_release_on_expire: true,
        }
    }
}

/// Lease service trait
#[async_trait]
pub trait LeaseService: Send + Sync {
    /// Acquire a lease for an environment
    async fn acquire_lease(&self, request: AcquireLeaseRequest) -> Result<RuntimeLease, LeaseServiceError>;

    /// Release a lease
    async fn release_lease(&self, lease_id: Uuid) -> Result<(), LeaseServiceError>;

    /// Refresh heartbeat for a lease
    async fn refresh_heartbeat(&self, lease_id: Uuid) -> Result<RuntimeLease, LeaseServiceError>;

    /// Get active leases for an environment
    async fn get_active_leases(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, LeaseServiceError>;

    /// Mark expired leases
    async fn mark_expired(&self) -> Result<i64, LeaseServiceError>;

    /// Get lease by ID
    async fn get_lease(&self, lease_id: Uuid) -> Result<Option<RuntimeLease>, LeaseServiceError>;
}

/// Default implementation of LeaseService
pub struct DefaultLeaseService<E, L>
where
    E: EnvironmentRepository,
    L: RuntimeLeaseRepository,
{
    environment_repo: Arc<E>,
    lease_repo: Arc<L>,
    driver_registry: Arc<DriverRegistry>,
}

impl<E, L> DefaultLeaseService<E, L>
where
    E: EnvironmentRepository,
    L: RuntimeLeaseRepository,
{
    pub fn new(
        environment_repo: Arc<E>,
        lease_repo: Arc<L>,
        driver_registry: Arc<DriverRegistry>,
    ) -> Self {
        Self {
            environment_repo,
            lease_repo,
            driver_registry,
        }
    }
}

#[async_trait]
impl<E, L> LeaseService for DefaultLeaseService<E, L>
where
    E: EnvironmentRepository + 'static,
    L: RuntimeLeaseRepository + 'static,
{
    async fn acquire_lease(&self, request: AcquireLeaseRequest) -> Result<RuntimeLease, LeaseServiceError> {
        // Get environment
        let environment = self
            .environment_repo
            .get_by_id(request.environment_id)
            .await?
            .ok_or(LeaseServiceError::EnvironmentNotFound(request.environment_id))?;

        // Check if reusable lease exists for this environment
        if request.policy == EnvironmentLeasePolicy::Reusable {
            if let Some(existing_lease) = self.lease_repo.find_reusable_lease(request.environment_id).await? {
                // Return existing reusable lease
                return Ok(existing_lease);
            }
        }

        // Get driver for this environment
        let driver = self.driver_registry.find_driver(environment.driver)?;

        // Probe environment first
        let probe_result = driver.probe(&environment).await?;
        if !probe_result.ok {
            return Err(LeaseServiceError::EnvironmentNotAvailable(
                probe_result.summary,
            ));
        }

        // Acquire lease from driver
        let lease_result = driver
            .acquire_lease(&environment, request.workspace_id.clone(), None)
            .await?;

        // Create lease record
        let input = CreateRuntimeLeaseInput {
            environment_id: request.environment_id,
            agent_id: request.agent_id,
            run_id: request.run_id,
            issue_id: request.issue_id,
            policy: request.policy,
            workspace_id: request.workspace_id,
            lease_metadata: Some(lease_result.connection_info),
            expires_at: request.expires_at.or(lease_result.expires_at),
        };

        let lease = self.lease_repo.create(input).await?;

        Ok(lease)
    }

    async fn release_lease(&self, lease_id: Uuid) -> Result<(), LeaseServiceError> {
        // Get lease
        let lease = self
            .lease_repo
            .get_by_id(lease_id)
            .await?
            .ok_or(LeaseServiceError::LeaseNotFound(lease_id))?;

        // Get environment
        let environment = self
            .environment_repo
            .get_by_id(lease.environment_id)
            .await?
            .ok_or(LeaseServiceError::EnvironmentNotFound(lease.environment_id))?;

        // Get driver
        let driver = self.driver_registry.find_driver(environment.driver)?;

        // Release lease in driver
        driver.release_lease(&environment, lease_id).await?;

        // Update lease status
        self.lease_repo.release(lease_id).await?;

        Ok(())
    }

    async fn refresh_heartbeat(&self, lease_id: Uuid) -> Result<RuntimeLease, LeaseServiceError> {
        // Get lease
        let lease = self
            .lease_repo
            .get_by_id(lease_id)
            .await?
            .ok_or(LeaseServiceError::LeaseNotFound(lease_id))?;

        // Check if expired
        if let Some(expires_at) = lease.expires_at {
            if expires_at < Utc::now() {
                return Err(LeaseServiceError::LeaseExpired(lease_id));
            }
        }

        // Update last_used_at (heartbeat refresh is handled by repository update logic)
        let updated_lease = self
            .lease_repo
            .update(
                lease_id,
                UpdateRuntimeLeaseInput {
                    status: None,
                    cleanup_status: None,
                    cleanup_error: None,
                    released_at: None,
                    lease_metadata: None,
                },
            )
            .await?;

        Ok(updated_lease)
    }

    async fn get_active_leases(&self, environment_id: Uuid) -> Result<Vec<RuntimeLease>, LeaseServiceError> {
        let leases = self
            .lease_repo
            .list_active_by_environment(environment_id)
            .await?;

        Ok(leases)
    }

    async fn mark_expired(&self) -> Result<i64, LeaseServiceError> {
        let count = self.lease_repo.mark_expired().await?;

        Ok(count)
    }

    async fn get_lease(&self, lease_id: Uuid) -> Result<Option<RuntimeLease>, LeaseServiceError> {
        let lease = self.lease_repo.get_by_id(lease_id).await?;

        Ok(lease)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_lease_policy() {
        let policy = LeasePolicy::default();
        assert_eq!(policy.heartbeat_interval, Duration::seconds(30));
        assert_eq!(policy.max_ttl, Some(Duration::hours(2)));
        assert!(policy.auto_release_on_expire);
    }

    #[test]
    fn test_acquire_lease_request_serialization() {
        let request = AcquireLeaseRequest {
            environment_id: Uuid::new_v4(),
            workspace_id: Some("workspace-1".to_string()),
            issue_id: None,
            agent_id: Some(Uuid::new_v4()),
            run_id: None,
            policy: EnvironmentLeasePolicy::Ephemeral,
            expires_at: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: AcquireLeaseRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.environment_id, request.environment_id);
        assert_eq!(deserialized.workspace_id, request.workspace_id);
        assert_eq!(deserialized.policy, request.policy);
    }
}
