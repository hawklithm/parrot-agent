use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt::Debug;
use uuid::Uuid;

/// Event trait that all events must implement
#[async_trait]
pub trait Event: Send + Sync + Debug {
    fn event_type(&self) -> &str;
    fn payload(&self) -> JsonValue;
    fn metadata(&self) -> JsonValue;
    fn timestamp(&self) -> chrono::DateTime<chrono::Utc>;
}

/// Event handler trait
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(&self, event: Box<dyn Event>) -> Result<(), EventBusError>;
    fn event_type(&self) -> &str;
}

/// Event bus error
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("Handler error: {0}")]
    HandlerError(String),
    #[error("Publish error: {0}")]
    PublishError(String),
    #[error("Subscribe error: {0}")]
    SubscribeError(String),
}

/// Issue events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueEvent {
    Created { issue_id: Uuid, company_id: Uuid, title: String },
    CheckedOut { issue_id: Uuid, agent_id: Uuid },
    Released { issue_id: Uuid, agent_id: Uuid },
    StatusChanged { issue_id: Uuid, from_status: String, to_status: String },
    Completed { issue_id: Uuid, terminal_state: String },
    Commented { issue_id: Uuid, user_id: Uuid, comment: String },
}

/// Approval events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalEvent {
    Requested { approval_id: Uuid, company_id: Uuid, requested_by: Uuid },
    Approved { approval_id: Uuid, approved_by: Uuid },
    Rejected { approval_id: Uuid, rejected_by: Uuid, reason: String },
    RevisionRequested { approval_id: Uuid, requested_by: Uuid, reason: String },
}

/// Routine events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutineEvent {
    Triggered { routine_id: Uuid, trigger_id: Uuid, source: String },
    RunStarted { run_id: Uuid, routine_id: Uuid },
    RunCompleted { run_id: Uuid, routine_id: Uuid, status: String },
    RunFailed { run_id: Uuid, routine_id: Uuid, error: String },
}

/// Agent events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Hired { agent_id: Uuid, company_id: Uuid, agent_type: String },
    StatusChanged { agent_id: Uuid, from_status: String, to_status: String },
    Terminated { agent_id: Uuid, reason: String },
}

/// Environment events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvironmentEvent {
    LeaseAcquired { lease_id: Uuid, environment_id: Uuid, agent_id: Uuid },
    LeaseReleased { lease_id: Uuid, environment_id: Uuid },
    LeaseExpired { lease_id: Uuid, environment_id: Uuid },
}

/// Goal events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoalEvent {
    Created { goal_id: Uuid, company_id: Uuid, title: String },
    ProgressUpdated { goal_id: Uuid, progress: f64 },
    Completed { goal_id: Uuid },
}

/// Unified domain event wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    pub id: Uuid,
    pub event_type: String,
    pub payload: JsonValue,
    pub metadata: JsonValue,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl DomainEvent {
    pub fn new(event_type: String, payload: JsonValue) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            payload,
            metadata: serde_json::json!({}),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn with_metadata(mut self, metadata: JsonValue) -> Self {
        self.metadata = metadata;
        self
    }
}

#[async_trait]
impl Event for DomainEvent {
    fn event_type(&self) -> &str {
        &self.event_type
    }

    fn payload(&self) -> JsonValue {
        self.payload.clone()
    }

    fn metadata(&self) -> JsonValue {
        self.metadata.clone()
    }

    fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        self.timestamp
    }
}

/// Event bus trait
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: Box<dyn Event>) -> Result<(), EventBusError>;
    async fn subscribe(&self, event_type: String, handler: Box<dyn EventHandler>) -> Result<(), EventBusError>;
    async fn unsubscribe(&self, event_type: &str) -> Result<(), EventBusError>;
}

/// In-memory event bus implementation
pub mod in_memory {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    pub struct InMemoryEventBus {
        handlers: Arc<RwLock<HashMap<String, Vec<Arc<Box<dyn EventHandler>>>>>>,
    }

    impl InMemoryEventBus {
        pub fn new() -> Self {
            Self {
                handlers: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    impl Default for InMemoryEventBus {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl EventBus for InMemoryEventBus {
        async fn publish(&self, event: Box<dyn Event>) -> Result<(), EventBusError> {
            let event_type = event.event_type().to_string();
            let handlers = self.handlers.read().await;

            if let Some(handler_list) = handlers.get(&event_type) {
                for handler in handler_list {
                    // Clone event data for each handler
                    let cloned_event = DomainEvent {
                        id: Uuid::new_v4(),
                        event_type: event.event_type().to_string(),
                        payload: event.payload(),
                        metadata: event.metadata(),
                        timestamp: event.timestamp(),
                    };

                    if let Err(e) = handler.handle(Box::new(cloned_event)).await {
                        eprintln!("Handler error for event {}: {}", event_type, e);
                    }
                }
            }

            Ok(())
        }

        async fn subscribe(&self, event_type: String, handler: Box<dyn EventHandler>) -> Result<(), EventBusError> {
            let mut handlers = self.handlers.write().await;
            handlers
                .entry(event_type)
                .or_insert_with(Vec::new)
                .push(Arc::new(handler));
            Ok(())
        }

        async fn unsubscribe(&self, event_type: &str) -> Result<(), EventBusError> {
            let mut handlers = self.handlers.write().await;
            handlers.remove(event_type);
            Ok(())
        }
    }
}
