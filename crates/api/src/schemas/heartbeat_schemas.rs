use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 实例调度心跳 Agent 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSchedulerHeartbeatAgent {
    /// Agent ID
    pub id: Uuid,

    /// 公司 ID
    pub company_id: Uuid,

    /// 公司名称
    pub company_name: String,

    /// 公司 Issue 前缀
    pub company_issue_prefix: String,

    /// Agent 名称
    pub agent_name: String,

    /// Agent URL key
    pub agent_url_key: String,

    /// Agent 角色
    pub role: String,

    /// Agent 标题
    pub title: Option<String>,

    /// Agent 状态
    pub status: String,

    /// 适配器类型
    pub adapter_type: String,

    /// 心跳间隔（秒）
    pub interval_sec: i32,

    /// 心跳是否启用
    pub heartbeat_enabled: bool,

    /// 调度器是否活跃
    pub scheduler_active: bool,

    /// 最后心跳时间
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 调度器心跳策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerHeartbeatPolicy {
    /// 是否启用
    pub enabled: bool,

    /// 间隔（秒）
    pub interval_sec: i32,
}

impl Default for SchedulerHeartbeatPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_sec: 0,
        }
    }
}

/// 从 runtimeConfig 中解析心跳策略
pub fn parse_scheduler_heartbeat_policy(runtime_config: &serde_json::Value) -> SchedulerHeartbeatPolicy {
    let heartbeat = runtime_config
        .as_object()
        .and_then(|obj| obj.get("heartbeat"))
        .and_then(|v| v.as_object());

    if let Some(hb) = heartbeat {
        let enabled = hb
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let interval_sec = hb
            .get("intervalSec")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0) as i32;

        SchedulerHeartbeatPolicy {
            enabled,
            interval_sec,
        }
    } else {
        SchedulerHeartbeatPolicy::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_heartbeat_policy_enabled() {
        let config = json!({
            "heartbeat": {
                "enabled": true,
                "intervalSec": 300
            }
        });

        let policy = parse_scheduler_heartbeat_policy(&config);
        assert!(policy.enabled);
        assert_eq!(policy.interval_sec, 300);
    }

    #[test]
    fn test_parse_heartbeat_policy_disabled() {
        let config = json!({
            "heartbeat": {
                "enabled": false,
                "intervalSec": 600
            }
        });

        let policy = parse_scheduler_heartbeat_policy(&config);
        assert!(!policy.enabled);
        assert_eq!(policy.interval_sec, 600);
    }

    #[test]
    fn test_parse_heartbeat_policy_missing() {
        let config = json!({
            "someOtherConfig": "value"
        });

        let policy = parse_scheduler_heartbeat_policy(&config);
        assert!(!policy.enabled);
        assert_eq!(policy.interval_sec, 0);
    }

    #[test]
    fn test_parse_heartbeat_policy_negative_interval() {
        let config = json!({
            "heartbeat": {
                "enabled": true,
                "intervalSec": -100
            }
        });

        let policy = parse_scheduler_heartbeat_policy(&config);
        assert!(policy.enabled);
        assert_eq!(policy.interval_sec, 0);
    }

    #[test]
    fn test_instance_scheduler_heartbeat_agent_serialization() {
        let agent = InstanceSchedulerHeartbeatAgent {
            id: Uuid::nil(),
            company_id: Uuid::nil(),
            company_name: "Test Company".to_string(),
            company_issue_prefix: "TEST".to_string(),
            agent_name: "Scheduler Agent".to_string(),
            agent_url_key: "scheduler-agent".to_string(),
            role: "ceo".to_string(),
            title: Some("Chief Executive Officer".to_string()),
            status: "active".to_string(),
            adapter_type: "anthropic".to_string(),
            interval_sec: 300,
            heartbeat_enabled: true,
            scheduler_active: true,
            last_heartbeat_at: None,
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("schedulerActive"));
        assert!(json.contains("heartbeatEnabled"));
        assert!(json.contains("intervalSec"));
    }
}
