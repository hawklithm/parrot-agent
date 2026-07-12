use async_trait::async_trait;
use dashmap::DashMap;
use models::{SseEvent, SseFrame as ModelSseFrame, SseSubscription, SseEventType};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SseError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// SSE frame structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseFrame {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

impl SseFrame {
    pub fn new(data: String) -> Self {
        Self {
            event: None,
            data,
            id: None,
            retry: None,
        }
    }

    pub fn with_event(mut self, event: String) -> Self {
        self.event = Some(event);
        self
    }

    pub fn with_id(mut self, id: String) -> Self {
        self.id = Some(id);
        self
    }

    /// Format frame as SSE text
    pub fn format(&self) -> String {
        let mut output = String::new();

        if let Some(ref event) = self.event {
            output.push_str(&format!("event: {}\n", event));
        }

        if let Some(ref id) = self.id {
            output.push_str(&format!("id: {}\n", id));
        }

        if let Some(retry) = self.retry {
            output.push_str(&format!("retry: {}\n", retry));
        }

        // Handle multi-line data
        for line in self.data.lines() {
            output.push_str(&format!("data: {}\n", line));
        }

        output.push('\n');
        output
    }
}

/// SSE stream event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SseStreamEvent {
    Log {
        stream: String,
        chunk: String,
    },
    Delta {
        text: String,
        index: Option<usize>,
    },
    Terminal {
        status: String,
        exit_code: Option<i32>,
        message: Option<String>,
    },
}

impl SseStreamEvent {
    pub fn to_frame(&self) -> SseFrame {
        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string());

        match self {
            SseStreamEvent::Log { .. } => SseFrame::new(data).with_event("log".to_string()),
            SseStreamEvent::Delta { .. } => SseFrame::new(data).with_event("message.delta".to_string()),
            SseStreamEvent::Terminal { .. } => SseFrame::new(data).with_event("run.complete".to_string()),
        }
    }
}

/// Parse SSE frames from a text buffer
///
/// Returns: (parsed_frames, remaining_buffer)
pub fn parse_sse_frames(buffer: &str) -> (Vec<SseFrame>, String) {
    let normalized = buffer.replace("\r\n", "\n");
    let mut frames = Vec::new();
    let mut current_frame = SseFrame {
        event: None,
        data: String::new(),
        id: None,
        retry: None,
    };
    let mut data_lines = Vec::new();
    let mut pos = 0;

    for line in normalized.lines() {
        pos += line.len() + 1; // +1 for newline

        // Skip comments
        if line.starts_with(':') {
            continue;
        }

        // Empty line marks end of frame
        if line.is_empty() {
            if !data_lines.is_empty() {
                current_frame.data = data_lines.join("\n");
                frames.push(current_frame.clone());

                // Reset for next frame
                current_frame = SseFrame {
                    event: None,
                    data: String::new(),
                    id: None,
                    retry: None,
                };
                data_lines.clear();
            }
            continue;
        }

        // Parse field
        if let Some(colon_pos) = line.find(':') {
            let field = &line[..colon_pos];
            let value = line[colon_pos + 1..].trim_start();

            match field {
                "event" => current_frame.event = Some(value.to_string()),
                "data" => data_lines.push(value.to_string()),
                "id" => current_frame.id = Some(value.to_string()),
                "retry" => {
                    if let Ok(retry_val) = value.parse::<u64>() {
                        current_frame.retry = Some(retry_val);
                    }
                }
                _ => {} // Ignore unknown fields
            }
        }
    }

    // Return remaining buffer (incomplete frame)
    let remaining = if pos < normalized.len() {
        normalized[pos..].to_string()
    } else {
        String::new()
    };

    (frames, remaining)
}

/// Crital headers that should be redacted in logs
pub const CRITICAL_HEADERS: &[&str] = &[
    "authorization",
    "x-api-key",
    "x-auth-token",
    "cookie",
    "set-cookie",
    "secret",
    "password",
    "api_key",
    "apikey",
    "access_token",
    "bearer",
];

