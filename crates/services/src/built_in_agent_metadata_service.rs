use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInAgentMetadata {
    pub agent_type: String,
    pub display_name: String,
    pub description: String,
    pub capabilities: Vec<AgentCapability>,
    pub default_config: AgentConfig,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub tools: Vec<String>,
    pub permissions: Vec<String>,
    pub timeout_seconds: Option<i32>,
}

#[async_trait]
pub trait BuiltInAgentMetadataService: Send + Sync {
    fn get_metadata(&self, agent_type: &str) -> Option<BuiltInAgentMetadata>;
    fn list_all_metadata(&self) -> Vec<BuiltInAgentMetadata>;
    fn get_capabilities(&self, agent_type: &str) -> Vec<AgentCapability>;
    fn get_default_config(&self, agent_type: &str) -> Option<AgentConfig>;
}

pub struct BuiltInAgentMetadataServiceImpl {
    metadata: HashMap<String, BuiltInAgentMetadata>,
}

impl BuiltInAgentMetadataServiceImpl {
    pub fn new() -> Self {
        let mut metadata = HashMap::new();
        
        // Task Agent
        metadata.insert(
            "task".to_string(),
            BuiltInAgentMetadata {
                agent_type: "task".to_string(),
                display_name: "Task Agent".to_string(),
                description: "General-purpose task execution agent".to_string(),
                capabilities: vec![
                    AgentCapability {
                        name: "code_execution".to_string(),
                        description: "Execute code in various languages".to_string(),
                        enabled: true,
                    },
                    AgentCapability {
                        name: "file_operations".to_string(),
                        description: "Read and write files".to_string(),
                        enabled: true,
                    },
                ],
                default_config: AgentConfig {
                    max_tokens: Some(4000)                 temperature: Some(0.7),
                    tools: vec!["read".to_string(), "write".to_string(), "bash".to_string()],
                    permissions: vec!["file:read".to_string(), "file:write".to_string()],
                    timeout_seconds: Some(300),
                },
                version: "1.0.0".to_string(),
            },
        );
        
        // Scout Agent
        metadata.insert(
            "scout".to_string(),
            BuiltInAgentMetadata {
                agent_type: "scout".to_string(),
                display_name: "Scout Agent".to_string(),
                description: "Read-only exploration and analysis agent".to_string(),
                capabilities: vec![
                    AgentCapability {
                        name: "code_analysis".to_string(),
                        description: "Analyze code structure and patterns".to_string(),
                        enabled: true,
                    },
                    AgentCapability {
                        name: "search".to_string(),
                        description: "Search across codebase".to_string(),
                        enabled: true,
                    },
                ],
                default_config: AgentConfig {
                    max_tokens: Some(2000),
                    temperature: Some(0.5),
                    tools: vec!["read".to_string(), "grep".to_string(), "glob".to_string()],
                    permissions: vec!["file:read".to_string()],
                    timeout_seconds: Some(180),
                },
                version: "1.0.0".to_string(),
            },
        );
        
        // Reviewer Agent
        metadata.insert(
            "reviewer".to_string(),
            BuiltInAgentMetadata {
                agent_type: "reviewer".to_string(),
                display_name: "Reviewer Agent".to_string(),
                description: "Code review and quality analysis agent".to_string(),
                capabilities: vec![
                    AgentCapability {
                        name: "code_review".to_string(),
                        description: "Review code quality and security".to_string(),
                        enabled: true,
                    },
                ],
                default_config: AgentConfig {
                    max_tokens: Some(3000),
                    temperature: Some(0.3),
                    tools: vec!["read".to_string(), "grep".to_string()],
                    permissions: vec!["file:read".to_string()],
                    timeout_seconds: Some(240),
                },
                version: "1.0.0".to_string(),
            },
        );
        
        Self { metadata }
    }
}

impl Default for BuiltInAgentMetadataServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BuiltInAgentMetadataService for BuiltInAgentMetadataServiceImpl {
    fn get_metadata(&self, agent_type: &str) -> Option<BuiltInAgentMetadata> {
        self.metadata.get(agent_type).cloned()
    }
    
    fn list_all_metadata(&self) -> Vec<BuiltInAgentMetadata> {
        self.metadata.values().cloned().collect()
    }
    
    fn get_capabilities(&self, agent_type: &str) -> Vec<AgentCapability> {
        self.metadata
            .get(agent_type)
            .map(|m| m.capabilities.clone())
            .unwrap_or_default()
    }
    
    fn get_default_config(&self, agent_type: &str) -> Option<AgentConfig> {
        self.metadata
            .get(agent_type)
            .map(|m| m.default_config.clone())
    }
}
