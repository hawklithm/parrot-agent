//! Mock implementations for environment-related services.
//!
//! These provide in-memory stub behavior so the API routes can compile and run
//! without a backing datastore. They mirror the method signatures expected by
//! `crates/api/src/routes/environments.rs`.

use chrono::Utc;
use models::environment::{
    Environment, EnvironmentDriver, EnvironmentLease, EnvironmentStatus, LeaseStatus,
};
use models::execution_environment::{
    CreateEnvironmentInput, EnvironmentProbeResult, ExecutionWorkspace, UpdateEnvironmentInput,
    WorkspaceMode, WorkspaceStatus, WorkspaceStrategyType,
};
use std::sync::Arc;
use uuid::Uuid;

type Result<T> = std::result::Result<T, String>;

/// Mock environment service (in-memory, no persistence).
#[derive(Debug, Default, Clone)]
pub struct MockEnvironmentService;

impl MockEnvironmentService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    fn sample_environment(company_id: Uuid) -> Environment {
        Environment {
            id: Uuid::new_v4(),
            company_id,
            name: "mock-environment".to_string(),
            description: Some("Mock environment".to_string()),
            driver: EnvironmentDriver::Local,
            status: EnvironmentStatus::Active,
            config: serde_json::json!({}),
            env_vars: serde_json::json!({}),
            metadata: Some(serde_json::json!({})),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl MockEnvironmentService {
    pub async fn list_environments(&self, company_id: Uuid) -> Result<Vec<Environment>> {
        Ok(vec![Self::sample_environment(company_id)])
    }

    pub async fn get_environment(
        &self,
        _id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<Environment>> {
        Ok(Some(Self::sample_environment(company_id)))
    }

    pub async fn create_environment(
        &self,
        company_id: Uuid,
        input: CreateEnvironmentInput,
    ) -> Result<Environment> {
        Ok(Environment {
            id: Uuid::new_v4(),
            company_id,
            name: input.name,
            description: input.description,
            driver: EnvironmentDriver::Local,
            status: EnvironmentStatus::Active,
            config: input.config.unwrap_or_else(|| serde_json::json!({})),
            env_vars: input.env_vars.unwrap_or_else(|| serde_json::json!({})),
            metadata: input.metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    pub async fn update_environment(
        &self,
        id: Uuid,
        company_id: Uuid,
        input: UpdateEnvironmentInput,
    ) -> Result<Environment> {
        Ok(Environment {
            id,
            company_id,
            name: input.name.unwrap_or_else(|| "mock-environment".to_string()),
            description: input.description,
            driver: EnvironmentDriver::Local,
            status: EnvironmentStatus::Active,
            config: input.config.unwrap_or_else(|| serde_json::json!({})),
            env_vars: input.env_vars.unwrap_or_else(|| serde_json::json!({})),
            metadata: input.metadata,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    pub async fn delete_environment(&self, _id: Uuid, _company_id: Uuid) -> Result<()> {
        Ok(())
    }

    pub async fn probe_environment(
        &self,
        _id: Uuid,
        _company_id: Uuid,
    ) -> Result<EnvironmentProbeResult> {
        Ok(EnvironmentProbeResult {
            ok: true,
            driver: models::execution_environment::EnvironmentDriver::Local,
            summary: "Mock environment probe succeeded".to_string(),
            details: Some(serde_json::json!({})),
            error: None,
        })
    }
}

/// Mock environment lease service (in-memory, no persistence).
#[derive(Debug, Default, Clone)]
pub struct MockEnvironmentLeaseService;

impl MockEnvironmentLeaseService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    fn sample_lease(
        environment_id: Uuid,
        company_id: Uuid,
        issue_id: Option<Uuid>,
        heartbeat_run_id: Option<Uuid>,
    ) -> EnvironmentLease {
        EnvironmentLease {
            id: Uuid::new_v4(),
            company_id,
            environment_id,
            execution_workspace_id: None,
            issue_id,
            heartbeat_run_id,
            status: LeaseStatus::Active,
            lease_policy: None,
            provider: Some("mock".to_string()),
            provider_lease_id: None,
            acquired_at: Utc::now(),
            last_used_at: None,
            expires_at: None,
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        }
    }
}

impl MockEnvironmentLeaseService {
    pub async fn acquire_lease(
        &self,
        environment_id: Uuid,
        company_id: Uuid,
        issue_id: Option<Uuid>,
        heartbeat_run_id: Option<Uuid>,
    ) -> Result<EnvironmentLease> {
        Ok(Self::sample_lease(
            environment_id,
            company_id,
            issue_id,
            heartbeat_run_id,
        ))
    }

    pub async fn release_lease(&self, lease_id: Uuid, company_id: Uuid) -> Result<EnvironmentLease> {
        Ok(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: Uuid::new_v4(),
            execution_workspace_id: None,
            issue_id: None,
            heartbeat_run_id: None,
            status: LeaseStatus::Released,
            lease_policy: None,
            provider: Some("mock".to_string()),
            provider_lease_id: None,
            acquired_at: Utc::now(),
            last_used_at: None,
            expires_at: None,
            released_at: Some(Utc::now()),
            failure_reason: None,
            cleanup_status: None,
        })
    }

    pub async fn list_active_leases(&self, company_id: Uuid) -> Result<Vec<EnvironmentLease>> {
        Ok(vec![Self::sample_lease(
            Uuid::new_v4(),
            company_id,
            None,
            None,
        )])
    }
}

/// Mock execution workspace service (in-memory, no persistence).
#[derive(Debug, Default, Clone)]
pub struct MockExecutionWorkspaceService;

impl MockExecutionWorkspaceService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    fn sample_workspace(company_id: Uuid, project_id: Option<Uuid>, name: String) -> ExecutionWorkspace {
        ExecutionWorkspace {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            project_workspace_id: None,
            source_issue_id: None,
            name,
            mode: WorkspaceMode::Ephemeral,
            strategy_type: WorkspaceStrategyType::GitWorktree,
            status: WorkspaceStatus::Ready,
            cwd: None,
            provider_ref: None,
            base_ref: None,
            branch_name: None,
            repo_url: None,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl MockExecutionWorkspaceService {
    pub async fn create_workspace(
        &self,
        company_id: Uuid,
        project_id: Option<Uuid>,
        name: String,
    ) -> Result<ExecutionWorkspace> {
        Ok(Self::sample_workspace(company_id, project_id, name))
    }

    pub async fn get_workspace(
        &self,
        _id: Uuid,
        company_id: Uuid,
    ) -> Result<Option<ExecutionWorkspace>> {
        Ok(Some(Self::sample_workspace(
            company_id,
            None,
            "mock-workspace".to_string(),
        )))
    }

    pub async fn list_workspaces(&self, company_id: Uuid) -> Result<Vec<ExecutionWorkspace>> {
        Ok(vec![Self::sample_workspace(
            company_id,
            None,
            "mock-workspace".to_string(),
        )])
    }

    pub async fn dispose_workspace(&self, _id: Uuid, _company_id: Uuid) -> Result<()> {
        Ok(())
    }
}
