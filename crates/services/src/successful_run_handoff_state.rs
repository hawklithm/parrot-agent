use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents the state that should be handed off when a run completes successfully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessfulRunHandoffState {
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub completion_time: chrono::DateTime<chrono::Utc>,
    pub final_artifacts: Vec<ArtifactReference>,
    pub context_summary: String,
    pub next_actions: Vec<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactReference {
    pub artifact_id: Uuid,
    pub artifact_type: String,
    pub location: String,
    pub description: Option<String>,
}

impl SuccessfulRunHandoffState {
    pub fn new(run_id: Uuid, agent_id: Uuid) -> Self {
        Self {
            run_id,
            agent_id,
            completion_time: chrono::Utc::now(),
            final_artifacts: Vec::new(),
            context_summary: String::new(),
            next_actions: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn add_artifact(&mut self, artifact: ArtifactReference) {
        self.final_artifacts.push(artifact);
    }
    
    pub fn set_context_summary(&mut self, summary: String) {
        self.context_summary = summary;
    }
    
    pub fn add_next_action(&mut self, action: String) {
        self.next_actions.push(action);
    }
    
    pub fn set_metadata(&mut self, metadata: serde_json::Value) {
        self.metadata = metadata;
    }
    
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({}))
    }
    
    pub fn from_json(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_handoff_state_creation() {
        let run_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        
        let mut state = SuccessfulRunHandoffState::new(run_id, agent_id);
        state.set_context_summary("Task completed successfully".to_string());
        state.add_next_action("Review artifacts".to_string());
        
        assert_eq!(state.run_id, run_id);
        assert_eq!(state.agent_id, agent_id);
        assert_eq!(state.next_actions.len(), 1);
    }
    
    #[test]
    fn test_json_serialization() {
        let state = SuccessfulRunHandoffState::new(Uuid::new_v4(), Uuid::new_v4());
        let json = state.to_json();
        let deserialized = SuccessfulRunHandoffState::from_json(json).unwrap();
        
        assert_eq!(state.run_id, deserialized.run_id);
    }
}
