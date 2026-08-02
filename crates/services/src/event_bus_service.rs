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
        let event_type = event.event_type_str().to_string();
        let event_clone = event.clone();

        if let Some(handlers) = self.handlers.get(&event_type) {
            for handler in handlers.value().iter() {
                let handler = Arc::clone(handler);
                let event = event_clone.clone();
                let et = event_type.clone();

                // Spawn handler execution in background to avoid blocking
                tokio::spawn(async move {
                    if let Err(e) = handler.handle(&event).await {
                        eprintln!(
                            "Event handler '{}' failed for event '{}': {}",
                            handler.handler_name(),
                            et,
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
            event.as_any()
                .downcast_ref::<SystemEvent>()
                .ok_or("Failed to downcast event to SystemEvent")?
                .clone(),
        );

        // Broadcast to all subscribers
        let _ = self.broadcast_tx.send(Arc::clone(&system_event));

        // Dispatch to registered handlers
        self.dispatch_to_handlers(&system_event).await;

        Ok(())
    }

    async fn subscribe(&self, handler: Box<dyn EventHandler>) -> Result<(), String> {
        let handler: Arc<dyn EventHandler> = Arc::from(handler);
        let _handler_name = handler.handler_name().to_string();

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
