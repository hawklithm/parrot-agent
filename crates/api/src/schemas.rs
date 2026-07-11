use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// CreateAgentHireSchema - Agent创建请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateAgentHireSchema {
    #[garde(length(min = 1, max = 100))]
    pub name: String,

    #[garde(skip)]
    pub role: Option<models::AgentRole>,

    #[garde(length(min = 1, max = 50))]
    pub adapter_type: String,

    #[garde(skip)]
    pub adapter_config: serde_json::Value,

    #[garde(skip)]
    pub runtime_config: Option<serde_json::Value>,

    #[garde(skip)]
    pub permissions: Option<models::AgentPermissions>,

    #[garde(range(min = 0))]
    pub budget_monthly_cents: Option<i32>,

    #[garde(skip)]
    pub reports_to: Option<Uuid>,
}

/// UpdateAgentSchema - Agent更新请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateAgentSchema {
    #[garde(length(min = 1, max = 100))]
    pub name: Option<String>,

    #[garde(skip)]
    pub role: Option<models::AgentRole>,

    #[garde(skip)]
    pub status: Option<models::AgentStatus>,

    #[garde(skip)]
    pub adapter_config: Option<serde_json::Value>,

    #[garde(skip)]
    pub runtime_config: Option<serde_json::Value>,

    #[garde(range(min = 0))]
    pub budget_monthly_cents: Option<i32>,

    #[garde(skip)]
    pub reports_to: Option<Uuid>,
}

/// TestAdapterEnvironmentSchema - 测试适配器环境请求验证
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TestAdapterEnvironmentSchema {
    #[garde(skip)]
    pub adapter_config: serde_json::Value,

    #[garde(skip)]
    pub environment_id: Option<String>,

    #[garde(skip)]
    pub with_lease: Option<bool>,

    #[garde(skip)]
    pub with_workspace: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_agent_hire_schema_validation() {
        let valid = CreateAgentHireSchema {
            name: "Test Agent".to_string(),
            role: Some(models::AgentRole::General),
            adapter_type: "process".to_string(),
            adapter_config: serde_json::json!({}),
            runtime_config: None,
            permissions: None,
            budget_monthly_cents: Some(10000),
            reports_to: None,
        };

        assert!(valid.validate(&()).is_ok());

        let invalid_name = CreateAgentHireSchema {
            name: "".to_string(),
            adapter_type: "process".to_string(),
            adapter_config: serde_json::json!({}),
            role: None,
            runtime_config: None,
            permissions: None,
            budget_monthly_cents: None,
            reports_to: None,
        };

        assert!(invalid_name.validate(&()).is_err());
    }

    #[test]
    fn test_update_agent_schema_validation() {
        let valid = UpdateAgentSchema {
            name: Some("Updated Name".to_string()),
            role: None,
            status: None,
            adapter_config: None,
            runtime_config: None,
            budget_monthly_cents: Some(20000),
            reports_to: None,
        };

        assert!(valid.validate(&()).is_ok());
    }
}
