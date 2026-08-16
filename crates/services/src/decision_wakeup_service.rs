use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 决策唤醒输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionWakeupInput {
    pub agent_id: Uuid,
    pub issue_id: Uuid,
    pub decision_id: Uuid,
    pub outcome: String,
}

/// 归档通知批次（用于retention服务）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveNotificationBatch {
    pub agent_id: Uuid,
    pub items: Vec<ArchiveNotificationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveNotificationItem {
    pub source_kind: String,
    pub source_id: String,
    pub issue_id: Uuid,
    pub archive_version: i32,
}

/// Heartbeat唤醒选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatWakeupOptions {
    pub source: String,
    pub trigger_detail: String,
    pub reason: String,
    pub payload: serde_json::Value,
}

/// 决策唤醒服务
/// 
/// 此服务负责在决策完成后唤醒origin agent，以及在attention items归档时通知agent。
/// 它作为decision service和heartbeat runtime之间的桥梁。
#[async_trait]
pub trait DecisionWakeupService: Send + Sync {
    /// 在决策完成后唤醒origin agent
    /// 
    /// 当决策达到终态（approved, rejected, completed等）时调用，
    /// 以唤醒负责该决策的agent继续工作。
    async fn wake_origin_agent_for_decision(
        &self,
        input: DecisionWakeupInput,
    ) -> Result<(), WakeupError>;

    /// 通知agent关于attention items的归档
    /// 
    /// 当多个attention items被归档时，批量通知相关的origin agent。
    async fn notify_origin_agent_for_archives(
        &self,
        batch: ArchiveNotificationBatch,
    ) -> Result<(), WakeupError>;
}

#[derive(Debug, thiserror::Error)]
pub enum WakeupError {
    #[error("Heartbeat runtime not available")]
    HeartbeatUnavailable,

    #[error("Agent not found: {0}")]
    AgentNotFound(Uuid),

    #[error("Wakeup failed: {0}")]
    WakeupFailed(String),

    #[error("Notification failed: {0}")]
    NotificationFailed(String),
}

/// 默认决策唤醒服务实现
/// 
/// 此实现连接到heartbeat runtime，如果runtime不可用则静默失败。
/// 这确保即使在heartbeat调度器禁用时，决策系统仍可正常工作。
pub struct DefaultDecisionWakeupService {
    heartbeat_enabled: bool,
    // TODO: 添加 heartbeat_service 依赖
}

impl DefaultDecisionWakeupService {
    /// 创建新的唤醒服务实例
    /// 
    /// `heartbeat_enabled`: 是否启用heartbeat runtime集成
    pub fn new(heartbeat_enabled: bool) -> Self {
        Self {
            heartbeat_enabled,
        }
    }

    /// 检查heartbeat runtime是否可用
    fn is_heartbeat_available(&self) -> bool {
        self.heartbeat_enabled
    }

    /// 构建决策完成的wakeup payload
    fn build_decision_wakeup_payload(input: &DecisionWakeupInput) -> serde_json::Value {
        serde_json::json!({
            "issueId": input.issue_id,
            "decisionId": input.decision_id,
            "outcome": input.outcome,
        })
    }

    /// 构建归档通知的wakeup payload
    fn build_archive_notification_payload(batch: &ArchiveNotificationBatch) -> serde_json::Value {
        let issue_ids: Vec<Uuid> = batch
            .items
            .iter()
            .map(|item| item.issue_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let archives: Vec<serde_json::Value> = batch
            .items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "sourceKind": item.source_kind,
                    "sourceId": item.source_id,
                    "archiveVersion": item.archive_version,
                })
            })
            .collect();

        serde_json::json!({
            "issueIds": issue_ids,
            "archives": archives,
        })
    }
}

impl Default for DefaultDecisionWakeupService {
    fn default() -> Self {
        Self::new(false)
    }
}

#[async_trait]
impl DecisionWakeupService for DefaultDecisionWakeupService {
    async fn wake_origin_agent_for_decision(
        &self,
        input: DecisionWakeupInput,
    ) -> Result<(), WakeupError> {
        // 如果heartbeat runtime不可用，静默返回成功
        if !self.is_heartbeat_available() {
            return Ok(());
        }

        // TODO: 调用heartbeat service的wakeup方法
        // let options = HeartbeatWakeupOptions {
        //     source: "automation".to_string(),
        //     triggdetail: "system".to_string(),
        //     reason: format!("decision_{}", input.outcome),
        //     payload: Self::build_decision_wakeup_payload(&input),
        // };
        // 
        // self.heartbeat_service
        //     .wake_agent(input.agent_id, options)
        //     .await
        //     .map_err(|e| WakeupError::WakeupFailed(e.to_string()))?;

        tracing::info!(
            agent_id = %input.agent_id,
            decision_id = %input.decision_id,
            outcome = %input.outcome,
            "Would wake origin agent for decision completion"
        );Ok(())
    }

