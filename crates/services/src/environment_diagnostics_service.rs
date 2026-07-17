use async_trait::async_trait;
use models::{
    AcquireEnvironmentLeaseRequest, EnvironmentDeleteBlastRadius, EnvironmentLease,
    EnvironmentProbeResult,
};
use uuid::Uuid;

use crate::errors::ServiceResult;

/// Service for environment diagnostics and lease management
#[async_trait]
pub trait EnvironmentDiagnosticsService: Send + Sync {
    /// Probe an environment to check connectivity and health
    async fn probe(&self, environment_id: Uuid) -> ServiceResult<EnvironmentProbeResult>;

    /// Acquire a lease for exclusive access to an environment
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        request: AcquireEnvironmentLeaseRequest,
    ) -> ServiceResult<EnvironmentLease>;

    /// Analyze the impact of deleting an environment
    async fn delete_blast_radius(
        &self,
        environment_id: Uuid,
    ) -> ServiceResult<EnvironmentDeleteBlastRadius>;
}

/// Mock implementation for testing
pub struct MockEnvironmentDiagnosticsService;

#[async_trait]
impl EnvironmentDiagnosticsService for MockEnvironmentDiagnosticsService {
    async fn probe(&self, environment_id: Uuid) -> ServiceResult<EnvironmentProbeResult> {
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: models::EnvironmentDriver::Local,
            summary: format!("Environment {} is operational", environment_id),
            details: Some(serde_json::json!({
                "version": "1.0.0",
                "availableCommands": ["bash", "python", "node"],
                "workingDirectory": "/workspace"
            })),
            error: None,
        })
    }

    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        request: AcquireEnvironmentLeaseRequest,
    ) -> ServiceResult<EnvironmentLease> {
        use chrono::Utc;
        use models::LeaseStatus;

        let now = Utc::now();
        Ok(EnvironmentLease {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            environment_id,
            execution_workspace_id: request.execution_workspace_id,
            issue_id: request.issue_id,
            heartbeat_run_id: request.heartbeat_run_id,
            status: LeaseStatus::Active,
            lease_policy: None,
            provider: Some("local".to_string()),
            provider_lease_id: Some(format!("lease-{}", Uuid::new_v4())),
            acquired_at: now,
            last_used_at: Some(now),
            expires_at: Some(now + chrono::Duration::hours(1)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        })
    }

    async fn delete_blast_radius(
        &self,
        environment_id: Uuid,
    ) -> ServiceResult<EnvironmentDeleteBlastRadius> {
        use models::{EnvironmentActiveRuntimeUse, EnvironmentStaticReferences};

        Ok(EnvironmentDeleteBlastRadius {
            environment_id,
            can_delete: true,
            delete_blocked_reasons: vec![],
            blocked_reasons: vec![],
            affected_agents: vec![],
            affected_issues: vec![],
            active_leases: vec![],
            static_references: EnvironmentStaticReferences {
                is_managed_local: false,
                is_instance_default: false,
                agent_default_count: 0,
                execution_workspace_selection_count: 0,
                issue_selection_count: 0,
                project_selection_count: 0,
                secret_binding_count: 0,
            },
            active_runtime_use: EnvironmentActiveRuntimeUse {
                active_lease_count: 0,
                active_custom_image_setup_session_count: 0,
                has_active_runtime_use: false,
            },
        })
    }
}
