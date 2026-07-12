use async_trait::async_trait;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Heartbeat service for managing agent wake/sleep lifecycle
#[async_trait]
pub trait HeartbeatService: Send + Sync {
    /// Wake up an agent to work on an issue
    /// Called after checkout to notify the assignee
    async fn wakeup(&self, agent_id: Uuid, issue_id: Uuid, company_id: Uuid) -> Result<(), HeartbeatError>;

    /// Cancel an active run for an issue
    /// Called after force_release to stop ongoing execution
    async fn cancel_run(&self, agent_id: Uuid, issue_id: Uuid, company_id: Uuid, reason: &str) -> Result<(), HeartbeatError>;

    /// Get heartbeat context for an issue (diagnostics/monitoring)
    async fn get_heartbeat_context(&self, issue_id: Uuid, company_id: Uuid) -> Result<HeartbeatContext, HeartbeatError>;
}

/// Heartbeat context information for an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatContext {
    pub issue_id: Uuid,
    pub company_id: Uuid,
    pub active_agents: Vec<AgentHeartbeatInfo>,
    pub last_wakeup_at: Option<DateTime<Utc>>,
    pub wakeup_count: i64,
}

/// Agent heartbeat information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHeartbeatInfo {
    pub agent_id: Uuid,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub status: HeartbeatStatus,
}

/// Heartbeat status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatStatus {
    Active,
    Idle,
    Sleeping,
    Unknown,
}

/// Heartbeat error
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),

    #[error("Issue not found: {0}")]
    IssueNotFound(Uuid),

    #[error("Wakeup failed: {0}")]
    WakeupFailed(String),

    #[error("Cancel run failed: {0}")]
    CancelRunFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Default no-op implementation of HeartbeatService
pub struct DefaultHeartbeatService;

#[async_trait]
impl HeartbeatService for DefaultHeartbeatService {
    async fn wakeup(&self, agent_id: Uuid, issue_id: Uuid, _company_id: Uuid) -> Result<(), HeartbeatError> {
        // In production: send wakeup signal via WebSocket/SSE to the agent
        // For now, log the wakeup event
        tracing::info!(
            "Heartbeat wakeup: agent_id={}, issue_id={}",
            agent_id, issue_id
        );
        Ok(())
    }

    async fn cancel_run(&self, agent_id: Uuid, issue_id: Uuid, _company_id: Uuid, reason: &str) -> Result<(), HeartbeatError> {
        // In production: send cancel signal to the agent's active run
        tracing::info!(
            "Heartbeat cancel_run: agent_id={}, issue_id={}, reason={}",
            agent_id, issue_id, reason
        );
        Ok(())
    }

    async fn get_heartbeat_context(&self, issue_id: Uuid, _company_id: Uuid) -> Result<HeartbeatContext, HeartbeatError> {
        Ok(HeartbeatContext {
            issue_id,
            company_id: _company_id,
            active_agents: vec![],
            last_wakeup_at: None,
            wakeup_count: 0,
        })
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    pub struct MockHeartbeatService {
        wakeup_count: AtomicI64,
        cancel_count: AtomicI64,
    }

    impl MockHeartbeatService {
        pub fn new() -> Self {
            Self {
                wakeup_count: AtomicI64::new(0),
                cancel_count: AtomicI64::new(0),
            }
        }

        pub fn wakeup_call_count(&self) -> i64 {
            self.wakeup_count.load(Ordering::Relaxed)
        }

        pub fn cancel_call_count(&self) -> i64 {
            self.cancel_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl HeartbeatService for MockHeartbeatService {
        async fn wakeup(&self, _agent_id: Uuid, _issue_id: Uuid, _company_id: Uuid) -> Result<(), HeartbeatError> {
            self.wakeup_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn cancel_run(&self, _agent_id: Uuid, _issue_id: Uuid, _company_id: Uuid, _reason: &str) -> Result<(), HeartbeatError> {
            self.cancel_count.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn get_heartbeat_context(&self, issue_id: Uuid, _company_id: Uuid) -> Result<HeartbeatContext, HeartbeatError> {
            Ok(HeartbeatContext {
                issue_id,
                company_id: _company_id,
                active_agents: vec![],
                last_wakeup_at: None,
                wakeup_count: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_heartbeat_service() {
        let service = DefaultHeartbeatService;
        let agent_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        // Wakeup should succeed
        let result = service.wakeup(agent_id, issue_id, company_id).await;
        assert!(result.is_ok());

        // Cancel run should succeed
        let result = service.cancel_run(agent_id, issue_id, company_id, "test").await;
        assert!(result.is_ok());

        // Get context should succeed
        let context = service.get_heartbeat_context(issue_id, company_id).await.unwrap();
        assert_eq!(context.issue_id, issue_id);
        assert_eq!(context.company_id, company_id);
    }

    #[tokio::test]
    async fn test_mock_heartbeat_service() {
        let service = mock::MockHeartbeatService::new();
        let agent_id = Uuid::new_v4();
        let issue_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        assert_eq!(service.wakeup_call_count(), 0);
        assert_eq!(service.cancel_call_count(), 0);

        service.wakeup(agent_id, issue_id, company_id).await.unwrap();
        assert_eq!(service.wakeup_call_count(), 1);

        service.cancel_run(agent_id, issue_id, company_id, "test").await.unwrap();
        assert_eq!(service.cancel_call_count(), 1);
    }
}
