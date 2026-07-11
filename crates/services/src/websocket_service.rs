use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WsMessage {
    Subscribe {
        channel: String,
    },
    Unsubscribe {
        channel: String,
    },
    Event {
        channel: String,
        event: String,
        data: JsonValue,
    },
    Response {
        request_id: Option<String>,
        data: JsonValue,
    },
    Error {
        request_id: Option<String>,
        error: String,
    },
    Ping,
    Pong,
}

/// WebSocket session information
#[derive(Debug, Clone)]
pub struct WsSession {
    pub session_id: Uuid,
    pub user_id: Option<Uuid>,
    pub company_id: Uuid,
    pub permissions: Vec<String>,
    pub subscriptions: HashSet<String>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

impl WsSession {
    pub fn new(session_id: Uuid, company_id: Uuid, user_id: Option<Uuid>) -> Self {
        let now = chrono::Utc::now();
        Self {
            session_id,
            user_id,
            company_id,
            permissions: Vec::new(),
            subscriptions: HashSet::new(),
            connected_at: now,
            last_activity: now,
        }
    }

    pub fn add_subscription(&mut self, channel: String) {
        self.subscriptions.insert(channel);
        self.last_activity = chrono::Utc::now();
    }

    pub fn remove_subscription(&mut self, channel: &str) {
        self.subscriptions.remove(channel);
        self.last_activity = chrono::Utc::now();
    }

    pub fn is_subscribed(&self, channel: &str) -> bool {
        self.subscriptions.contains(channel)
    }

    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now();
    }
}

/// Session manager trait for managing WebSocket connections
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Register a new connection
    async fn register_connection(&self, session: WsSession) -> Result<(), WebSocketError>;

    /// Remove a connection
    async fn remove_connection(&self, session_id: Uuid) -> Result<(), WebSocketError>;

    /// Get session by ID
    async fn get_session(&self, session_id: Uuid) -> Result<Option<WsSession>, WebSocketError>;

    /// Update session activity
    async fn update_activity(&self, session_id: Uuid) -> Result<(), WebSocketError>;

    /// Add subscription to a channel
    async fn subscribe(&self, session_id: Uuid, channel: String) -> Result<(), WebSocketError>;

    /// Remove subscription from a channel
    async fn unsubscribe(&self, session_id: Uuid, channel: &str) -> Result<(), WebSocketError>;

    /// Broadcast message to all subscribers of a channel
    async fn broadcast(&self, channel: &str, message: WsMessage) -> Result<usize, WebSocketError>;

    /// Send message to a specific session
    async fn send_to_session(&self, session_id: Uuid, message: WsMessage) -> Result<(), WebSocketError>;

    /// List all active sessions
    async fn list_sessions(&self) -> Result<Vec<WsSession>, WebSocketError>;

    /// Count active connections
    async fn connection_count(&self) -> Result<usize, WebSocketError>;

    /// Mark connection as unhealthy
    async fn mark_connection_unhealthy(&self, session_id: Uuid) -> Result<(), WebSocketError>;
}

/// In-memory session manager implementation
pub struct DefaultSessionManager {
    sessions: Arc<RwLock<HashMap<Uuid, WsSession>>>,
    // In a real implementation, this would hold actual WebSocket senders
    // For now, we just track session state
}

impl DefaultSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a session is stale (no activity for > 5 minutes)
    async fn is_stale(&self, session_id: Uuid) -> bool {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(&session_id) {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(session.last_activity);
            duration.num_minutes() > 5
        } else {
            false
        }
    }

    /// Cleanup stale sessions
    pub async fn cleanup_stale_sessions(&self) -> usize {
        let mut sessions = self.sessions.write().await;
        let now = chrono::Utc::now();
        let stale_threshold = chrono::Duration::minutes(5);

        let stale_ids: Vec<Uuid> = sessions
            .iter()
            .filter(|(_, session)| {
                let duration = now.signed_duration_since(session.last_activity);
                duration > stale_threshold
            })
            .map(|(id, _)| *id)
            .collect();

        let count = stale_ids.len();
        for id in stale_ids {
            sessions.remove(&id);
        }

        count
    }
}

