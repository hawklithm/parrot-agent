use async_trait::async_trait;
use models::{
    AcquireEnvironmentLeaseRequest, EnvironmentDeleteBlastRadius, EnvironmentLease,
    EnvironmentProbeResult,
};
use uuid::Uuid;

use crate::errors::ServiceResult;
use sqlx::{PgPool, Row};

pub struct PgEnvironmentDiagnosticsService { pool: PgPool }

impl PgEnvironmentDiagnosticsService { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[async_trait]
impl EnvironmentDiagnosticsService for PgEnvironmentDiagnosticsService {
    async fn probe(&self, environment_id: Uuid) -> ServiceResult<EnvironmentProbeResult> {
        let row = sqlx::query("SELECT driver, status, config, metadata FROM environments WHERE id = $1").bind(environment_id).fetch_optional(&self.pool).await?.ok_or_else(|| crate::errors::ServiceError::NotFound("environment not found".into()))?;
        let driver: String = row.get("driver");
        let driver = match driver.as_str() { "ssh" => models::execution_environment::EnvironmentDriver::Ssh, "sandbox" => models::execution_environment::EnvironmentDriver::Sandbox, "plugin" => models::execution_environment::EnvironmentDriver::Plugin, _ => models::execution_environment::EnvironmentDriver::Local };
        Ok(EnvironmentProbeResult { ok: row.get::<String,_>("status") != "archived", driver, summary: "Environment record is available".into(), details: Some(serde_json::json!({"config": row.get::<serde_json::Value,_>("config"), "metadata": row.get::<Option<serde_json::Value>,_>("metadata")})), error: None })
    }
    async fn acquire_lease(&self, environment_id: Uuid, request: AcquireEnvironmentLeaseRequest) -> ServiceResult<EnvironmentLease> {
        let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM environments WHERE id = $1").bind(environment_id).fetch_optional(&self.pool).await?.ok_or_else(|| crate::errors::ServiceError::NotFound("environment not found".into()))?;
        let row = sqlx::query("INSERT INTO environment_leases (company_id, environment_id, execution_workspace_id, issue_id, heartbeat_run_id, status, lease_policy, provider, expires_at) VALUES ($1,$2,$3,$4,$5,'active','ephemeral',(SELECT driver FROM environments WHERE id=$2),NOW() + INTERVAL '1 hour') RETURNING *").bind(company_id).bind(environment_id).bind(request.execution_workspace_id).bind(request.issue_id).bind(request.heartbeat_run_id).fetch_one(&self.pool).await?;
        Ok(EnvironmentLease { id:row.get("id"), company_id, environment_id, execution_workspace_id:row.get("execution_workspace_id"), issue_id:row.get("issue_id"), heartbeat_run_id:row.get("heartbeat_run_id"), status:models::environment::LeaseStatus::Active, lease_policy:Some(serde_json::json!("ephemeral")), provider:row.get("provider"), provider_lease_id:None, acquired_at:row.get("acquired_at"), last_used_at:row.get("last_used_at"), expires_at:row.get("expires_at"), released_at:None, failure_reason:None, cleanup_status:None })
    }
    async fn delete_blast_radius(&self, environment_id: Uuid) -> ServiceResult<EnvironmentDeleteBlastRadius> {
        let company_id: Uuid = sqlx::query_scalar("SELECT company_id FROM environments WHERE id = $1").bind(environment_id).fetch_optional(&self.pool).await?.ok_or_else(|| crate::errors::ServiceError::NotFound("environment not found".into()))?;
        let active_leases: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM environment_leases WHERE environment_id=$1 AND status IN ('active','acquired')").bind(environment_id).fetch_all(&self.pool).await?;
        let lease_count = active_leases.len() as i32;
        let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE company_id=$1 AND metadata->>'environmentId'=$2").bind(company_id).bind(environment_id.to_string()).fetch_one(&self.pool).await.unwrap_or(0);
        let issue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE company_id=$1 AND metadata->>'environmentId'=$2").bind(company_id).bind(environment_id.to_string()).fetch_one(&self.pool).await.unwrap_or(0);
        Ok(EnvironmentDeleteBlastRadius { environment_id, can_delete: active_leases.is_empty(), delete_blocked_reasons: vec![], blocked_reasons: if active_leases.is_empty(){vec![]}else{vec!["active environment leases exist".into()]}, affected_agents: vec![], affected_issues: vec![], active_leases, static_references: models::EnvironmentStaticReferences { is_managed_local:false, is_instance_default:false, agent_default_count:agent_count as i32, execution_workspace_selection_count:0, issue_selection_count:issue_count as i32, project_selection_count:0, secret_binding_count:0 }, active_runtime_use: models::EnvironmentActiveRuntimeUse { active_lease_count:lease_count, active_custom_image_setup_session_count:0, has_active_runtime_use: lease_count > 0 } })
    }
}

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