/// Sanitize sensitive information from text
pub fn sanitize_sensitive_text(text: &str) -> String {
    let mut sanitized = text.to_string();

    // Redact common credential patterns
    let patterns = vec![
        (r#"api_key\s*=\s*['"]?([^'"\s]+)"#, "api_key=***REDACTED***"),
        (r#"apikey\s*=\s*['"]?([^'"\s]+)"#, "apikey=***REDACTED***"),
        (r#"token\s*=\s*['"]?([^'"\s]+)"#, "token=***REDACTED***"),
        (r#"password\s*=\s*['"]?([^'"\s]+)"#, "password=***REDACTED***"),
        (r#"secret\s*=\s*['"]?([^'"\s]+)"#, "secret=***REDACTED***"),
        (r"Bearer\s+([^\s]+)", "Bearer ***REDACTED***"),
        (r"Basic\s+([^\s]+)", "Basic ***REDACTED***"),
    ];

    for (pattern, replacement) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            sanitized = re.replace_all(&sanitized, replacement).to_string();
        }
    }

    sanitized
}

/// Redact sensitive data for logging
pub fn redact_for_log(data: &str) -> String {
    sanitize_sensitive_text(data)
}

/// SSE stream manager
pub struct SseStreamManager {
    active_streams: HashSet<String>,
}

impl SseStreamManager {
    pub fn new() -> Self {
        Self {
            active_streams: HashSet::new(),
        }
    }

    /// Register a new stream
    pub fn register_stream(&mut self, stream_id: String) {
        self.active_streams.insert(stream_id);
    }

    /// Unregister a stream
    pub fn unregister_stream(&mut self, stream_id: &str) {
        self.active_streams.remove(stream_id);
    }

    /// Check if stream is active
    pub fn is_stream_active(&self, stream_id: &str) -> bool {
        self.active_streams.contains(stream_id)
    }

    /// Count active streams
    pub fn active_count(&self) -> usize {
        self.active_streams.len()
    }

    /// Clear all streams
    pub fn clear(&mut self) {
        self.active_streams.clear();
    }
}

impl Default for SseStreamManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time server-sent-events service.
///
/// Implementations manage per-channel broadcast topics and allow subscribers to
/// receive a stream of [`SseFrame`]s.
#[async_trait]
pub trait SseService: Send + Sync {
    /// Subscribe to a channel, returning a receiver that yields frames.
    async fn subscribe(
        &self,
        subscription: SseSubscription,
    ) -> Result<broadcast::Receiver<ModelSseFrame>, String>;

    /// Publish an event to a company/channel.
    async fn publish(
        &self,
        company_id: Uuid,
        channel: &str,
        event: SseEvent,
    ) -> Result<(), String>;

    /// Number of active subscribers for a company/channel.
    async fn subscriber_count(&self, company_id: Uuid, channel: &str) -> u64;
}

/// In-memory broadcast-backed implementation of [`SseService`].
///
/// Each channel maps to a `tokio` broadcast topic. Subscribers receive frames
/// published after they subscribe; late frames are simply dropped.
#[derive(Debug, Default)]
pub struct InMemorySseService {
    channels: DashMap<String, broadcast::Sender<ModelSseFrame>>,
}

impl InMemorySseService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channels: DashMap::new(),
        })
    }

    fn channel_key(company_id: Uuid, channel: &str) -> String {
        format!("{}:{}", company_id, channel)
    }

    fn sender_for(&self, key: &str) -> broadcast::Sender<ModelSseFrame> {
        if let Some(existing) = self.channels.get(key) {
            return existing.clone();
        }
        let (tx, _rx) = broadcast::channel(256);
        // Only insert if another writer hasn't done so concurrently.
        match self.channels.entry(key.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(o) => o.get().clone(),
            dashmap::mapref::entry::Entry::Vacant(v) => v.insert(tx).clone(),
        }
    }
}

#[async_trait]
impl SseService for InMemorySseService {
    async fn subscribe(
        &self,
        subscription: SseSubscription,
    ) -> Result<broadcast::Receiver<ModelSseFrame>, String> {
        let key = Self::channel_key(subscription.company_id, &subscription.channel);
        let sender = self.sender_for(&key);
        Ok(sender.subscribe())
    }

