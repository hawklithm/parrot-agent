use crate::errors::{ServiceError, ServiceResult};
use async_trait::async_trait;
use models::{ConnectionEvent, ConnectionEventType, WebSocketContext, WebSocketMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Service for WebSocket connection and message routing
#[async_trait]
pub trait WebSocketService: Send + Sync {
    /// Register a new WebSocket connection
    async fn register_connection(&self, context: WebSocketContext) -> ServiceResult<()>;

    /// Unregister a WebSocket connection
    async fn unregister_connection(&self, connection_id: Uuid) -> ServiceResult<()>;

    /// Subscribe a connection to a channel
    async fn subscribe_to_channel(
        &self,
        connection_id: Uuid,
        channel: String,
    ) -> ServiceResult<()>;

    /// Unsubscribe a connection from a channel
    async fn unsubscribe_from_channel(
        &self,
        connection_id: Uuid,
        channel: String,
    ) -> ServiceResult<()>;

    /// Broadcast message to all subscribers of a channel
    async fn broadcast_to_channel(
        &self,
        company_id: Uuid,
        channel: &str,
        message: WebSocketMessage,
    ) -> ServiceResult<usize>;

    /// Get active connection count
    async fn connection_count(&self) -> usize;
}

/// In-memory WebSocket service implementation
pub struct WebSocketServiceImpl {
    connections: Arc<RwLock<HashMap<Uuid, WebSocketContext>>>,
    subscriptions: Arc<RwLock<HashMap<String, std::collections::HashSet<Uuid>>>>,
    event_broadcaster: broadcast::Sender<ConnectionEvent>,
}

impl WebSocketServiceImpl {
    pub fn new() -> Self {
        let (event_tx, _event_rx) = broadcast::channel(1000);
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: event_tx,
        }
    }

    fn channel_key(company_id: Uuid, channel: &str) -> String {
        format!("{}:{}", company_id, channel)
    }

    async fn emit_event(&self, event_type: ConnectionEventType, connection_id: Uuid, company_id: Uuid) {
        let event = ConnectionEvent {
            event_type,
            connection_id,
            company_id,
            timestamp: chrono::Utc::now(),
        };
        let _ = self.event_broadcaster.send(event);
    }
}

#[async_trait]
impl WebSocketService for WebSocketServiceImpl {
    async fn register_connection(&self, context: WebSocketContext) -> ServiceResult<()> {
        let connection_id = context.connection_id;
        let company_id = context.company_id;
        self.connections.write().await.insert(connection_id, context);
        self.emit_event(ConnectionEventType::Connected, connection_id, company_id).await;
        Ok(())
    }

    async fn unregister_connection(&self, connection_id: Uuid) -> ServiceResult<()> {
        let context = self.connections.write().await.remove(&connection_id)
            .ok_or_else(|| ServiceError::NotFound("Connection not found".to_string()))?;
        let mut subscriptions = self.subscriptions.write().await;
        for subscribers in subscriptions.values_mut() {
            subscribers.remove(&connection_id);
        }
        self.emit_event(ConnectionEventType::Disconnected, connection_id, context.company_id).await;
        Ok(())
    }

    async fn subscribe_to_channel(&self, connection_id: Uuid, channel: String) -> ServiceResult<()> {
        let connections = self.connections.read().await;
        let context = connections.get(&connection_id)
            .ok_or_else(|| ServiceError::NotFound("Connection not found".to_string()))?;
        let key = Self::channel_key(context.company_id, &channel);
        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.entry(key).or_insert_with(std::collections::HashSet::new).insert(connection_id);
        self.emit_event(ConnectionEventType::Subscribed, connection_id, context.company_id).await;
        Ok(())
    }

    async fn unsubscribe_from_channel(&self, connection_id: Uuid, channel: String) -> ServiceResult<()> {
        let connections = self.connections.read().await;
        let context = connections.get(&connection_id)
            .ok_or_else(|| ServiceError::NotFound("Connection not found".to_string()))?;
        let key = Self::channel_key(context.company_id, &channel);
        let mut subscriptions = self.subscriptions.write().await;
        if let Some(subscribers) = subscriptions.get_mut(&key) {
          subscribers.remove(&connection_id);
        }
        self.emit_event(ConnectionEventType::Unsubscribed, connection_id, context.company_id).await;
        Ok(())
    }

    async fn broadcast_to_channel(&self, company_id: Uuid, channel: &str, _message: WebSocketMessage) -> ServiceResult<usize> {
        let key = Self::channel_key(company_id, channel);
        let subscriptions = self.subscriptions.read().await;
        let subscriber_count = subscriptions.get(&key).map(|set| set.len()).unwrap_or(0);
        Ok(subscriber_count)
    }

    async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

pub fn create_websocket_service() -> Arc<dyn WebSocketService> {
    Arc::new(WebSocketServiceImpl::new())
}
