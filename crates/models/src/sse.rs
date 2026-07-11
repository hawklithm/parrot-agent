use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// SSE event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SseEventType {
    Message,
    Open,
    Close,
    Error,
    Heartbeat,
}

/// SSE frame structure
#[derive(Debug, Clone, Serialize)]
pub struct SseFrame {
    /// Event name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,

    /// Event data (JSON serialized)
    pub data: String,

    /// Event ID for resumption (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Retry interval in milliseconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
}

impl SseFrame {
    /// Create a new SSE frame with data only
    pub fn new(data: String) -> Self {
        Self {
            event: None,
            data,
            id: None,
            retry: None,
        }
    }

    /// Create an SSE frame with event type and data
    pub fn with_event(event: String, data: String) -> Self {
        Self {
            event: Some(event),
            data,
            id: None,
            retry: None,
        }
    }

    /// Format as SSE protocol text
    pub fn to_sse_text(&self) -> String {
        let mut lines = Vec::new();

        if let Some(event) = &self.event {
            lines.push(format!("event: {}", event));
        }

        if let Some(id) = &self.id {
            lines.push(format!("id: {}", id));
        }

        if let Some(retry) = self.retry {
            lines.push(format!("retry: {}", retry));
        }

        // Data can be multi-line
        for line in self.data.lines() {
            lines.push(format!("data: {}", line));
        }

        // SSE frames end with double newline
        lines.push(String::new());
        lines.push(String::new());

        lines.join("\n")
    }
}

/// SSE subscription context
#[derive(Debug, Clone)]
pub struct SseSubscription {
    pub company_id: Uuid,
    pub actor_id: Uuid,
    pub channel: String,
    pub last_event_id: Option<String>,
}

/// SSE event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SseEvent {
    pub event_type: SseEventType,
    pub channel: String,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
