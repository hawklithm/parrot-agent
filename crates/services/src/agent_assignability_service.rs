/// Agent Assignability Service
/// 
/// Agent可分配性判断服务

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AssignabilityError {
    #[error("assignment not allowed: {0}")]
    NotAllowed(String),
    
    #[error("conflict detected: {0}")]
    ConflictDetected(String),
}

pub type AssignabilityResult<T> = Result<T, AssignabilityError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentRule {
    pub rule_type: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

impl AssignmentRule {
    pub fn evaluate(&self, context: &AssignmentContext) -> bool {
        match self.rule_type.as_str() {
            "max_assignments" => {
                if let Some(max) = self.parameters.get("max").and_then(|v| v.as_u64()) {
                    context.current_assignments < max as usize
                } else {
                    false
                }
            }
            "skill_required" => {
                if let Some(required) = self.parameters.get("skill").and_then(|v| v.as_str()) {
                    context.agent_skills.contains(&required.to_string())
                } else {
                    false
                }
            }
            "workspace_access" => {
                if let Some(required_ws) = self.parameters.get("workspace_id").and_then(|v| v.as_str()) {
                    if let Some(ws) = &context.target_workspace {
                        ws.to_string() == required_ws
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssignmentContext {
    pub agent_id: Uuid,
    pub target_issue: Option<Uuid>,
    pub target_workspace: Option<Uuid>,
    pub current_assignments: usize,
    pub agent_skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRule {
    pub agent_id: Uuid,
    pub conflicting_agents: Vec<Uuid>,
    pub reason: String,
}

pub struct AgentAssignabilityService {
    rules: HashMap<Uuid, Vec<AssignmentRule>>,
    conflicts: Vec<ConflictRule>,
}

impl AgentAssignabilityService {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            conflicts: Vec::new(),
        }
    }
    
    /// 添加分配规则
    pub fn add_rule(&mut self, agent_id: Uuid, rule: AssignmentRule) {
        self.rules.entry(agent_id)
            .or_insert_with(Vec::new)
            .push(rule);
    }
    
    /// 添加冲突规则
    pub fn add_conflict_rule(&mut self, rule: ConflictRule) {
        self.conflicts.push(rule);
    }
    
    /// 检查是否可分配
    pub fn is_assignable(&self, context: &AssignmentContext) -> AssignabilityResult<bool> {
        // 检查分配规则
        if let Some(rules) = self.rules.get(&context.agent_id) {
            for rule in rules {
                if !rule.evaluate(context) {
                    return Err(AssignabilityError::NotAllowed(format!(
                        "rule '{}' not satisfied",
                        rule.rule_type
                    )));
                }
            }
        }
        
        // 检查冲突
        for conflict in &self.conflicts {
            if conflict.agent_id == context.agent_id {
                // 检查是否有冲突的agent已分配
                // 简化实现 - 实际应该查询当前分配状态
                if !conflict.conflicting_agents.is_empty() {
                    // 假设检查逻辑
                }
            }
        }
        
        Ok(true)
    }
    
    /// 检测冲突
    pub fn detect_conflicts(&self, agent_id: Uuid, assigned_agents: &[Uuid]) -> Vec<String> {
        let mut conflicts = Vec::new();
        
        for conflict_rule in &self.conflicts {
            if conflict_rule.agent_id == agent_id {
                for &assigned in assigned_agents {
                    if conflict_rule.conflicting_agents.contains(&assigned) {
                        conflicts.push(format!(
                            "Conflict with agent {}: {}",
                            assigned, conflict_rule.reason
                        ));
                    }
                }
            }
        }
        
        conflicts
    }
    
    /// 获取分配建议
    pub fn get_assignment_suggestion(&self, context: &AssignmentContext) -> Option<String> {
        match self.is_assignable(context) {
            Ok(_) => Some("Agent is assignable".to_string()),
            Err(AssignabilityError::NotAllowed(reason)) => Some(format!("Not assignable: {}", reason)),
            Err(AssignabilityError::ConflictDetected(reason)) => Some(format!("Conflict: {}", reason)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_max_assignments_rule() {
        let mut service = AgentAssignabilityService::new();
        let agent_id = Uuid::new_v4();
        
        let mut params = HashMap::new();
        params.insert("max".to_string(), serde_json::json!(5));
        
        let rule = AssignmentRule {
            rule_type: "max_assignments".to_string(),
            parameters: params,
        };
        
        service.add_rule(agent_id, rule);
        
        // 在限制内
        let context = AssignmentContext {
            agent_id,
            target_issue: None,
            target_workspace: None,
            current_assignments: 3,
            agent_skills: vec![],
        };
        
        assert!(service.is_assignable(&context).is_ok());
        
        // 超过限制
        let context2 = AssignmentContext {
            agent_id,
            target_issue: None,
            target_workspace: None,
            current_assignments: 6,
            agent_skills: vec![],
        };
        
        assert!(service.is_assignable(&context2).is_err());
    }
    
    #[test]
    fn test_skill_requirement() {
        let mut service = AgentAssignabilityService::new();
        let agent_id = Uuid::new_v4();
        
        let mut params = HashMap::new();
        params.insert("skill".to_string(), serde_json::json!("rust"));
        
        let rule = AssignmentRule {
            rule_type: "skill_required".to_string(),
            parameters: params,
        };
        
        service.add_rule(agent_id, rule);
        
        let context = AssignmentContext {
            agent_id,
            target_issue: None,
            target_workspace: None,
            current_assignments: 0,
            agent_skills: vec!["rust".to_string(), "typescript".to_string()],
        };
        
        assert!(service.is_assignable(&context).is_ok());
    }
    
    #[test]
    fn test_conflict_detection() {
        let mut service = AgentAssignabilityService::new();
        let agent_id = Uuid::new_v4();
        let conflicting_agent = Uuid::new_v4();
        
        let conflict_rule = ConflictRule {
            agent_id,
            conflicting_agents: vec![conflicting_agent],
            reason: "Cannot work on same issue".to_string(),
        };
        
        service.add_conflict_rule(conflict_rule);
        
        let conflicts = service.detect_conflicts(agent_id, &[conflicting_agent]);
        assert_eq!(conflicts.len(), 1);
    }
}
