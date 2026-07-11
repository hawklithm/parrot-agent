use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// WebSocket connection context
#[derive(Debug, Clone)]
pub struct WebSocketContext {
    pub company_id: Uuid,
    pub actor_type: String, // "board" | "agent"
    pub actor_id: Uuid,
    pub connection_id: Uuid,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSocketMessage {
    /// Ping message for keepalive
    #[serde(rename_all = "camelCase")]
    Ping { timestamp: chrono::DateTime<chrono::Utc> },

    /// Pong response to ping
    #[serde(rename_all = "camelCase")]
    Pong { timestamp: chrono::DateTime<chrono::Utc> },

    /// Subscribe to a channel
    #[serde(rename_all = "camelCase")]
    Subscribe { channel: String },

    /// Unsubscribe from a channel
    #[serde(rename_all = "camelCase")]
    Unsubscribe { channel: String },

    /// Data message on a channel
    #[serde(rename_all = "camelCase")]
    Data {
        channel: String,
        payload: serde_json::Value,
    },

    /// Error message
    #[serde(rename_all = "camelCase")]
    Error { message: String },
}

/// WebSocket connection event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    pub event_type: ConnectionEventType,
    pub connection_id: Uuid,
    pub company_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Connection event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionEventType {
    Connected,
    Disconnected,
    Subscribed,
    Unsubscribed,
    Error,
}
