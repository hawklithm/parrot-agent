use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use models::{EnvironmentLease, LeaseStatus};

/// Acquire lease request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquireLeaseRequest {
    pub environment_id: Uuid,
    pub execution_workspace_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub heartbeat_run_id: Option<Uuid>,
}

/// Lease policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeasePolicy {
    pub heartbeat_interval_secs: u64,
    pub max_ttl_secs: u64,
    pub auto_release_on_expire: bool,
}

impl Default for LeasePolicy {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 30,
            max_ttl_secs: 3600,
            auto_release_on_expire: true,
        }
    }
}

/// Lease state machine
pub struct LeaseStateMachine;

impl LeaseStateMachine {
    /// Check if transition from current_status to target_status is valid
    pub fn can_transition(current_status: &LeaseStatus, target_status: &LeaseStatus) -> bool {
        match (current_status, target_status) {
            (LeaseStatus::Active, LeaseStatus::Released) => true,
            (LeaseStatus::Active, LeaseStatus::Expired) => true,
            (LeaseStatus::Active, LeaseStatus::Failed) => true,
            (LeaseStatus::Released, _) => false,
            (LeaseStatus::Expired, _) => false,
            (LeaseStatus::Failed, _) => false,
            _ => false,
        }
    }
  /// Check if lease is expired
    pub fn is_expired(lease: &EnvironmentLease, policy: &LeasePolicy) -> bool {
        if let Some(expires_at) = lease.expires_at {
            return chrono::Utc::now() > expires_at;
        }
        
        if let Some(last_used_at) = lease.last_used_at {
            let timeout_duration = chrono::Duration::seconds(policy.heartbeat_interval_secs as i64 * 3);
            return chrono::Utc::now() > last_used_at + timeout_duration;
        }
        
        false
    }
    
    /// Check if lease should be released
    pub fn should_release(lease: &EnvironmentLease, policy: &LeasePolicy) -> bool {
        matches!(lease.status, LeaseStatus::Active) 
            && Self::is_expired(lease, policy) 
            && policy.auto_release_on_expire
    }
}

/// Lease service trait
#[async_trait]
pub trait LeaseService: Send + Sync {
    /// Acquire a new lease
    async fn acquire_lease(
        &self,
        company_id: Uuid,
        request: AcquireLeaseRequest,
    ) -> Result<EnvironmentLease, String>;
    
    /// Release a lease
    async fn release_lease(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<EnvironmentLease, String>;
    
    /// Refresh lease heartbeat
    async fn refresh_heartbeat(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<EnvironmentLease, String>;
    
    /// Get active leases for a company
    async fn get_active_leases(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<EnvironmentLease>, String>;
    
    /// Get lease by ID
    async fn get_lease(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<EnvironmentLease>, String>;
}

/// Mock lease service implementation
pub struct MockLeaseService;

impl MockLeaseService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LeaseService for MockLeaseService {
    async fn acquire_lease(
        &self,
        company_id: Uuid,
        request: AcquireLeaseRequest,
    ) -> Result<EnvironmentLease, String> {
        let lease_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let policy = LeasePolicy::default();
        
        Ok(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: request.environment_id,
            execution_workspace_id: request.execution_workspace_id,
            issue_id: request.issue_id,
            heartbeat_run_id: request.heartbeat_run_id,
            status: LeaseStatus::Active,
            lease_policy: Some(serde_json::to_value(&policy).unwrap()),
            provider: Some("mock".to_string()),
            provider_lease_id: Some(format!("mock-{}", lease_id)),
            acquired_at: now,
            last_used_at: Some(now),
            expires_at: Some(now + chrono::Duration::seconds(policy.max_ttl_secs as i64)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        })
    }
    
    async fn release_lease(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<EnvironmentLease, String> {
        let now = chrono::Utc::now();
        let policy = LeasePolicy::default();
        
        Ok(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: Uuid::new_v4(),
            execution_workspace_id: None,
            issue_id: None,
            heartbeat_run_id: None,
            status: LeaseStatus::Released,
            lease_policy: Some(serde_json::to_value(&policy).unwrap()),
            provider: Some("mock".to_string()),
            provider_lease_id: Some(format!("mock-{}", lease_id)),
            acquired_at: now - chrono::Duration::hours(1),
            last_used_at: Some(now),
            expires_at: Some(now + chrono::Duration::hours(1)),
            released_at: Some(now),
            failure_reason: None,
            cleanup_status: Some("cleaned".to_string()),
        })
    }
    
    async fn refresh_heartbeat(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<EnvironmentLease, String> {
        let now = chrono::Utc::now();
        let policy = LeasePolicy::default();
        
        Ok(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: Uuid::new_v4(),
            execution_workspace_id: None,
            issue_id: None,
            heartbeat_run_id: None,
            status: LeaseStatus::Active,
            lease_policy: Some(serde_json::to_value(&policy).unwrap()),
            provider: Some("mock".to_string()),
            provider_lease_id: Some(format!("mock-{}", lease_id)),
            acquired_at: now - chrono::Duration::minutes(30),
            last_used_at: Some(now),
            expires_at: Some(now + chrono::Duration::minutes(30)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        })
    }
    
    async fn get_active_leases(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<EnvironmentLease>, String> {
        let now = chrono::Utc::now();
        let policy = LeasePolicy::default();
        
        Ok(vec![
            EnvironmentLease {
                id: Uuid::new_v4(),
                company_id,
                environment_id: Uuid::new_v4(),
                execution_workspace_id: None,
                issue_id: None,
                heartbeat_run_id: None,
                status: LeaseStatus::Active,
                lease_policy: Some(serde_json::to_value(&policy).unwrap()),
                provider: Some("mock".to_string()),
                provider_lease_id: Some(format!("mock-{}", Uuid::new_v4())),
                acquired_at: now - chrono::Duration::minutes(10),
                last_used_at: Some(now),
                expires_at: Some(now + chrono::Duration::minutes(50)),
                released_at: None,
                failure_reason: None,
                cleanup_status: None,
            },
        ])
    }
    
    async fn get_lease(
        &self,
        lease_id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<EnvironmentLease>, String> {
        let now = chrono::Utc::now();
        let policy = LeasePolicy::default();
        
        Ok(Some(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: Uuid::new_v4(),
            execution_workspace_id: None,
            issue_id: None,
            heartbeat_run_id: None,
            status: LeaseStatus::Active,
            lease_policy: Some(serde_json::to_value(&policy).unwrap()),
            provider: Some("mock".to_string()),
            provider_lease_id: Some(format!("mock-{}", lease_id)),
            acquired_at: now - chrono::Duration::minutes(15),
            last_used_at: Some(now - chrono::Duration::minutes(2)),
            expires_at: Some(now + chrono::Duration::minutes(45)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        }))
    }
}
