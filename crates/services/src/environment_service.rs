use async_trait::async_trait;
use uuid::Uuid;
use crate::models::{Environment, EnvironmentLease, ExecutionWorkspace, CreateEnvironmentInput, UpdateEnvironmentInput};

/// Environment service trait
#[async_trait]
pub trait EnvironmentService: Send + Sync {
    /// List environments for a company
    async fn list_environments(&self, company_id: Uuid) -> Result<Vec<Environment>, String>;
    
    /// Get environment by ID
    async fn get_environment(&self, id: Uuid, company_id: Uuid) -> Result<Option<Environment>, String>;
    
    /// Create environment
    async fn create_environment(&self, company_id: Uuid, input: CreateEnvironmentInput) -> Result<Environment, String>;
    
    /// Update environment
    async fn update_environment(&self, id: Uuid, company_id: Uuid, input: UpdateEnvironmentInput) -> Result<Environment, String>;
    
    /// Delete environment
    async fn delete_environment(&self, id: Uuid, company_id: Uuid) -> Result<bool, String>;
    
    /// Probe environment (test connectivity)
    async fn probe_environment(&self, id: Uuid, company_id: Uuid) -> Result<serde_json::Value, String>;
}

/// Environment lease service trait
#[async_trait]
pub trait EnvironmentLeaseService: Send + Sync {
    /// Acquire environment lease
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        company_id: Uuid,
        issue_id: Option<Uuid>,
        heartbeat_run_id: Option<Uuid>,
    ) -> Result<EnvironmentLease, String>;
    
    /// Release environment lease
    async fn release_lease(&self, lease_id: Uuid, company_id: Uuid) -> Result<EnvironmentLease, String>;
    
    /// List active leases for a company
    async fn list_active_leases(&self, company_id: Uuid) -> Result<Vec<EnvironmentLease>, String>;
}

/// Execution workspace service trait
#[async_trait]
pub trait ExecutionWorkspaceService: Send + Sync {
    /// Create execution workspace
    async fn create_workspace(
        &self,
        company_id: Uuid,
        project_id: Option<Uuid>,
        name: String,
    ) -> Result<ExecutionWorkspace, String>;
    
    /// Get workspace by ID
    async fn get_workspace(&self, id: Uuid, company_id: Uuid) -> Result<Option<ExecutionWorkspace>, String>;
    
    /// List workspaces for a company
    async fn list_workspaces(&self, company_id: Uuid) -> Result<Vec<ExecutionWorkspace>, String>;
    
    /// Dispose workspace
    async fn dispose_workspace(&self, id: Uuid, company_id: Uuid) -> Result<bool, String>;
}

/// Mock implementation of EnvironmentService
pub struct MockEnvironmentService;

impl MockEnvironmentService {
    pub fn new() -> Self {
        Self
    }
    
