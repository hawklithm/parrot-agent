use async_trait::async_trait;
use dashmap::DashMap;
use models::event_bus::{Event, EventBus, EventHandler, SystemEvent};
use std::sync::Arc;
use tokio::sync::broadcast;

/// In-memory event bus implementation
pub struct InMemoryEventBus {
    /// Event handlers indexed by event type
    handlers: Arc<DashMap<String, Vec<Arc<dyn EventHandler>>>>,
    /// Broadcast channel for all events
    broadcast_tx: broadcast::Sender<Arc<SystemEvent>>,
}

impl InMemoryEventBus {
    pub fn new(capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(capacity);

        Self {
            handlers: Arc::new(DashMap::new()),
            broadcast_tx,
        }
    }

    /// Get a receiver for listening to all events
    pub fn subscribe_all(&self) -> broadcast::Receiver<Arc<SystemEvent>> {
        self.broadcast_tx.subscribe()
    }

    /// Dispatch event to registered handlers
    async fn dispatch_to_handlers(&self, event: &SystemEvent) {
        let event_type = event.event_type_str();

        if let Some(handlers) = self.handlers.get(event_type) {
            for handler in handlers.value().iter() {
                let handler = Arc::clone(handler);
                let event_ptr: &dyn Event = event;

                // Spawn handler execution in background to avoid blocking
                tokio::spawn(async move {
                    if let Err(e) = handler.handle(event_ptr).await {
                        eprintln!(
                            "Event handler '{}' failed for event '{}': {}",
                            handler.handler_name(),
                            event_type,
                            e
                        );
                    }
                });
            }
        }
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, event: Box<dyn Event>) -> Result<(), String> {
        // Downcast to SystemEvent (assuming all events are SystemEvent)
        let system_event = Arc::new(
            *event
                .as_any()
                .downcast::<SystemEvent>()
                .map_err(|_| "Failed to downcast event to SystemEvent")?,
        );

        // Broadcast to all subscribers
        let _ = self.broadcast_tx.send(Arc::clone(&system_event));

        // Dispatch to registered handlers
        self.dispatch_to_handlers(&system_event).await;

        Ok(())
    }

    async fn subscribe(&self, handler: Box<dyn EventHandler>) -> Result<(), String> {
        let handler = Arc::from(handler);
        let handler_name = handler.handler_name().to_string();

        for event_type in handler.event_types() {
            self.handlers
                .entry(event_type.clone())
                .or_insert_with(Vec::new)
                .push(Arc::clone(&handler));
        }

        Ok(())
    }

    async fn unsubscribe(&self, handler_name: &str) -> Result<(), String> {
        for mut entry in self.handlers.iter_mut() {
            entry.value_mut().retain(|h| h.handler_name() != handler_name);
        }

        Ok(())
    }
}

// Trait extension to support downcasting
pub trait EventExt: Event {
    fn as_any(&self) -> &dyn std::any::Any;
}

impl EventExt for SystemEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Issue completion to goal progress handler
pub struct IssueCompletionToGoalProgressHandler {
    // goal_service: Arc<dyn crate::GoalService>,
}

impl IssueCompletionToGoalProgressHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for IssueCompletionToGoalProgressHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for IssueCompletionToGoalProgressHandler {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        if event.event_type() != "issue.completed" {
            return Ok(());
        }

        // In production: extract issue_id from event payload, query linked goals, recalculate
        // For now: placeholder
        eprintln!("IssueCompletionToGoalProgressHandler: processing issue.completed");

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["issue.completed".to_string()]
    }

    fn handler_name(&self) -> &str {
        "IssueCompletionToGoalProgressHandler"
    }
}

/// Approval approved to issue unblock handler
pub struct ApprovalApprovedToIssueUnblockHandler {
    // issue_service: Arc<dyn crate::IssueService>,
}

impl ApprovalApprovedToIssueUnblockHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ApprovalApprovedToIssueUnblockHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for ApprovalApprovedToIssueUnblockHandler {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        if event.event_type() != "approval.approved" {
            return Ok(());
        }

        // In production: extract approval_id, query linked issue, update status
        eprintln!("ApprovalApprovedToIssueUnblockHandler: processing approval.approved");

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["approval.approved".to_string()]
    }

    fn handler_name(&self) -> &str {
        "ApprovalApprovedToIssueUnblockHandler"
    }
}

/// Routine triggered to issue creation handler
pub struct RoutineTriggeredToIssueCreationHandler {
    // issue_service: Arc<dyn crate::IssueService>,
}

impl RoutineTriggeredToIssueCreationHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for RoutineTriggeredToIssueCreationHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventHandler for RoutineTriggeredToIssueCreationHandler {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        if event.event_type() != "routine.triggered" {
            return Ok(());
        }

        // In production: extract routine_id, create issue, checkout to agent
        eprintln!("RoutineTriggeredToIssueCreationHandler: processing routine.triggered");

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["routine.triggered".to_string()]
    }

    fn handler_name(&self) -> &str {
        "RoutineTriggeredToIssueCreationHandler"
    }
}

/// Environment lease expired to workspace cleanup handler
pub struct EnvironmentLeaseExpiredHandler;

#[async_trait]
impl EventHandler for EnvironmentLeaseExpiredHandler {
    async fn handle(&self, event: &dyn Event) -> Result<(), String> {
        if event.event_type() != "environment.lease_expired" {
            return Ok(());
        }

        // In production: extract environment_id, call workspace cleanup
        eprintln!("EnvironmentLeaseExpiredHandler: processing environment.lease_expired");

        Ok(())
    }

    fn event_types(&self) -> Vec<String> {
        vec!["environment.lease_expired".to_string()]
    }

    fn handler_name(&self) -> &str {
        "EnvironmentLeaseExpiredHandler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use models::event_bus::{EventMetadata, IssueEvent, SystemEvent, SystemEventPayload};
    use uuid::Uuid;

    struct TestHandler {
        name: String,
        event_types: Vec<String>,
    }

    #[async_trait]
    impl EventHandler for TestHandler {
        async fn handle(&self, _event: &dyn Event) -> Result<(), String> {
            Ok(())
        }

        fn event_types(&self) -> Vec<String> {
            self.event_types.clone()
        }

        fn handler_name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn test_event_bus_subscribe_and_publish() {
        let bus = InMemoryEventBus::new(100);

        let handler = Box::new(TestHandler {
            name: "test_handler".to_string(),
            event_types: vec!["issue.created".to_string()],
        });

        bus.subscribe(handler).await.unwrap();

        let event = SystemEvent::new(
            EventMetadata {
                event_id: Uuid::new_v4(),
                correlation_id: None,
                causation_id: None,
                actor_type: "user".to_string(),
                actor_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            },
            SystemEventPayload::Issue(IssueEvent::Created {
                issue_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
                title: "Test Issue".to_string(),
                created_by: Uuid::new_v4(),
            }),
        );

        let result = bus.publish(Box::new(event)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_unsubscribe() {
        let bus = InMemoryEventBus::new(100);

        let handler = Box::new(TestHandler {
            name: "test_handler".to_string(),
            event_types: vec!["issue.created".to_string()],
        });

        bus.subscribe(handler).await.unwrap();
        bus.unsubscribe("test_handler").await.unwrap();

        assert!(bus.handlers.get("issue.created").is_none() || bus.handlers.get("issue.created").unwrap().is_empty());
    }
}