    async fn publish(
        &self,
        company_id: Uuid,
        channel: &str,
        event: SseEvent,
    ) -> Result<(), String> {
        let key = Self::channel_key(company_id, channel);
        let sender = self.sender_for(&key);

        let event_name = match event.event_type {
            SseEventType::Message => "message".to_string(),
            SseEventType::Open => "open".to_string(),
            SseEventType::Close => "close".to_string(),
            SseEventType::Error => "error".to_string(),
            SseEventType::Heartbeat => "heartbeat".to_string(),
        };
        let data = serde_json::to_string(&event.payload)
            .unwrap_or_else(|_| "{}".to_string());
        let frame = ModelSseFrame::with_event(event_name, data);

        // Ignore send errors when there are no active subscribers.
        let _ = sender.send(frame);
        Ok(())
    }

    async fn subscriber_count(&self, company_id: Uuid, channel: &str) -> u64 {
        let key = Self::channel_key(company_id, channel);
        self.channels
            .get(&key)
            .map(|s| s.receiver_count() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_frame_format() {
        let frame = SseFrame::new("test data".to_string())
            .with_event("test".to_string())
            .with_id("123".to_string());

        let formatted = frame.format();
        assert!(formatted.contains("event: test\n"));
        assert!(formatted.contains("id: 123\n"));
        assert!(formatted.contains("data: test data\n"));
        assert!(formatted.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_frame_multiline_data() {
        let frame = SseFrame::new("line1\nline2\nline3".to_string());
        let formatted = frame.format();

        assert!(formatted.contains("data: line1\n"));
        assert!(formatted.contains("data: line2\n"));
        assert!(formatted.contains("data: line3\n"));
    }

    #[test]
    fn test_parse_sse_frames() {
        let input = "event: test\ndata: hello\n\ndata: world\n\n";
        let (frames, remaining) = parse_sse_frames(input);

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].event, Some("test".to_string()));
        assert_eq!(frames[0].data, "hello");
        assert_eq!(frames[1].data, "world");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_parse_sse_frames_incomplete() {
        let input = "event: test\ndata: hello";
        let (frames, remaining) = parse_sse_frames(input);

        assert_eq!(frames.len(), 0);
        assert!(remaining.contains("event: test"));
    }

    #[test]
    fn test_parse_sse_frames_with_comments() {
        let input = ": this is a comment\ndata: hello\n\n";
        let (frames, _) = parse_sse_frames(input);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data, "hello");
    }

    #[test]
    fn test_sanitize_sensitive_text() {
        let text = "api_key=secret123 password='mypass' Bearer abc123";
        let sanitized = sanitize_sensitive_text(text);

        assert!(sanitized.contains("api_key=***REDACTED***"));
        assert!(sanitized.contains("password=***REDACTED***"));
        assert!(sanitized.contains("Bearer ***REDACTED***"));
        assert!(!sanitized.contains("secret123"));
        assert!(!sanitized.contains("mypass"));
        assert!(!sanitized.contains("abc123"));
    }

    #[test]
    fn test_sse_stream_event_to_frame() {
        let event = SseStreamEvent::Log {
            stream: "stdout".to_string(),
            chunk: "test output".to_string(),
        };

        let frame = event.to_frame();
        assert_eq!(frame.event, Some("log".to_string()));
        assert!(frame.data.contains("stdout"));
        assert!(frame.data.contains("test output"));
    }

    #[test]
    fn test_sse_stream_manager() {
        let mut manager = SseStreamManager::new();

        manager.register_stream("stream1".to_string());
        manager.register_stream("stream2".to_string());

        assert_eq!(manager.active_count(), 2);
        assert!(manager.is_stream_active("stream1"));

        manager.unregister_stream("stream1");
        assert_eq!(manager.active_count(), 1);
        assert!(!manager.is_stream_active("stream1"));

        manager.clear();
        assert_eq!(manager.active_count(), 0);
    }
}