    async fn notify_origin_agent_for_archives(
        &self,
        batch: ArchiveNotificationBatch,
    ) -> Result<(), WakeupError> {
        // 如果heartbeat runtime不可用，静默返回成功
        if !self.is_heartbeat_available() {
            return Ok(());
        }

        // TODO: 调用heartbeat service的wakeup方法
        // let options = HeartbeatWakeupOptions {
        //     source: "automation".to_string(),
        //     trigger_detail: "system".to_string(),
        //     reason: "attention_auto_archived".to_string(),
        //     payload: Self::build_archive_notification_payload(&batch),
        // };
        // 
        // self.heartbeat_service
        //     .wake_agent(batch.agent_id, options)
        //     .await
        //     .map_err(|e| WakeupError::NotificationFailed(e.to_string()))?;

        tracing::info!(
            agent_id = %batch.agent_id,
            item_count = batch.items.len(),
            "Would notify origin agent for archived attention items"
        );

        Ok(())
    }
}

/// 创建decision service的wakeup回调
/// 
/// 此函数返回一个可选的回调函数，仅在heartbeat runtime启用时才实际执行唤醒。
/// 这确保在调度器禁用时不会接受无法处理的唤醒请求。
pub fn create_decision_wake_origin_agent_callback(
    wakeup_service: Option<&dyn DecisionWakeupService>,
) -> Option<impl Fn(DecisionWakeupInput) + Send + Sync> {
    wakeup_service.map(|_service| {
        move |input: DecisionWakeupInput| {
            // TODO: 实现实际的异步调用
            tracing::debug!(
                agent_id = %input.agent_id,
                decision_id = %input.decision_id,
                "Decision wakeup callback triggered"
            );
        }
    })
}

/// 创建retention service的归档通知回调
/// 
/// 此函数返回一个可选的回调函数，仅在heartbeat runtime启用时才实际发送通知。
pub fn create_decision_retention_notify_origin_agent_callback(
    wakeup_service: Option<&dyn DecisionWakeupService>,
) -> Option<impl Fn(ArchiveNotificationBatch) + Send + Sync> {
    wakeup_service.map(|_service| {
        move |batch: ArchiveNotificationBatch| {
            // TODO: 实现实际的异步调用
            tracing::debug!(
                agent_id = %batch.agent_id,
                item_count = batch.items.len(),
                "Archive notification callback triggered"
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wake_origin_agent_disabled() {
        let service = DefaultDecisionWakeupService::new(false);
        let input = DecisionWakeupInput {
            agent_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            decision_id: Uuid::new_v4(),
            outcome: "approved".to_string(),
        };

        let result = service.wake_origin_agent_for_decision(input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notify_origin_agent_disabled() {
        let service = DefaultDecisionWakeupService::new(false);
        let batch = ArchiveNotificationBatch {
            agent_id: Uuid::new_v4(),
            items: vec![ArchiveNotificationItem {
                source_kind: "issue".to_string(),
                source_id: "test-1".to_string(),
                issue_id: Uuid::new_v4(),
                archive_version: 1,
            }],
        };

        let result = service.notify_origin_agent_for_archives(batch).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_decision_wakeup_payload() {
        let input = DecisionWakeupInput {
            agent_id: Uuid::new_v4(),
            issue_id: Uuid::new_v4(),
            decision_id: Uuid::new_v4(),
            outcome: "approved".to_string(),
        };

        let payload = DefaultDecisionWakeupService::build_decision_wakeup_payload(&input);
        assert_eq!(payload["outcome"], "approved");
        assert!(payload["issueId"].is_string());
        assert!(payload["decisionId"].is_string());
    }

    #[test]
    fn test_build_archive_notification_payload() {
        let issue_id = Uuid::new_v4();
        let batch = ArchiveNotificationBatch {
            agent_id: Uuid::new_v4(),
            items: vec![
                ArchiveNotificationItem {
                    source_kind: "issue".to_string(),
                    source_id: "test-1".to_string(),
                    issue_id,
                    archive_version: 1,
                },
                ArchiveNotificationItem {
                    source_kind: "approval".to_string(),
                    source_id: "test-2".to_string(),
                    issue_id,
                    archive_version: 1,
                },
            ],
        };

        let payload = DefaultDecisionWakeupService::build_archive_notification_payload(&batch);
        assert!(payload["issueIds"].is_array());
        assert!(payload["archives"].is_array());
        assert_eq!(payload["archives"].as_array().unwrap().len(), 2);
    }
}
