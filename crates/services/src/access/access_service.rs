use async_trait::async_trait;
use uuid::Uuid;

use super::abac::{AccessDecision, Action, Actor};

#[async_trait]
pub trait AccessService: Send + Sync {
    async fn decide(
        &self,
        action: &Action,
        actor: &dyn Actor,
        resource: Option<&ResourceContext>,
    ) -> AccessDecision;

    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError>;

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    async fn assert_built_in_agents_enabled(
        &self,
        company_id: Uuid,
    ) -> Result<(), AccessError>;

    /// 批量过滤Agent列表（基于权限判定）
    async fn filter_agents_for_actor(
        &self,
        actor: &dyn Actor,
        agents: Vec<models::Agent>,
    ) -> Vec<models::Agent>;

    /// 为受限视图脱敏Agent配置
    fn redact_for_restricted_agent_view(&self, agent: &mut models::Agent);

    /// 脱敏事件载荷中的敏感信息
    fn redact_event_payload(&self, payload: &mut serde_json::Value);
}

#[derive(Debug, Clone)]
pub struct ResourceContext {
    pub resource_type: ResourceType,
    pub resource_id: Uuid,
    pub company_id: Uuid,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Agent,
    AgentConfig,
    Task,
    Company,
    BuiltInAgent,
    Routine,
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("Access denied: {0}")]
    Denied(String),

    #[error("Company access denied for company {company_id}")]
    CompanyAccessDenied { company_id: Uuid },

    #[error("Agent access denied for agent {agent_id}")]
    AgentAccessDenied { agent_id: Uuid },

    #[error("Permission required: {permission}")]
    PermissionRequired { permission: String },

    #[error("Built-in agents feature is not enabled for this company")]
    BuiltInAgentsNotEnabled,

    #[error("Internal error: {0}")]
    Internal(String),
}

pub struct DefaultAccessService;

