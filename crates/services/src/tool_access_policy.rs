/// Tool Access Policy Service
/// 
/// 工具访问策略定义和评估引擎

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("policy not found: {0}")]
    PolicyNotFound(String),
    
    #[error("policy evaluation failed: {0}")]
    EvaluationFailed(String),
    
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),
}

pub type PolicyResult<T> = Result<T, PolicyError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCondition {
    pub field: String,
    pub operator: String, // eq, ne, in, contains, matches
    pub value: serde_json::Value,
}

impl PolicyCondition {
    pub fn evaluate(&self, context: &HashMap<String, serde_json::Value>) -> bool {
        let actual = match context.get(&self.field) {
            Some(v) => v,
            None => return false,
        };
        
        match self.operator.as_str() {
            "eq" => actual == &self.value,
            "ne" => actual != &self.value,
            "in" => {
                if let Some(arr) = self.value.as_array() {
                    arr.contains(actual)
                } else {
                    false
                }
            }
            "contains" => {
                if let (Some(haystack), Some(needle)) = (actual.as_str(), self.value.as_str()) {
                    haystack.contains(needle)
                } else {
                    false
                }
            }
            "matches" => {
                // 简化实现 - 实际应该支持正则
                if let (Some(text), Some(pattern)) = (actual.as_str(), self.value.as_str()) {
                    text == pattern
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAccessPolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub effect: PolicyEffect,
    pub tools: Vec<String>, // 工具名称模式，支持通配符
    pub agents: Vec<Uuid>,  // 应用的agent列表，空表示所有
    pub conditions: Vec<PolicyCondition>,
    pub priority: i32,      // 优先级，数字越大优先级越高
    pub enabled: bool,
}

impl ToolAccessPolicy {
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        for pattern in &self.tools {
            if Self::wildcard_match(pattern, tool_name) {
                return true;
            }
        }
        false
    }
    
    pub fn matches_agent(&self, agent_id: Uuid) -> bool {
        self.agents.is_empty() || self.agents.contains(&agent_id)
    }
    
    pub fn evaluate_conditions(&self, context: &HashMap<String, serde_json::Value>) -> bool {
        self.conditions.iter().all(|cond| cond.evaluate(context))
    }
    
    fn wildcard_match(pattern: &str, text: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        
        if pattern.ends_with("*") {
            let prefix = &pattern[..pattern.len() - 1];
            return text.starts_with(prefix);
        }
        
        pattern == text
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    pub agent_id: Uuid,
    pub tool_name: String,
    pub context: HashMap<String, serde_json::Value>,
}

pub struct ToolAccessPolicyService {
    policies: Vec<ToolAccessPolicy>,
}

impl ToolAccessPolicyService {
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }
    
    /// 添加策略
    pub fn add_policy(&mut self, policy: ToolAccessPolicy) -> PolicyResult<()> {
        // 验证策略
        if policy.id.is_empty() {
            return Err(PolicyError::InvalidPolicy("policy id is required".to_string()));
        }
        
        // 按优先级插入
        let pos = self.policies.iter()
            .position(|p| p.priority < policy.priority)
            .unwrap_or(self.policies.len());
        
        self.policies.insert(pos, policy);
        Ok(())
    }
    
    /// 删除策略
    pub fn remove_policy(&mut self, policy_id: &str) -> PolicyResult<()> {
        let pos = self.policies.iter()
            .position(|p| p.id == policy_id)
            .ok_or_else(|| PolicyError::PolicyNotFound(policy_id.to_string()))?;
        
        self.policies.remove(pos);
        Ok(())
    }
    
    /// 更新策略
    pub fn update_policy(&mut self, policy: ToolAccessPolicy) -> PolicyResult<()> {
        self.remove_policy(&policy.id)?;
        self.add_policy(policy)
    }
    
    /// 评估访问请求
    pub fn evaluate(&self, request: &AccessRequest) -> PolicyResult<bool> {
        // 默认拒绝
        let mut allowed = false;
        
        // 按优先级评估策略
        for policy in &self.policies {
            if !policy.enabled {
                continue;
            }
            
            // 检查工具匹配
            if !policy.matches_tool(&request.tool_name) {
                continue;
            }
            
            // 检查agent匹配
            if !policy.matches_agent(request.agent_id) {
                continue;
            }
            
            // 评估条件
            if !policy.evaluate_conditions(&request.context) {
                continue;
            }
            
            // 匹配成功，应用效果
            match policy.effect {
                PolicyEffect::Allow => allowed = true,
                PolicyEffect::Deny => return Ok(false), // 显式拒绝立即返回
            }
        }
        
        Ok(allowed)
    }
    
    /// 列出所有策略
    pub fn list_policies(&self) -> Vec<&ToolAccessPolicy> {
        self.policies.iter().collect()
    }
    
    /// 获取策略
    pub fn get_policy(&self, policy_id: &str) -> Option<&ToolAccessPolicy> {
        self.policies.iter().find(|p| p.id == policy_id)
    }
    
    /// 获取agent的适用策略
    pub fn get_agent_policies(&self, agent_id: Uuid) -> Vec<&ToolAccessPolicy> {
        self.policies.iter()
            .filter(|p| p.enabled && p.matches_agent(agent_id))
            .collect()
    }
    
    /// 获取工具的适用策略
    pub fn get_tool_policies(&self, tool_name: &str) -> Vec<&ToolAccessPolicy> {
        self.policies.iter()
            .filter(|p| p.enabled && p.matches_tool(tool_name))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wildcard_match() {
        assert!(ToolAccessPolicy::wildcard_match("*", "any-tool"));
        assert!(ToolAccessPolicy::wildcard_match("github:*", "github:search"));
        assert!(ToolAccessPolicy::wildcard_match("exact", "exact"));
        assert!(!ToolAccessPolicy::wildcard_match("exact", "other"));
    }
    
    #[test]
    fn test_condition_evaluation() {
        let condition = PolicyCondition {
            field: "role".to_string(),
            operator: "eq".to_string(),
            value: serde_json::json!("admin"),
        };
        
        let mut context = HashMap::new();
        context.insert("role".to_string(), serde_json::json!("admin"));
        
        assert!(condition.evaluate(&context));
        
        context.insert("role".to_string(), serde_json::json!("user"));
        assert!(!condition.evaluate(&context));
    }
    
    #[test]
    fn test_policy_evaluation() {
        let mut service = ToolAccessPolicyService::new();
        
        let agent_id = Uuid::new_v4();
        
        // 添加允许策略
        let allow_policy = ToolAccessPolicy {
            id: "allow-github".to_string(),
            name: "Allow GitHub Tools".to_string(),
            description: "Allow all GitHub tools".to_string(),
            effect: PolicyEffect::Allow,
            tools: vec!["github:*".to_string()],
            agents: vec![agent_id],
            conditions: vec![],
            priority: 10,
            enabled: true,
        };
        
        service.add_policy(allow_policy).unwrap();
        
        // 测试允许
        let request = AccessRequest {
            agent_id,
            tool_name: "github:search".to_string(),
            context: HashMap::new(),
        };
        
        assert!(service.evaluate(&request).unwrap());
        
        // 测试拒绝（不匹配）
        let request2 = AccessRequest {
            agent_id,
            tool_name: "slack:post".to_string(),
            context: HashMap::new(),
        };
        
        assert!(!service.evaluate(&request2).unwrap());
    }
    
    #[test]
    fn test_deny_precedence() {
        let mut service = ToolAccessPolicyService::new();
        let agent_id = Uuid::new_v4();
        
        // 低优先级允许
        service.add_policy(ToolAccessPolicy {
            id: "allow-all".to_string(),
            name: "Allow All".to_string(),
            description: "".to_string(),
            effect: PolicyEffect::Allow,
            tools: vec!["*".to_string()],
            agents: vec![agent_id],
            conditions: vec![],
            priority: 1,
            enabled: true,
        }).unwrap();
        
        // 高优先级拒绝
        service.add_policy(ToolAccessPolicy {
            id: "deny-dangerous".to_string(),
            name: "Deny Dangerous".to_string(),
            description: "".to_string(),
            effect: PolicyEffect::Deny,
            tools: vec!["dangerous:*".to_string()],
            agents: vec![agent_id],
            conditions: vec![],
            priority: 100,
            enabled: true,
        }).unwrap();
        
        // 拒绝应该优先
        let request = AccessRequest {
            agent_id,
            tool_name: "dangerous:exec".to_string(),
            context: HashMap::new(),
        };
        
        assert!(!service.evaluate(&request).unwrap());
    }
}
