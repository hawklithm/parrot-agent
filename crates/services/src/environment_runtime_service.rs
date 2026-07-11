use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

/// 环境运行时租约错误
#[derive(Debug, Error)]
pub enum EnvironmentRuntimeError {
    #[error("Environment not found: {0}")]
    EnvironmentNotFound(String),

    #[error("Lease acquisition failed: {0}")]
    LeaseAcquireFailed(String),

    #[error("Lease release failed: {0}")]
    LeaseReleaseFailed(String),

    #[error("Workspace realization failed: {0}")]
    WorkspaceRealizationFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

/// 环境租约状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseStatus {
    Active,
    Released,
    Expired,
    Failed,
}

/// 环境租约记录
#[derive(Debug, Clone)]
pub struct EnvironmentLease {
    pub id: Uuid,
    pub environment_id: String,
    pub agent_id: Option<Uuid>,
    pub issue_id: Option<Uuid>,
    pub status: LeaseStatus,
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
    pub failure_reason: Option<String>,
    pub metadata: JsonValue,
}

/// 工作区实例化结果
#[derive(Debug, Clone)]
pub struct WorkspaceRealizationResult {
    pub workspace_path: String,
    pub execution_target: Option<JsonValue>,
    pub metadata: HashMap<String, JsonValue>,
}

/// 执行目标解析结果
#[derive(Debug, Clone)]
pub struct ExecutionTargetResult {
    pub target_type: String,
    pub connection_info: JsonValue,
    pub metadata: HashMap<String, JsonValue>,
}

/// 环境运行时服务 trait
#[async_trait]
pub trait EnvironmentRuntimeService: Send + Sync {
    /// 获取环境租约（用于运行时执行）
    ///
    /// # 参数
    /// - environment_id: 环境ID
    /// - agent_id: 请求租约的Agent ID
    /// - lease_metadata: 租约元数据（如issue_id、workspace配置等）
    ///
    /// # 返回
    /// - Ok(EnvironmentLease): 成功获取的租约
    /// - Err: 租约获取失败
    async fn acquire_run_lease(
        &self,
        environment_id: &str,
        agent_id: Option<Uuid>,
        lease_metadata: JsonValue,
    ) -> Result<EnvironmentLease, EnvironmentRuntimeError>;

    /// 释放环境租约
    ///
    /// # 参数
    /// - lease_id: 租约ID
    /// - status: 释放状态（released/expired/failed）
    ///
    /// # 返回
    /// - Ok(()): 成功释放
    /// - Err: 释放失败
    async fn release_run_lease(
        &self,
        lease_id: Uuid,
        status: LeaseStatus,
    ) -> Result<(), EnvironmentRuntimeError>;

    /// 实例化工作区（在环境中创建/准备工作目录）
    ///
    /// # 参数
    /// - lease: 环境租约
    /// - workspace_config: 工作区配置（如git repo、branch等）
    ///
    /// # 返回
    /// - Ok(WorkspaceRealizationResult): 工作区实例化结果
    /// - Err: 实例化失败
    async fn realize_workspace(
        &self,
        lease: &EnvironmentLease,
        work_config: JsonValue,
    ) -> Result<WorkspaceRealizationResult, EnvironmentRuntimeError>;

    /// 解析环境执行目标（获取连接信息）
    ///
    /// # 参数
    /// - environment_id: 环境ID
    /// - adapter_type: 适配器类型（用于适配不同的执行方式）
    ///
    /// # 返回
    /// - Ok(ExecutionTargetResult): 执行目标信息
    /// - Err: 解析失败
    async fn resolve_environment_execution_target(
        &self,
        environment_id: &str,
        adapter_type: &str,
    ) -> Result<ExecutionTargetResult, EnvironmentRuntimeError>;
}

/// 默认的环境运行时服务实现（占位符，用于测试）
pub struct DefaultEnvironmentRuntimeService;

impl DefaultEnvironmentRuntimeService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultEnvironmentRuntimeService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EnvironmentRuntimeService for DefaultEnvironmentRuntimeService {
    async fn acquire_run_lease(
        &self,
        environment_id: &str,
        agent_id: Option<Uuid>,
        lease_metadata: JsonValue,
    ) -> Result<EnvironmentLease, EnvironmentRuntimeError> {
        // 占位实现：创建一个临时租约
        Ok(EnvironmentLease {
            id: Uuid::new_v4(),
            environment_id: environment_id.to_string(),
            agent_id,
       issue_id: None,
            status: LeaseStatus::Active,
            acquired_at: chrono::Utc::now(),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            released_at: None,
            failure_reason: None,
            metadata: lease_metadata,
        })
    }

    async fn release_run_lease(
        &self,
        _lease_id: Uuid,
        _status: LeaseStatus,
    ) -> Result<(), EnvironmentRuntimeError> {
        // 占位实现：直接返回成功
        Ok(())
    }

    async fn realize_workspace(
        &self,
        _lease: &EnvironmentLease,
        _workspace_config: JsonValue,
    ) -> Result<WorkspaceRealizationResult, EnvironmentRuntimeError> {
        // 占位实现：返回本地路径
        Ok(WorkspaceRealizationResult {
            workspace_path: "/tmp/workspace".to_string(),
            execution_target: None,
            metadata: HashMap::new(),
        })
    }

    async fn resolve_environment_execution_target(
        &self,
        environment_id: &str,
        adapter_type: &str,
    ) -> Result<ExecutionTargetResult, EnvironmentRuntimeError> {
        // 占位实现：返回本地执行目标
        Ok(ExecutionTargetResult {
            target_type: "local".to_string(),
            connection_info: serde_json::json!({
                "environment_id": environment_id,
                "adapter_type": adapter_type,
            }),
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_acquire_and_release_lease() {
        let service = DefaultEnvironmentRuntimeService::new();

        let lease = service
            .acquire_run_lease("env-local", None, serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(lease.environment_id, "env-local");
        assert_eq!(lease.status, LeaseStatus::Active);

        service
            .release_run_lease(lease.id, LeaseStatus::Released)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_realize_workspace() {
        let service = DefaultEnvironmentRuntimeService::new();

        let lease = service
            .acquire_run_lease("env-local", None, serde_json::json!({}))
            .await
            .unwrap();

        let result = service
            .realize_workspace(&lease, serde_json::json!({"repo": "test"}))
            .await
            .unwrap();

        assert!(!result.workspace_path.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_execution_target() {
        let service = DefaultEnvironmentRuntimeService::new();

        let result = service
            .resolve_environment_execution_target("env-local", "process")
            .await
            .unwrap();

        assert_eq!(result.target_type, "local");
    }
}
