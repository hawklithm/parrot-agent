use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Event trait - 所有事件必须实现此trait
#[async_trait]
pub trait Event: Send + Sync {
    /// 事件类型标识符
    fn event_type(&self) -> &str;

    /// 事件时间戳
    fn timestamp(&self) -> DateTime<Utc>;

    /// 事件载荷（JSON格式）
    fn payload(&self) -> JsonValue;

    /// 事件关联的company_id（用于权限过滤）
    fn company_id(&self) -> Uuid;
}

/// EventHandler trait - 事件处理器
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// 处理事件
    async fn handle_event(&self, event: Arc<dyn Event>) -> Result<(), EventHandlerError>;

    /// Handler标识符（用于去重和日志）
    fn handler_id(&self) -> &str;
}

/// EventBus trait - 事件总线核心接口
#[async_trait]
pub trait EventBus: Send + Sync {
    /// 发布事件到所有订阅者
    async fn publish(&self, event: Arc<dyn Event>) -> Result<(), EventBusError>;

    /// 订阅指定类型的事件
    async fn subscribe(
        &self,
        event_type: &str,
        handler: Arc<dyn EventHandler>,
    ) -> Result<(), EventBusError>;

    /// 取消订阅
    async fn unsubscribe(&self, event_type: &str, handler_id: &str) -> Result<(), EventBusError>;

    /// 获取指定事件类型的订阅者数量
    async fn subscriber_count(&self, event_type: &str) -> usize;
}

/// 事件处理错误
#[derive(Debug, thiserror::Error)]
pub enum EventHandlerError {
    #[error("Handler execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Handler timeout: {0}")]
    Timeout(String),
}

/// 事件总线错误
#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("Failed to publish event: {0}")]
    PublishFailed(String),

    #[error("Failed to subscribe: {0}")]
    SubscribeFailed(String),

    #[error("Failed to unsubscribe: {0}")]
    UnsubscribeFailed(String),
}

// ==================== 标准事件类型 ====================

/// Issue事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueEvent {
    pub issue_id: Uuid,
    pub company_id: Uuid,
    pub action: IssueEventAction,
    pub timestamp: DateTime<Utc>,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueEventAction {
    Created,
    Updated,
    Assigned,
    Released,
    Completed,
    Commented,
}

