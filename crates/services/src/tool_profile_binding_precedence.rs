use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolProfile {
    pub tool_name: String,
    pub bindings: Vec<ProfileBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileBinding {
    pub context: BindingContext,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BindingContext {
    Global,
    Workspace(String),
    Agent(String),
    Session(String),
}

pub struct ToolProfileBindingPrecedence {
    bindings: HashMap<String, Vec<ProfileBinding>>,
}

impl ToolProfileBindingPrecedence {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
    
    pub fn add_binding(&mut self, tool_name: String, binding: ProfileBinding) {
        self.bindings
            .entry(tool_name)
            .or_insert_with(Vec::new)
            .push(binding);
    }
    
    pub fn get_effective_binding(
        &self,
        tool_name: &str,
        context: &BindingContext,
    ) -> Option<ProfileBinding> {
        let bindings = self.bindings.get(tool_name)?;
        
        // Sort by precedence: Session > Agent > Workspace > Global
        let mut applicable: Vec<_> = bindings
            .iter()
            .filter(|b| b.enabled && self.is_applicable(&b.context, context))
            .cloned()
            .collect();
        
        applicable.sort_by(|a, b| {
            // First by context precedence
            let context_order = self.context_precedence(&b.context)
                .cmp(&self.context_precedence(&a.context));
            
            // Then by explicit priority
            if context_order == std::cmp::Ordering::Equal {
                b.priority.cmp(&a.priority)
            } else {
                context_order
            }
        });
        
        applicable.into_iter().next()
    }
    
    fn is_applicable(&self, binding_context: &BindingContext, current_context: &BindingContext) -> bool {
        match (binding_context, current_context) {
            (BindingContext::Global, _) => true,
            (BindingContext::Workspace(w1), BindingContext::Workspace(w2)) => w1 == w2,
            (BindingContext::Workspace(w1), BindingContext::Agent(a)) => {
                // In production, would check if agent belongs to workspace
                true
            }
            (BindingContext::Agent(a1), BindingContext::Agent(a2)) => a1 == a2,
            (BindingContext::Session(s1), BindingContext::Session(s2)) => s1 == s2,
            _ => false,
        }
    }
    
    fn context_precedence(&self, context: &BindingContext) -> i32 {
        match context {
            BindingContext::Session(_) => 4,
            BindingContext::Agent(_) => 3,
            BindingContext::Workspace(_) => 2,
            BindingContext::Global => 1,
        }
    }
    
    pub fn list_tool_bindings(&self, tool_name: &str) -> Vec<ProfileBinding> {
        self.bindings
            .get(tool_name)
            .cloned()
            .unwrap_or_default()
    }
    
    pub fn remove_binding(&mut self, tool_name: &str, context: &BindingContext) {
        if let Some(bindings) = self.bindings.get_mut(tool_name) {
            bindings.retain(|b| &b.context != context);
        }
    }
}

impl Default for ToolProfileBindingPrecedence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_binding_precedence() {
        let mut precedence = ToolProfileBindingPrecedence::new();
        
        precedence.add_binding(
            "read".to_string(),
            ProfileBinding {
                context: BindingContext::Global,
                priority: 1,
                enabled: true,
            },
        );
        
        precedence.add_binding(
            "read".to_string(),
            ProfileBinding {
                context: BindingContext::Agent("agent1".to_string()),
                priority: 10,
                enabled: true,
            },
        );
        
        let binding = precedence.get_effective_binding(
            "read",
            &BindingContext::Agent("agent1".to_string()),
        );
        
        assert!(binding.is_some());
        assert!(matches!(binding.unwrap().context, BindingContext::Agent(_)));
    }
}
