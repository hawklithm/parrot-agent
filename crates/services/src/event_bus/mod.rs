use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::ServiceResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub metadata: EventMetadata,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    pub company_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IssueEvent {
    Created { issue_id: Uuid },
    CheckedOut { issue_id: Uuid, agent_id: Uuid },
    Released { issue_id: Uuid },
    StatusChanged { issue_id: Uuid, old_status: String, new_status: String },
    Completed { issue_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Requested { approval_id: Uuid, issue_id: Option<Uuid> },
    Approved { approval_id: Uuid, approver_id: Uuid },
    Rejected { approval_id: Uuid, approver_id: Uuid },
    RevisionRequested { approval_id: Uuid, approver_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineEvent {
    Triggered { routine_id: Uuid, run_id: Uuid },
    RunStarted { run_id: Uuid },
    RunCompleted { run_id: Uuid, status: String },
    RunFailed { run_id: Uuid, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Hired { agent_id: Uuid },
    StatusChanged { agent_id: Uuid, old_status: String, new_status: String },
    Terminated { agent_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvironmentEvent {
    LeaseAcquired { lease_id: Uuid, environment_id: Uuid, agent_id: Uuid },
    LeaseReleased { lease_id: Uuid },
    LeaseExpired { lease_id: Uuid, environment_id: Uuid },
}

pub type EventHandlerFn = Arc<dyn Fn(Event) -> std::pin::Pin<Box<dyn std::future::Future<Output = ServiceResult<()>> + Send>> + Send + Sync>;

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: Event) -> ServiceResult<()>;
    async fn subscribe(&self, event_type: String, handler: EventHandlerFn) -> ServiceResult<Uuid>;
    async fn unsubscribe(&self, subscription_id: Uuid) -> ServiceResult<()>;
}

pub struct InMemoryEventBus {
    subscribers: Arc<RwLock<HashMap<String, HashMap<Uuid, EventHandlerFn>>>>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
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
    async fn publish(&self, event: Event) -> ServiceResult<()> {
        let subscribers = self.subscribers.read().await;

        if let Some(handlers) = subscribers.get(&event.event_type) {
            let mut tasks = vec![];

            for handler in handlers.values() {
                let event_clone = event.clone();
                let handler_clone = Arc::clone(handler);
                tasks.push(tokio::spawn(async move {
                    handler_clone(event_clone).await
                }));
            }

            for task in tasks {
                if let Err(e) = task.await {
                    eprintln!("Event handler task failed: {}", e);
                }
            }
        }

        Ok(())
    }

    async fn subscribe(&self, event_type: String, handler: EventHandlerFn) -> ServiceResult<Uuid> {
        let subscription_id = Uuid::new_v4();
        let mut subscribers = self.subscribers.write().await;

        subscribers
            .entry(event_type)
            .or_insert_with(HashMap::new)
            .insert(subscription_id, handler);

        Ok(subscription_id)
    }

    async fn unsubscribe(&self, subscription_id: Uuid) -> ServiceResult<()> {
        let mut subscribers = self.subscribers.write().await;

        for handlers in subscribers.values_mut() {
            handlers.remove(&subscription_id);
        }

        Ok(())
    }
}