#[async_trait]
impl Event for IssueEvent {
    fn event_type(&self) -> &str {
        "issue"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn payload(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn company_id(&self) -> Uuid {
        self.company_id
    }
}

/// Approval事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalEvent {
    pub approval_id: Uuid,
    pub company_id: Uuid,
    pub action: ApprovalEventAction,
    pub timestamp: DateTime<Utc>,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalEventAction {
    Requested,
    Approved,
    Rejected,
    Cancelled,
}

#[async_trait]
impl Event for ApprovalEvent {
    fn event_type(&self) -> &str {
        "approval"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn payload(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn company_id(&self) -> Uuid {
        self.company_id
    }
}

/// Routine事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineEvent {
    pub routine_id: Uuid,
    pub company_id: Uuid,
    pub action: RoutineEventAction,
    pub timestamp: DateTime<Utc>,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineEventAction {
    Created,
    Triggered,
    Completed,
    Failed,
    Paused,
    Resumed,
}

#[async_trait]
impl Event for RoutineEvent {
    fn event_type(&self) -> &str {
        "routine"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn payload(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn company_id(&self) -> Uuid {
        self.company_id
    }
}

/// Agent事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub action: AgentEventAction,
    pub timestamp: DateTime<Utc>,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEventAction {
    Hired,
    Configured,
    Started,
    Paused,
    Terminated,
    Reassigned,
}

#[async_trait]
impl Event for AgentEvent {
    fn event_type(&self) -> &str {
        "agent"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn payload(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn company_id(&self) -> Uuid {
        self.company_id
    }
}

/// Environment事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentEvent {
    pub environment_id: Uuid,
    pub company_id: Uuid,
    pub action: EnvironmentEventAction,
    pub timestamp: DateTime<Utc>,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentEventAction {
    Provisioned,
    Leased,
    Released,
    Deleted,
    HealthCheckFailed,
}

#[async_trait]
impl Event for EnvironmentEvent {
    fn event_type(&self) -> &str {
        "environment"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn payload(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }

    fn company_id(&self) -> Uuid {
        self.company_id
    }
}

// ==================== InMemoryEventBus实现 ====================

/// 内存事件总线实现（使用RwLock + HashMap）
pub struct InMemoryEventBus {
    /// 订阅关系：event_type -> Vec<Arc<dyn EventHandler>>
    subscribers: Arc<RwLock<HashMap<String, Vec<Arc<dyn EventHandler>>>>>,
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
    async fn publish(&self, event: Arc<dyn Event>) -> Result<(), EventBusError> {
        let event_type = event.event_type().to_string();
        let subscribers = self.subscribers.read().await;

        let handlers = subscribers
            .get(&event_type)
            .map(|h| h.clone())
            .unwrap_or_default();

        drop(subscribers); // 释放读锁

        // 并行执行所有handler（不阻塞）
        let mut tasks = Vec::new();
        for handler in handlers {
            let event_clone = Arc::clone(&event);
            let task = tokio::spawn(async move {
                handler.handle_event(event_clone).await
            });
            tasks.push(task);
        }

        // 等待所有handler完成（收集错误但不中断）
        let mut errors = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("Task join error: {}", e)),
            }
        }

        if !errors.is_empty() {
            return Err(EventBusError::PublishFailed(format!(
                "{} handlers failed: {}",
                errors.len(),
                errors.join(", ")
            )));
        }

        Ok(())
    }

    async fn subscribe(
        &self,
        event_type: &str,
        handler: Arc<dyn EventHandler>,
    ) -> Result<(), EventBusError> {
        let mut subscribers = self.subscribers.write().await;

        let handlers = subscribers
            .entry(event_type.to_string())
            .or_insert_with(Vec::new);

        // 检查是否已订阅（防止重复）
        let handler_id = handler.handler_id();
        if handlers.iter().any(|h| h.handler_id() == handler_id) {
            return Err(EventBusError::SubscribeFailed(format!(
                "Handler {} already subscribed to {}",
                handler_id, event_type
            )));
        }

        handlers.push(handler);
        Ok(())
    }

    async fn unsubscribe(&self, event_type: &str, handler_id: &str) -> Result<(), EventBusError> {
        let mut subscribers = self.subscribers.write().await;

        if let Some(handlers) = subscribers.get_mut(event_type) {
            let original_len = handlers.len();
            handlers.retain(|h| h.handler_id() != handler_id);

            if handlers.len() == original_len {
                return Err(EventBusError::UnsubscribeFailed(format!(
                    "Handler {} not found for event type {}",
                    handler_id, event_type
                )));
            }
        } else {
            return Err(EventBusError::UnsubscribeFailed(format!(
                "No subscribers for event type {}",
                event_type
            )));
        }

        Ok(())
    }

    async fn subscriber_count(&self, event_type: &str) -> usize {
        let subscribers = self.subscribers.read().await;
        subscribers
            .get(event_type)
            .map(|h| h.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_issue_event_creation() {
        let event = IssueEvent {
            issue_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            action: IssueEventAction::Created,
            timestamp: Utc::now(),
            metadata: json!({"reason": "test"}),
        };

        assert_eq!(event.event_type(), "issue");
        assert_eq!(event.action, IssueEventAction::Created);
    }

    #[test]
    fn test_agent_event_serialization() {
        let event = AgentEvent {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            action: AgentEventAction::Hired,
            timestamp: Utc::now(),
            metadata: json!({}),
        };

        let payload = event.payload();
        assert!(payload.is_object());
    }
}
