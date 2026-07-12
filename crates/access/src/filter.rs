use models::Agent;
use uuid::Uuid;

use super::models::{Action, Actor};
use super::service::{AccessService, Resource, ResourceType};

/// 为访问主体过滤 Agent 列表
pub async fn filter_agents_for_actor<S: AccessService>(
    service: &S,
    actor: &dyn Actor,
    agents: Vec<Agent>,
) -> Vec<Agent> {
    let mut filtered = Vec::new();

    for agent in agents {
        let resource = Resource {
            resource_type: ResourceType::Agent,
            resource_id: agent.id,
            company_id: agent.company_id,
            issue_context: None,
        };

        let decision = service.decide(Action::AgentRead, actor, Some(&resource)).await;

        if decision.allowed {
            filtered.push(agent);
        }
    }

    filtered
}

/// 脱敏 Agent 视图（移除敏感配置）
pub fn redact_for_restricted_agent_view(mut agent: Agent) -> Agent {
    // 移除 adapter_config
    agent.adapter_config = sqlx::types::Json(serde_json::json!({}));

    // 移除 runtime_config
    agent.runtime_config = sqlx::types::Json(serde_json::json!({}));

    agent
}

/// 脱敏事件负载（通用敏感信息过滤）
pub fn redact_event_payload(payload: &mut serde_json::Value) {
    if let Some(obj) = payload.as_object_mut() {
        // 移除敏感字段
        let sensitive_keys = vec![
            "api_key",
            "secret",
            "password",
            "token",
            "credentials",
            "adapter_config",
            "runtime_config",
        ];

        for key in sensitive_keys {
            if obj.contains_key(key) {
                obj.insert(key.to_string(), serde_json::json!("[REDACTED]"));
            }
        }

        // 递归处理嵌套对象
        for (_key, value) in obj.iter_mut() {
            if value.is_object() {
                redact_event_payload(value);
            } else if let Some(arr) = value.as_array_mut() {
                for item in arr.iter_mut() {
                    if item.is_object() {
                        redact_event_payload(item);
                    }
                }
            }
        }
    }
}

/// 检查 Actor 是否可以读取完整配置
pub fn can_read_full_config(actor: &dyn Actor, agent: &Agent) -> bool {
    // 如果是同一个 Agent 或者有配置读取权限
    if let Some(actor_agent_id) = actor.agent_id() {
        if actor_agent_id == agent.id {
            return true;
        }
    }

    // 检查是否有 agent_config:read 权限
    actor.has_permission(Action::AgentConfigRead)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentActor, UserActor};
    use crate::service::DefaultAccessService;
    use models::{AgentRole, AgentStatus, AgentPermissions, AgentMetadata};
    use sqlx::types::Json;
    use chrono::Utc;

    fn create_test_agent(company_id: Uuid) -> Agent {
        Agent {
            id: Uuid::new_v4(),
            company_id,
            name: "Test Agent".to_string(),
            role: AgentRole::General,
            status: AgentStatus::Idle,
            adapter_type: "process".to_string(),
            adapter_config: Json(serde_json::json!({"api_key": "secret-key"})),
            runtime_config: Json(serde_json::json!({"env": "production"})),
            permissions: Json(AgentPermissions::default()),
            metadata: Json(AgentMetadata { is_built_in: None, built_in_key: None }),
            budget_monthly_cents: 0,
            reports_to: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_filter_agents() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_admin: false,
        };

        let agents = vec![
            create_test_agent(company_id),
            create_test_agent(company_id),
            create_test_agent(Uuid::new_v4()), // Different company
        ];

        let filtered = filter_agents_for_actor(&service, &user, agents).await;
        assert_eq!(filtered.len(), 2); // Only same-company agents
    }

    #[test]
    fn test_redact_agent_view() {
        let company_id = Uuid::new_v4();
        let agent = create_test_agent(company_id);

        let redacted = redact_for_restricted_agent_view(agent.clone());

        assert_eq!(redacted.adapter_config.0, serde_json::json!({}));
        assert_eq!(redacted.runtime_config.0, serde_json::json!({}));
        assert_eq!(redacted.name, agent.name); // Other fields unchanged
    }

    #[test]
    fn test_redact_event_payload() {
        let mut payload = serde_json::json!({
            "agent_id": "123",
            "api_key": "secret-123",
            "adapter_config": {"key": "value"},
            "metadata": {
                "password": "secret-password",
                "public_field": "visible"
            }
        });

        redact_event_payload(&mut payload);

        assert_eq!(payload["api_key"], "[REDACTED]");
        assert_eq!(payload["adapter_config"], "[REDACTED]");
        assert_eq!(payload["metadata"]["password"], "[REDACTED]");
        assert_eq!(payload["metadata"]["public_field"], "visible");
    }

    #[test]
    fn test_can_read_full_config() {
        let company_id = Uuid::new_v4();
        let agent = create_test_agent(company_id);

        // Same agent can read its own config
        let same_agent_actor = AgentActor {
            agent_id: agent.id,
            company_id,
            permissions: serde_json::json!({}),
        };
        assert!(can_read_full_config(&same_agent_actor, &agent));

        // Different agent without permission cannot read
        let different_agent_actor = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id,
            permissions: serde_json::json!({}),
        };
        assert!(!can_read_full_config(&different_agent_actor, &agent));
    }
}