impl Default for DefaultSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionManager for DefaultSessionManager {
    async fn register_connection(&self, session: WsSession) -> Result<(), WebSocketError> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_id, session);
        Ok(())
    }

    async fn remove_connection(&self, session_id: Uuid) -> Result<(), WebSocketError> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
        Ok(())
    }

    async fn get_session(&self, session_id: Uuid) -> Result<Option<WsSession>, WebSocketError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(&session_id).cloned())
    }

    async fn update_activity(&self, session_id: Uuid) -> Result<(), WebSocketError> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.update_activity();
            Ok(())
        } else {
            Err(WebSocketError::SessionNotFound(session_id))
        }
    }

    async fn subscribe(&self, session_id: Uuid, channel: String) -> Result<(), WebSocketError> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.add_subscription(channel);
            Ok(())
        } else {
            Err(WebSocketError::SessionNotFound(session_id))
        }
    }

    async fn unsubscribe(&self, session_id: Uuid, channel: &str) -> Result<(), WebSocketError> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.remove_subscription(channel);
            Ok(())
        } else {
            Err(WebSocketError::SessionNotFound(session_id))
        }
    }

    async fn broadcast(&self, channel: &str, message: WsMessage) -> Result<usize, WebSocketError> {
        let sessions = self.sessions.read().await;
        let mut count = 0;

        for session in sessions.values() {
            if session.is_subscribed(channel) {
                // In a real implementation, send message to session's WebSocket sender
                // For now, just count subscribers
                count += 1;
            }
        }

        Ok(count)
    }

    async fn send_to_session(&self, session_id: Uuid, _message: WsMessage) -> Result<(), WebSocketError> {
        let sessions = self.sessions.read().await;
        if sessions.contains_key(&session_id) {
            // In a real implementation, send message to session's WebSocket sender
            Ok(())
        } else {
            Err(WebSocketError::SessionNotFound(session_id))
        }
    }

    async fn list_sessions(&self) -> Result<Vec<WsSession>, WebSocketError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().cloned().collect())
    }

    async fn connection_count(&self) -> Result<usize, WebSocketError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.len())
    }

    async fn mark_connection_unhealthy(&self, session_id: Uuid) -> Result<(), WebSocketError> {
        // In a real implementation, this would mark the connection for cleanup
        // For now, just remove it
        self.remove_connection(session_id).await
    }
}

/// WebSocket event types for different channels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum WsEventType {
    AgentExecutionEvent {
        agent_id: Uuid,
        status: String,
    },
    WorkspaceRuntimeUpdate {
        workspace_id: Uuid,
        status: String,
    },
    IssueCommentEvent {
        issue_id: Uuid,
        comment_id: Uuid,
    },
    IssueStatusChange {
        issue_id: Uuid,
        old_status: String,
        new_status: String,
    },
    SecretRotated {
        secret_id: Uuid,
    },
}

/// Channel naming conventions
pub mod channels {
    use uuid::Uuid;

    pub fn company(company_id: Uuid) -> String {
        format!("company:{}", company_id)
    }

    pub fn issue(issue_id: Uuid) -> String {
        format!("issue:{}", issue_id)
    }

    pub fn agent(agent_id: Uuid) -> String {
        format!("agent:{}", agent_id)
    }

    pub fn workspace(workspace_id: Uuid) -> String {
        format!("workspace:{}", workspace_id)
    }

    pub fn user(user_id: Uuid) -> String {
        format!("user:{}", user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_manager_register_and_get() {
        let manager = DefaultSessionManager::new();
        let session_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let session = WsSession::new(session_id, company_id, None);
        manager.register_connection(session.clone()).await.unwrap();

        let retrieved = manager.get_session(session_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().session_id, session_id);
    }

    #[tokio::test]
    async fn test_session_manager_subscribe() {
        let manager = DefaultSessionManager::new();
        let session_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let session = WsSession::new(session_id, company_id, None);
        manager.register_connection(session).await.unwrap();

        manager.subscribe(session_id, "test-channel".to_string()).await.unwrap();

        let session = manager.get_session(session_id).await.unwrap().unwrap();
        assert!(session.is_subscribed("test-channel"));
    }

    #[tokio::test]
    async fn test_session_manager_broadcast() {
        let manager = DefaultSessionManager::new();
        let company_id = Uuid::new_v4();

        let session1 = WsSession::new(Uuid::new_v4(), company_id, None);
        let session2 = WsSession::new(Uuid::new_v4(), company_id, None);

        manager.register_connection(session1.clone()).await.unwrap();
        manager.register_connection(session2.clone()).await.unwrap();

        manager.subscribe(session1.session_id, "test-channel".to_string()).await.unwrap();
        manager.subscribe(session2.session_id, "test-channel".to_string()).await.unwrap();

        let message = WsMessage::Event {
            channel: "test-channel".to_string(),
            event: "test".to_string(),
            data: serde_json::json!({}),
        };

        let count = manager.broadcast("test-channel", message).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_session_manager_cleanup_stale() {
        let manager = DefaultSessionManager::new();
        let company_id = Uuid::new_v4();

        let mut session = WsSession::new(Uuid::new_v4(), company_id, None);
        session.last_activity = chrono::Utc::now() - chrono::Duration::minutes(10);

        manager.register_connection(session).await.unwrap();

        let cleaned = manager.cleanup_stale_sessions().await;
        assert_eq!(cleaned, 1);

        let count = manager.connection_count().await.unwrap();
        assert_eq!(count, 0);
    }
}
