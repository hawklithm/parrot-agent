use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Manages runtime skill selection and activation for agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSkillSelections {
    pub agent_id: Uuid,
    pub active_skills: Vec<SkillSelection>,
    pub available_skills: Vec<SkillInfo>,
    pub selection_strategy: SelectionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSelection {
    pub skill_id: Uuid,
    pub skill_name: String,
    pub priority: i32,
    pub enabled: bool,
    pub configuration: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub skill_id: Uuid,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub required_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStrategy {
    Manual,
    Automatic,
    Contextual,
    Priority,
}

impl RuntimeSkillSelections {
    pub fn new(agent_id: Uuid, strategy: SelectionStrategy) -> Self {
        Self {
            agent_id,
            active_skills: Vec::new(),
            available_skills: Vec::new(),
            selection_strategy: strategy,
        }
    }
    
    pub fn add_available_skill(&mut self, skill: SkillInfo) {
        if !self.available_skills.iter().any(|s| s.skill_id == skill.skill_id) {
            self.available_skills.push(skill);
        }
    }
    
    pub fn activate_skill(
        &mut self,
        skill_id: Uuid,
        priority: i32,
        configuration: HashMap<String, serde_json::Value>,
    ) -> Result<(), String> {
        // Check if skill is available
        let skill_info = self.available_skills
            .iter()
            .find(|s| s.skill_id == skill_id)
            .ok_or_else(|| "Skill not found in available skills".to_string())?;
        
        // Check if already active
        if self.active_skills.iter().any(|s| s.skill_id == skill_id) {
            return Err("Skill already active".to_string());
        }
        
        let selection = SkillSelection {
            skill_id,
            skill_name: skill_info.name.clone(),
            priority,
            enabled: true,
            configuration,
        };
        
        self.active_skills.push(selection);
        self.sort_by_priority();
        
        Ok(())
    }
    
    pub fn deactivate_skill(&mut self, skill_id: Uuid) {
        self.active_skills.retain(|s| s.skill_id != skill_id);
    }
    
    pub fn enable_skill(&mut self, skill_id: Uuid) {
        if let Some(skill) = self.active_skills.iter_mut().find(|s| s.skill_id == skill_id) {
            skill.enabled = true;
        }
    }
    
    pub fn disable_skill(&mut self, skill_id: Uuid) {
        if let Some(skill) = self.active_skills.iter_mut().find(|s| s.skill_id == skill_id) {
            skill.enabled = false;
        }
    }
    
    fn sort_by_priority(&mut self) {
        self.active_skills.sort_by(|a, b| b.priority.cmp(&a.priority));
    }
    
    pub fn get_enabled_skills(&self) -> Vec<&SkillSelection> {
        self.active_skills
            .iter()
            .filter(|s| s.enabled)
            .collect()
    }
    
    pub fn update_skill_configuration(
        &mut self,
        skill_id: Uuid,
        configuration: HashMap<String, serde_json::Value>,
    ) -> Result<(), String> {
        let skill = self.active_skills
            .iter_mut()
            .find(|s| s.skill_id == skill_id)
            .ok_or_else(|| "Skill not active".to_string())?;
        
        skill.configuration = configuration;
        Ok(())
    }
    
    pub fn has_capability(&self, capability: &str) -> bool {
        self.get_enabled_skills()
            .iter()
            .any(|selection| {
                self.available_skills
                    .iter()
                    .find(|info| info.skill_id == selection.skill_id)
                    .map(|info| info.capabilities.iter().any(|c| c == capability))
                    .unwrap_or(false)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_skill_activation() {
        let agent_id = Uuid::new_v4();
        let mut selections = RuntimeSkillSelections::new(agent_id, SelectionStrategy::Manual);
        
        let skill_id = Uuid::new_v4();
        let skill = SkillInfo {
            skill_id,
            name: "Code Analysis".to_string(),
            description: "Analyzes code".to_string(),
            capabilities: vec!["analyze".to_string()],
            required_permissions: vec![],
        };
        
        selections.add_available_skill(skill);
        
        let result = selections.activate_skill(skill_id, 10, HashMap::new());
        assert!(result.is_ok());
        assert_eq!(selections.active_skills.len(), 1);
    }
    
    #[test]
    fn test_priority_ordering() {
        let mut selections = RuntimeSkillSelections::new(Uuid::new_v4(), SelectionStrategy::Priority);
        
        for (name, priority) in &[("Low", 1), ("High", 10), ("Medium", 5)] {
            let skill_id = Uuid::new_v4();
            let skill = SkillInfo {
                skill_id,
                name: name.to_string(),
                description: String::new(),
                capabilities: vec![],
                required_permissions: vec![],
            };
            
            selections.add_available_skill(skill);
            selections.activate_skill(skill_id, *priority, HashMap::new()).unwrap();
        }
        
        assert_eq!(selections.active_skills[0].skill_name, "High");
        assert_eq!(selections.active_skills[2].skill_name, "Low");
    }
}
