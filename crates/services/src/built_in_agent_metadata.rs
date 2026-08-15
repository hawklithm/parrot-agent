/// Built-in Agent Metadata Service
/// 
/// 内置Agent元数据管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub default_config: serde_json::Value,
    pub version: String,
}

pub struct BuiltInAgentMetadataService {
    metadata: HashMap<String, AgentMetadata>,
}

impl BuiltInAgentMetadataService {
    pub fn new() -> Self {
        let mut metadata = HashMap::new();
        
        // 注册内置Agent元数据
        metadata.insert("task".to_string(), AgentMetadata {
            id: "task".to_string(),
            name: "Task Agent".to_string(),
            description: "General purpose task execution agent".to_string(),
            capabilities: vec!["read".to_string(), "write".to_string(), "execute".to_string()],
            default_config: serde_json::json!({"timeout": 3600}),
            version: "1.0.0".to_string(),
        });
        
        metadata.insert("scout".to_string(), AgentMetadata {
            id: "scout".to_string(),
            name: "Scout Agent".to_string(),
            description: "Read-only exploration agent".to_string(),
            capabilities: vec!["read".to_string(), "search".to_string()],
            default_config: serde_json::json!({"timeout": 1800}),
            version: "1.0.0".to_string(),
        });
        
        Self { metadata }
    }
    
    pub fn get_metadata(&self, agent_id: &str) -> Option<&AgentMetadata> {
        self.metadata.get(agent_id)
    }
    
    pub fn list_all(&self) -> Vec<&AgentMetadata> {
        self.metadata.values().collect()
    }
    
    pub fn register(&mut self, metadata: AgentMetadata) {
        self.metadata.insert(metadata.id.clone(), metadata);
    }
}

impl Default for BuiltInAgentMetadataService {
    fn default() -> Self {
        Self::new()
    }
}
