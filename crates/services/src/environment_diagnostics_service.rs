use async_trait::async_trait;
use models::{
    AcquireEnvironmentLeaseRequest, EnvironmentDeleteBlastRadius, EnvironmentLease,
    EnvironmentProbeResult,
};
use std::sync::Arc;
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
            driver: "local".to_string(),
            summary: format!("Environment {} is operational", environment_id),
            details: Some(serde_json::json!({
                "version": "1.0.0",
                "availableCommands": ["bash", "python", "node"],
                "workingDirectory": "/workspace"
            })),
        })
    }

    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        request: AcquireEnvironmentLeaseRequest,
    ) -> ServiceResult<EnvironmentLease> {
        use chrono::Utc;
        use models::{EnvironmentLeasePolicy, EnvironmentLeaseStatus};

        let now = Utc::now();
        Ok(EnvironmentLease {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            environment_id,
            execution_workspace_id: request.execution_workspace_id,
            issue_id: request.issue_id,
            heartbeat_run_id: request.heartbeat_run_id,
            status: EnvironmentLeaseStatus::Acquired,
            lease_policy: EnvironmentLeasePolicy::Reuse,
            provider: Some("local".to_string()),
            provider_lease_id: Some(format!("lease-{}", Uuid::new_v4())),
            acquired_at: now,
            last_used_at: now,
            expires_at: Some(now + chrono::Duration::hours(1)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
            metadata: Some(serde_json::json!({
                "adapterType": request.adapter_type,
                "workspaceMode": request.execution_workspace_mode
            })),
            created_at: now,
            updated_at: now,
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
            static_references: EnvironmentStaticReferences {
                is_managed_local: false,
                is_instance_default: false,
                agent_default_count: 3,
                execution_workspace_selection_count: 5,
                issue_selection_count: 2,
                project_selection_count: 1,
                secret_binding_count: 4,
            },
            active_runtime_use: EnvironmentActiveRuntimeUse {
                active_lease_count: 0,
                active_custom_image_setup_session_count: 0,
                has_active_runtime_use: false,
            },
        })
    }
}
