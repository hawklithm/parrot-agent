/// Plugin 工具注册表
/// 
/// 管理 Plugin 注册的工具，支持工具发现、元数据管理和版本控制

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ToolRegistryError {
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    
    #[error("tool already registered: {0}")]
    ToolAlreadyRegistered(String),
    
    #[error("invalid tool name: {0}")]
    InvalidToolName(String),
}

pub type ToolRegistryResult<T> = Result<T, ToolRegistryError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub plugin_id: Uuid,
    pub version: String,
    pub description: String,
    pub schema: serde_json::Value,
}

pub struct PluginToolRegistry {
    tools: Arc<RwLock<HashMap<String, ToolMetadata>>>,
}

impl PluginToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn register_tool(&self, tool: ToolMetadata) -> ToolRegistryResult<()> {
        let mut tools = self.tools.write().await;
        
        if tools.contains_key(&tool.name) {
            return Err(ToolRegistryError::ToolAlreadyRegistered(tool.name.clone()));
        }
        
        tools.insert(tool.name.clone(), tool);
        Ok(())
    }
    
    pub async fn unregister_tool(&self, tool_name: &str) -> ToolRegistryResult<()> {
        let mut tools = self.tools.write().await;
        tools.remove(tool_name)
            .ok_or_else(|| ToolRegistryError::ToolNotFound(tool_name.to_string()))?;
        Ok(())
    }
    
    pub async fn get_tool(&self, tool_name: &str) -> Option<ToolMetadata> {
        self.tools.read().await.get(tool_name).cloned()
    }
    
    pub async fn list_tools(&self) -> Vec<ToolMetadata> {
        self.tools.read().await.values().cloned().collect()
    }
    
    pub async fn list_tools_by_plugin(&self, plugin_id: Uuid) -> Vec<ToolMetadata> {
        self.tools.read().await
            .values()
            .filter(|t| t.plugin_id == plugin_id)
            .cloned()
            .collect()
    }
}

impl Default for PluginToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_register_and_get_tool() {
        let registry = PluginToolRegistry::new();
        let plugin_id = Uuid::new_v4();
        
        let tool = ToolMetadata {
            name: "test_tool".to_string(),
            plugin_id,
            version: "1.0.0".to_string(),
            description: "Test tool".to_string(),
            schema: serde_json::json!({}),
        };
        
        registry.register_tool(tool.clone()).await.unwrap();
        
        let retrieved = registry.get_tool("test_tool").await.unwrap();
        assert_eq!(retrieved.name, "test_tool");
        assert_eq!(retrieved.plugin_id, plugin_id);
    }
    
    #[tokio::test]
    async fn test_list_tools_by_plugin() {
        let registry = PluginToolRegistry::new();
        let plugin_id = Uuid::new_v4();
        
        let tool1 = ToolMetadata {
            name: "tool1".to_string(),
            plugin_id,
            version: "1.0.0".to_string(),
            description: "Tool 1".to_string(),
            schema: serde_json::json!({}),
        };
        
        let tool2 = ToolMetadata {
            name: "tool2".to_string(),
            plugin_id,
            version: "1.0.0".to_string(),
            description: "Tool 2".to_string(),
            schema: serde_json::json!({}),
        };
        
        registry.register_tool(tool1).await.unwrap();
        registry.register_tool(tool2).await.unwrap();
        
        let tools = registry.list_tools_by_plugin(plugin_id).await;
        assert_eq!(tools.len(), 2);
    }
}