impl DefaultAccessService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultAccessService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AccessService for DefaultAccessService {
    async fn decide(
        &self,
        action: &Action,
        actor: &dyn Actor,
        resource: Option<&ResourceContext>,
    ) -> AccessDecision {
        if actor.is_board_admin() || actor.is_instance_admin() {
            return AccessDecision::allow_with_reason("Admin access");
        }

        match action {
            Action::AgentsCreate => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            if actor.permissions().can_create_agents {
                                return AccessDecision::allow();
                            }
                            return AccessDecision::deny("Missing can_create_agents permission");
                        }
                        return AccessDecision::deny("Company mismatch");
                    }
                }
                AccessDecision::deny("No company context")
            }
            Action::AgentRead => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                        return AccessDecision::deny("Company mismatch");
                    }
                }
                AccessDecision::deny("No company context")
            }
            Action::AgentConfigUpdate | Action::AgentConfigRead => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                    }
                }
                AccessDecision::deny("Access denied")
            }
            Action::TasksAssign => {
                if actor.permissions().can_create_agents {
                    AccessDecision::allow()
                } else {
                    AccessDecision::deny("Missing can_create_agents permission")
                }
            }
            Action::AgentsDelete => {
                if let Some(res) = resource {
                    if let Some(actor_company) = actor.company_id() {
                        if res.company_id == actor_company {
                            return AccessDecision::allow();
                        }
                    }
                }
                AccessDecision::deny("Access denied")
            }
            Action::AgentsProvisionBuiltIn | Action::RoutinesControlBuiltIn => {
                if actor.is_board_admin() {
                    AccessDecision::allow()
                } else {
                    AccessDecision::deny("Board admin required")
                }
            }
        }
    }

    async fn assert_company_access(&self, actor: &dyn Actor, company_id: Uuid) -> Result<(), AccessError> {
        if actor.is_instance_admin() {
            return Ok(());
        }

        match actor.company_id() {
            Some(actor_company) if actor_company == company_id => Ok(()),
            _ => Err(AccessError::CompanyAccessDenied { company_id }),
        }
    }

    async fn assert_agent_read_allowed(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Agent,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentRead, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::AgentAccessDenied { agent_id })
        }
    }

    async fn assert_can_create_agents_for_company(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Company,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentsCreate, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agents:create".to_string(),
            })
        }
    }

    async fn assert_can_update_agent(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::AgentConfig,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentConfigUpdate, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agent_config:update".to_string(),
            })
        }
    }

    async fn assert_can_read_configurations(
        &self,
        actor: &dyn Actor,
        agent_id: Uuid,
        agent_company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, agent_company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::AgentConfig,
            resource_id: agent_id,
            company_id: agent_company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentConfigRead, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agent_config:read".to_string(),
            })
        }
    }

    async fn assert_can_provision_built_in_agents(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::BuiltInAgent,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::AgentsProvisionBuiltIn, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "agents:provision_built_in".to_string(),
            })
        }
    }

    async fn assert_can_control_built_in_routine(
        &self,
        actor: &dyn Actor,
        company_id: Uuid,
    ) -> Result<(), AccessError> {
        self.assert_company_access(actor, company_id).await?;

        let resource = ResourceContext {
            resource_type: ResourceType::Routine,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = self.decide(&Action::RoutinesControlBuiltIn, actor, Some(&resource)).await;
        if decision.allowed {
            Ok(())
        } else {
            Err(AccessError::PermissionRequired {
                permission: "routines:control_built_in".to_string(),
            })
        }
    }

    async fn assert_built_in_agents_enabled(
        &self,
        _company_id: Uuid,
    ) -> Result<(), AccessError> {
        // TODO: 实际实现需要检查公司配置中的实验特性开关
        // 目前默认返回启用
        Ok(())
    }

    async fn filter_agents_for_actor(
        &self,
        actor: &dyn Actor,
        agents: Vec<models::Agent>,
    ) -> Vec<models::Agent> {
        let mut filtered = Vec::new();

        for agent in agents {
            let resource = ResourceContext {
                resource_type: ResourceType::Agent,
                resource_id: agent.id,
                company_id: agent.company_id,
                metadata: std::collections::HashMap::new(),
            };

            let decision = self.decide(&Action::AgentRead, actor, Some(&resource)).await;
            if decision.allowed {
                filtered.push(agent);
            }
        }

        filtered
    }

    fn redact_for_restricted_agent_view(&self, agent: &mut models::Agent) {
        // 移除敏感配置字段
        agent.adapter_config = sqlx::types::Json(serde_json::json!({}));
        agent.runtime_config = sqlx::types::Json(serde_json::json!({}));
    }

    fn redact_event_payload(&self, payload: &mut serde_json::Value) {
        if let Some(obj) = payload.as_object_mut() {
            // 脱敏常见敏感字段
            let sensitive_keys = vec![
                "api_key",
                "apiKey",
                "secret",
                "password",
                "token",
                "accessToken",
                "refresh_token",
                "private_key",
                "privateKey",
                "credentials",
                "auth",
            ];

            for key in sensitive_keys {
                if obj.contains_key(key) {
                    obj.insert(key.to_string(), serde_json::json!("***REDACTED***"));
                }
            }

            // 递归处理嵌套对象
            for value in obj.values_mut() {
                if value.is_object() || value.is_array() {
                    self.redact_event_payload(value);
                }
            }
        } else if let Some(arr) = payload.as_array_mut() {
            for item in arr {
                self.redact_event_payload(item);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::abac::{AgentActor, AgentPermissions, TrustPreset, AuthorizationPolicy, UserActor};

    #[tokio::test]
    async fn test_admin_access() {
        let service = DefaultAccessService::new();
        let admin = UserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_board_admin: true,
            is_instance_admin: false,
        };

        let decision = service.decide(&Action::AgentsCreate, &admin, None).await;
        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_agent_create_permission() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let permissions = AgentPermissions {
            can_create_agents: true,
            can_create_skills: false,
            trust_preset: TrustPreset::Medium,
            authorization_policy: AuthorizationPolicy::RequireApproval,
        };

        let agent = AgentActor {
            agent_id: Uuid::new_v4(),
            company_id,
            permissions,
        };

        let resource = ResourceContext {
            resource_type: ResourceType::Company,
            resource_id: company_id,
            company_id,
            metadata: std::collections::HashMap::new(),
        };

        let decision = service.decide(&Action::AgentsCreate, &agent, Some(&resource)).await;
        assert!(decision.allowed);
    }

    #[tokio::test]
    async fn test_company_access_denied() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();
        let different_company = Uuid::new_v4();

        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_board_admin: false,
            is_instance_admin: false,
        };

        let result = service.assert_company_access(&user, different_company).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filter_agents_for_actor() {
        let service = DefaultAccessService::new();
        let company_id = Uuid::new_v4();

        let user = UserActor {
            user_id: Uuid::new_v4(),
            company_id,
            is_board_admin: false,
            is_instance_admin: false,
        };

        let agent1 = models::Agent {
            id: Uuid::new_v4(),
            company_id,
            name: "Agent 1".to_string(),
            role: models::AgentRole::General,
            status: models::AgentStatus::Idle,
            adapter_type: "process".to_string(),
            adapter_config: sqlx::types::Json(serde_json::json!({})),
            runtime_config: sqlx::types::Json(serde_json::json!({})),
            permissions: sqlx::types::Json(models::AgentPermissions::default()),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: None,
                built_in_key: None,
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: 0,
            reports_to: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let agent2 = models::Agent {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(), // Different company
            name: "Agent 2".to_string(),
            role: models::AgentRole::General,
            status: models::AgentStatus::Idle,
            adapter_type: "process".to_string(),
            adapter_config: sqlx::types::Json(serde_json::json!({})),
            runtime_config: sqlx::types::Json(serde_json::json!({})),
            permissions: sqlx::types::Json(models::AgentPermissions::default()),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: None,
                built_in_key: None,
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: 0,
            reports_to: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let agents = vec![agent1.clone(), agent2];
        let filtered = service.filter_agents_for_actor(&user, agents).await;

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, agent1.id);
    }

    #[test]
    fn test_redact_for_restricted_agent_view() {
        let service = DefaultAccessService::new();
        let mut agent = models::Agent {
            id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            name: "Test Agent".to_string(),
            role: models::AgentRole::General,
            status: models::AgentStatus::Idle,
            adapter_type: "claude_local".to_string(),
            adapter_config: sqlx::types::Json(serde_json::json!({
                "api_key": "secret-key",
                "model": "claude-opus-4"
            })),
            runtime_config: sqlx::types::Json(serde_json::json!({
                "env": {
                    "OPENAI_API_KEY": "sk-test"
                }
            })),
            permissions: sqlx::types::Json(models::AgentPermissions::default()),
            metadata: sqlx::types::Json(models::AgentMetadata {
                is_built_in: None,
                built_in_key: None,
                instructions_path: None,
                instructions_bundle: None,
            }),
            budget_monthly_cents: 10000,
            reports_to: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        service.redact_for_restricted_agent_view(&mut agent);

        assert_eq!(agent.adapter_config.0, serde_json::json!({}));
        assert_eq!(agent.runtime_config.0, serde_json::json!({}));
    }

    #[test]
    fn test_redact_event_payload() {
        let service = DefaultAccessService::new();
        let mut payload = serde_json::json!({
            "user": "john",
            "api_key": "secret123",
            "data": {
                "token": "token456",
                "value": "public"
            },
            "items": [
                {
                    "password": "pass789",
                    "name": "item1"
                }
            ]
        });

        service.redact_event_payload(&mut payload);

        assert_eq!(payload["api_key"], "***REDACTED***");
        assert_eq!(payload["data"]["token"], "***REDACTED***");
        assert_eq!(payload["items"][0]["password"], "***REDACTED***");
        assert_eq!(payload["user"], "john");
        assert_eq!(payload["data"]["value"], "public");
        assert_eq!(payload["items"][0]["name"], "item1");
    }
}
