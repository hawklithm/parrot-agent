/// Agent Invokability Service
/// 
/// Agent可调用性判断服务

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum InvokabilityError {
    #[error("invocation not allowed: {0}")]
    NotAllowed(String),
    
    #[error("agent not found: {0}")]
    AgentNotFound(Uuid),
}

pub type InvokabilityResult<T> = Result<T, InvokabilityError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokabilityCondition {
    pub condition_type: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

impl InvokabilityCondition {
    pub fn evaluate(&self, context: &InvokabilityContext) -> bool {
        match self.condition_type.as_str() {
            "agent_status" => {
                if let Some(required) = self.parameters.get("status").and_then(|v| v.as_str()) {
                    context.agent_status == required
                } else {
                    false
                }
            }
            "caller_role" => {
                if let Some(required) = self.parameters.get("role").and_then(|v| v.as_str()) {
                    context.caller_role.as_deref() == Some(required)
                } else {
                    false
                }
            }
            "time_range" => {
                // 简化实现 - 实际应该检查时间范围
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokabilityRule {
    pub agent_id: Uuid,
    pub allowed: bool,
    pub conditions: Vec<InvokabilityCondition>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InvokabilityContext {
    pub agent_status: String,
    pub caller_id: Option<Uuid>,
    pub caller_role: Option<String>,
    pub workspace_id: Option<Uuid>,
}

pub struct AgentInvokabilityService {
    rules: HashMap<Uuid, Vec<InvokabilityRule>>,
}

impl AgentInvokabilityService {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }
    
    /// 添加可调用性规则
    pub fn add_rule(&mut self, rule: InvokabilityRule) {
        self.rules.entry(rule.agent_id)
            .or_insert_with(Vec::new)
            .push(rule);
    }
    
    /// 移除规则
    pub fn remove_rules(&mut self, agent_id: &Uuid) {
        self.rules.remove(agent_id);
    }
    
    /// 检查agent是否可调用
    pub fn is_invokable(&self, agent_id: Uuid, context: &InvokabilityContext) -> InvokabilityResult<bool> {
      // 获取规则
        let rules = match self.rules.get(&agent_id) {
            Some(r) => r,
            None => return Ok(true), // 无规则默认允许
        };
        
        // 评估所有规则
        for rule in rules {
            // 检查条件
            let conditions_met = rule.conditions.iter().all(|c| c.evaluate(context));
            
            if conditions_met {
                if !rule.allowed {
                    return Err(InvokabilityError::NotAllowed(
                        rule.reason.clone().unwrap_or_else(|| "invocation not allowed".to_string())
                    ));
                }
            }
        }
        
        Ok(true)
    }
    
    /// 获取不可调用原因
    pub fn get_invokability_reason(&self, agent_id: Uuid, context: &InvokabilityContext) -> Option<String> {
        match self.is_invokable(agent_id, context) {
            Ok(_) => None,
            Err(InvokabilityError::NotAllowed(reason)) => Some(reason),
            Err(e) => Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_no_rules_allows_invocation() {
        let service = AgentInvokabilityService::new();
        let agent_id = Uuid::new_v4();
        
        let context = InvokabilityContext {
            agent_status: "active".to_string(),
            caller_id: None,
            caller_role: None,
            workspace_id: None,
        };
        
        assert!(service.is_invokable(agent_id, &context).unwrap());
    }
    
    #[test]
    fn test_deny_rule() {
        let mut service = AgentInvokabilityService::new();
        let agent_id = Uuid::new_v4();
        
        let mut params = HashMap::new();
        params.insert("status".to_string(), serde_json::json!("maintenance"));
        
        let rule = InvokabilityRule {
            agent_id,
            allowed: false,
            conditions: vec![InvokabilityCondition {
                condition_type: "agent_status".to_string(),
                parameters: params,
            }],
            reason: Some("Agent is under maintenance".to_string()),
        };
        
        service.add_rule(rule);
        
        let context = InvokabilityContext {
            agent_status: "maintenance".to_string(),
            caller_id: None,
            caller_role: None,
            workspace_id: None,
        };
        
        let result = service.is_invokable(agent_id, &context);
        assert!(result.is_err());
    }
}
