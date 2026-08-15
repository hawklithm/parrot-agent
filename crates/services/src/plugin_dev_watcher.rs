/// Plugin Dev Watcher Service
/// 
/// Plugin开发监控

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum PluginDevWatcherError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("watch error: {0}")]
    WatchError(String),
}

pub type PluginDevWatcherResult<T> = Result<T, PluginDevWatcherError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchTarget {
    pub plugin_id: String,
    pub path: PathBuf,
    pub auto_reload: bool,
}

pub struct PluginDevWatcherService {
    watches: HashMap<String, WatchTarget>,
}

impl PluginDevWatcherService {
    pub fn new() -> Self {
        Self {
            watches: HashMap::new(),
        }
    }
    
    pub fn add_watch(&mut self, target: WatchTarget) {
        self.watches.insert(target.plugin_id.clone(), target);
    }
    
    pub fn remove_watch(&mut self, plugin_id: &str) -> Option<WatchTarget> {
        self.watches.remove(plugin_id)
    }
    
    pub fn get_watch(&self, plugin_id: &str) -> Option<&WatchTarget> {
        self.watches.get(plugin_id)
    }
    
    pub fn list_watches(&self) -> Vec<&WatchTarget> {
        self.watches.values().collect()
    }
    
    pub async fn trigger_reload(&self, plugin_id: &str) -> PluginDevWatcherResult<()> {
        // 简化实现：实际应触发plugin重新加载
        Ok(())
    }
}

impl Default for PluginDevWatcherService {
    fn default() -> Self {
        Self::new()
    }
}