    fn create_mock_environment(id: Uuid, company_id: Uuid, name: String) -> Environment {
        use crate::models::{EnvironmentDriver, EnvironmentStatus};
        Environment {
            id,
            company_id,
            name,
            description: Some("Mock environment".to_string()),
            driver: EnvironmentDriver::Local,
            status: EnvironmentStatus::Active,
            config: serde_json::json!({}),
            env_vars: serde_json::json!({}),
            metadata: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

#[async_trait]
impl EnvironmentService for MockEnvironmentService {
    async fn list_environments(&self, company_id: Uuid) -> Result<Vec<Environment>, String> {
        Ok(vec![
            Self::create_mock_environment(Uuid::new_v4(), company_id, "Local Dev".to_string()),
            Self::create_mock_environment(Uuid::new_v4(), company_id, "SSH Remote".to_string()),
        ])
    }
    
    async fn get_environment(&self, id: Uuid, company_id: Uuid) -> Result<Option<Environment>, String> {
        Ok(Some(Self::create_mock_environment(id, company_id, "Mock Environment".to_string())))
    }
    
    async fn create_environment(&self, company_id: Uuid, input: CreateEnvironmentInput) -> Result<Environment, String> {
        let mut env = Self::create_mock_environment(Uuid::new_v4(), company_id, input.name);
        env.driver = input.driver;
        env.config = input.config;
        env.env_vars = input.env_vars.unwrap_or_else(|| serde_json::json!({}));
        Ok(env)
    }
    
    async fn update_environment(&self, id: Uuid, company_id: Uuid, input: UpdateEnvironmentInput) -> Result<Environment, String> {
        let mut env = Self::create_mock_environment(id, company_id, input.name.unwrap_or_else(|| "Updated".to_string()));
        if let Some(status) = input.status {
            env.status = status;
        }
        Ok(env)
    }
    
    async fn delete_environment(&self, _id: Uuid, _company_id: Uuid) -> Result<bool, String> {
        Ok(true)
    }
    
    async fn probe_environment(&self, _id: Uuid, _company_id: Uuid) -> Result<serde_json::Value, String> {
        Ok(serde_json::json!({"ok": true, "driver": "local", "summary": "All checks passed"}))
    }
}

/// Mock implementation of EnvironmentLeaseService
pub struct MockEnvironmentLeaseService;

impl MockEnvironmentLeaseService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl EnvironmentLeaseService for MockEnvironmentLeaseService {
    async fn acquire_lease(
        &self,
        environment_id: Uuid,
        company_id: Uuid,
        issue_id: Option<Uuid>,
        heartbeat_run_id: Option<Uuid>,
    ) -> Result<EnvironmentLease, String> {
        use crate::models::LeaseStatus;
        Ok(EnvironmentLease {
            id: Uuid::new_v4(),
            company_id,
            environment_id,
            execution_workspace_id: None,
            issue_id,
            heartbeat_run_id,
            status: LeaseStatus::Active,
            lease_policy: None,
            provider: Some("local".to_string()),
            provider_lease_id: None,
            acquired_at: chrono::Utc::now(),
            last_used_at: Some(chrono::Utc::now()),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            released_at: None,
            failure_reason: None,
            cleanup_status: None,
        })
    }
    
    async fn release_lease(&self, lease_id: Uuid, company_id: Uuid) -> Result<EnvironmentLease, String> {
        use crate::models::LeaseStatus;
        Ok(EnvironmentLease {
            id: lease_id,
            company_id,
            environment_id: Uuid::new_v4(),
            execution_workspace_id: None,
            issue_id: None,
            heartbeat_run_id: None,
            status: LeaseStatus::Released,
            lease_policy: None,
            provider: Some("local".to_string()),
            provider_lease_id: None,
            acquired_at: chrono::Utc::now() - chrono::Duration::hours(1),
            last_used_at: Some(chrono::Utc::now()),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            released_at: Some(chrono::Utc::now()),
            failure_reason: None,
            cleanup_status: Some("cleaned".to_string()),
        })
    }
    
    async fn list_active_leases(&self, company_id: Uuid) -> Result<Vec<EnvironmentLease>, String> {
        use crate::models::LeaseStatus;
        Ok(vec![
            EnvironmentLease {
                id: Uuid::new_v4(),
                company_id,
                environment_id: Uuid::new_v4(),
                execution_workspace_id: None,
                issue_id: None,
                heartbeat_run_id: None,
                status: LeaseStatus::Active,
                lease_policy: None,
                provider: Some("local".to_string()),
                provider_lease_id: None,
                acquired_at: chrono::Utc::now() - chrono::Duration::minutes(30),
                last_used_at: Some(chrono::Utc::now()),
                expires_at: Some(chrono::Utc::now() + chrono::Duration::minutes(30)),
                released_at: None,
                failure_reason: None,
                cleanup_status: None,
            },
        ])
    }
}

/// Mock implementation of ExecutionWorkspaceService
pub struct MockExecutionWorkspaceService;

impl MockExecutionWorkspaceService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExecutionWorkspaceService for MockExecutionWorkspaceService {
    async fn create_workspace(
        &self,
        company_id: Uuid,
        project_id: Option<Uuid>,
        name: String,
    ) -> Result<ExecutionWorkspace, String> {
        use crate::models::{ExecutionWorkspaceMode, ExecutionWorkspaceStrategyType, ExecutionWorkspaceStatus};
        Ok(ExecutionWorkspace {
            id: Uuid::new_v4(),
            company_id,
            project_id,
            project_workspace_id: None,
            source_issue_id: None,
            name,
            mode: ExecutionWorkspaceMode::Ephemeral,
            strategy_type: ExecutionWorkspaceStrategyType::CloneAndCheckout,
            status: ExecutionWorkspaceStatus::Provisioning,
            cwd: Some("/tmp/workspace".to_string()),
            provider_ref: None,
            base_ref: Some("main".to_string()),
            branch_name: None,
            repo_url: None,
            metadata: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }
    
    async fn get_workspace(&self, id: Uuid, company_id: Uuid) -> Result<Option<ExecutionWorkspace>, String> {
        use crate::models::{ExecutionWorkspaceMode, ExecutionWorkspaceStrategyType, ExecutionWorkspaceStatus};
        Ok(Some(ExecutionWorkspace {
            id,
            company_id,
            project_id: None,
            project_workspace_id: None,
            source_issue_id: None,
            name: "Mock Workspace".to_string(),
            mode: ExecutionWorkspaceMode::Ephemeral,
            strategy_type: ExecutionWorkspaceStrategyType::CloneAndCheckout,
            status: ExecutionWorkspaceStatus::Ready,
            cwd: Some("/tmp/workspace".to_string()),
            provider_ref: None,
            base_ref: Some("main".to_string()),
            branch_name: None,
            repo_url: None,
            metadata: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }))
    }
    
    async fn list_workspaces(&self, company_id: Uuid) -> Result<Vec<ExecutionWorkspace>, String> {
        use crate::models::{ExecutionWorkspaceMode, ExecutionWorkspaceStrategyType, ExecutionWorkspaceStatus};
        Ok(vec![
            ExecutionWorkspace {
                id: Uuid::new_v4(),
                company_id,
                project_id: None,
                project_workspace_id: None,
                source_issue_id: None,
                name: "Workspace 1".to_string(),
                mode: ExecutionWorkspaceMode::Ephemeral,
                strategy_type: ExecutionWorkspaceStrategyType::CloneAndCheckout,
                status: ExecutionWorkspaceStatus::Ready,
                cwd: Some("/tmp/workspace1".to_string()),
                provider_ref: None,
                base_ref: Some("main".to_string()),
                branch_name: None,
                repo_url: None,
                metadata: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        ])
    }
    
    async fn dispose_workspace(&self, _id: Uuid, _company_id: Uuid) -> Result<bool, String> {
        Ok(true)
    }
}
