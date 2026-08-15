use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default agent instructions and templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultAgentInstructions {
    pub instruction_type: InstructionType,
    pub template: String,
    pub variables: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InstructionType {
    SystemPrompt,
    TaskGuidance,
    ErrorHandling,
    OutputFormat,
    SafetyGuidelines,
    Custom(String),
}

impl DefaultAgentInstructions {
    pub fn new(instruction_type: InstructionType, template: String) -> Self {
        Self {
            instruction_type,
            template,
            variables: HashMap::new(),
            enabled: true,
        }
    }
    
    pub fn add_variable(&mut self, key: String, value: String) {
        self.variables.insert(key, value);
    }
    
    pub fn render(&self) -> String {
        let mut result = self.template.clone();
        
        for (key, value) in &self.variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        
        result
    }
    
    pub fn system_prompt_default() -> Self {
        Self::new(
            InstructionType::SystemPrompt,
            "You are a helpful AI assistant. {{role}}".to_string(),
        )
    }
    
    pub fn task_guidance_default() -> Self {
        Self::new(
            InstructionType::TaskGuidance,
            "Follow these steps: {{steps}}. Complete the task thoroughly.".to_string(),
        )
    }
    
    pub fn error_handling_default() -> Self {
        Self::new(
            InstructionType::ErrorHandling,
            "If you encounter an error: {{error_strategy}}. Always provide clear feedback.".to_string(),
        )
    }
    
    pub fn output_format_default() -> Self {
        Self::new(
            InstructionType::OutputFormat,
            "Format your response as: {{format}}".to_string(),
        )
    }
    
    pub fn safety_guidelines_default() -> Self {
        Self::new(
            InstructionType::SafetyGuidelines,
            "Safety guidelines: {{guidelines}}. Never perform harmful actions.".to_string(),
        )
    }
}

pub struct DefaultInstructionsRegistry {
    instructions: HashMap<InstructionType, DefaultAgentInstructions>,
}

impl DefaultInstructionsRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            instructions: HashMap::new(),
        };
        
        // Register default instructions
        registry.register(DefaultAgentInstructions::system_prompt_default());
        registry.register(DefaultAgentInstructions::task_guidance_default());
        registry.register(DefaultAgentInstructions::error_handling_default());
        registry.register(DefaultAgentInstructions::output_format_default());
        registry.register(DefaultAgentInstructions::safety_guidelines_default());
        
        registry
    }
    
    pub fn register(&mut self, instructions: DefaultAgentInstructions) {
        self.instructions.insert(instructions.instruction_type.clone(), instructions);
    }
    
    pub fn get(&self, instruction_type: &InstructionType) -> Option<&DefaultAgentInstructions> {
        self.instructions.get(instruction_type)
    }
    
    pub fn get_mut(&mut self, instruction_type: &InstructionType) -> Option<&mut DefaultAgentInstructions> {
        self.instructions.get_mut(instruction_type)
    }
    
    pub fn list_all(&self) -> Vec<&DefaultAgentInstructions> {
        self.instructions.values().filter(|i| i.enabled).collect()
    }
    
    pub fn remove(&mut self, instruction_type: &InstructionType) {
        self.instructions.remove(instruction_type);
    }
}

impl Default for DefaultInstructionsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_instructions_rendering() {
        let mut instructions = DefaultAgentInstructions::system_prompt_default();
        instructions.add_variable("role".to_string(), "coding assistant".to_string());
        
        let rendered = instructions.render();
        assert!(rendered.contains("coding assistant"));
        assert!(!rendered.contains("{{role}}"));
    }
    
    #[test]
    fn test_registry() {
        let registry = DefaultInstructionsRegistry::new();
        
        let system_prompt = registry.get(&InstructionType::SystemPrompt);
        assert!(system_prompt.is_some());
        
        let all = registry.list_all();
        assert_eq!(all.len(), 5);
    }
    
    #[test]
    fn test_custom_instruction() {
        let custom = DefaultAgentInstructions::new(
            InstructionType::Custom("debug".to_string()),
            "Debug mode: {{level}}".to_string(),
        );
        
        assert!(matches!(custom.instruction_type, InstructionType::Custom(_)));
    }
}
