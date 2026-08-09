use crate::adapter_plugin_store::{AdapterPluginStore, JsonFileAdapterPluginStore, AdapterPluginRecord};
use crate::builtin_adapter_types::is_builtin_adapter_type;
use models::AdapterType;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// 适配器注册表状态管理
/// 
/// 整合了：
/// - Builtin 适配器（内置，不可删除）
/// - External 适配器（外部安装，可以覆盖内置类型）
/// - 禁用/启用状态管理
/// - 覆盖暂停状态
pub struct AdapterRegistryState {
    /// 插件持久化存储
    plugin_store: Arc<RwLock<Box<dyn AdapterPluginStore>>>,
    
    /// 禁用的适配器类型集合
    disabled_types: Arc<RwLock<HashSet<String>>>,
    
    /// 覆盖暂停的适配器类型集合（对应 Paperclip 的 overridePaused）
    override_paused_types: Arc<RwLock<HashSet<String>>>,
}

impl AdapterRegistryState {
    /// 创建新的注册表状态（使用默认存储路径）
    pub fn new() -> Result<Self, std::io::Error> {
        let store_path = Self::default_store_path();
        Self::with_store_path(store_path)
    }
    
    /// 使用指定的存储路径创建
    pub fn with_store_path(store_path: PathBuf) -> Result<Self, std::io::Error> {
        let plugin_store = JsonFileAdapterPluginStore::new(store_path)?;
        Ok(Self {
            plugin_store: Arc::new(RwLock::new(Box::new(plugin_store))),
            disabled_types: Arc::new(RwLock::new(HashSet::new())),
            override_paused_types: Arc::new(RwLock::new(HashSet::new())),
        })
    }
    
    /// 默认存储路径（对齐 Paperclip 的 data/adapter-plugins.json）
    fn default_store_path() -> PathBuf {
        PathBuf::from("data").join("adapter-plugins.json")
    }
    
    /// 列出所有外部插件记录
    pub fn list_external_plugins(&self) -> Vec<AdapterPluginRecord> {
        let store = self.plugin_store.read();
        store.list()
    }
    
    /// 检查适配器类型是否为内置类型
    pub fn is_builtin(&self, adapter_type: &str) -> bool {
        is_builtin_adapter_type(adapter_type)
    }
    
    /// 检查适配器是否被外部插件覆盖
    pub fn is_overridden_by_external(&self, adapter_type: &str) -> bool {
        let store = self.plugin_store.read();
        store.contains(adapter_type)
    }
    
    /// 获取外部插件记录
    pub fn get_external_plugin(&self, adapter_type: &str) -> Option<AdapterPluginRecord> {
        let store = self.plugin_store.read();
        store.get(adapter_type)
    }
    
    /// 添加外部插件记录
    pub fn add_external_plugin(&self, record: AdapterPluginRecord) {
        let mut store = self.plugin_store.write();
        store.add(record);
    }
    
    /// 删除外部插件记录
    pub fn remove_external_plugin(&self, adapter_type: &str) -> bool {
        let mut store = self.plugin_store.write();
        store.remove(adapter_type)
    }
    
    /// 检查适配器是否被禁用
    /// 检查适配器是否被禁用
    pub fn is_disabled(&self, adapter_type: &str) -> bool {
        let disabled = self.disabled_types.read();
        disabled.contains(adapter_type)
    }
    /// 设置适配器禁用/启用状态
    pub fn set_disabled(&self, adapter_type: &str, disabled: bool) {
        let mut disabled_set = self.disabled_types.write();
        if disabled {
            disabled_set.insert(adapter_type.to_string());
        } else {
            disabled_set.remove(adapter_type);
        }
    }
    
    /// 批量设置禁用状态
    pub fn set_disabled_batch(&self, types: &[String], disabled: bool) {
        let mut disabled_set = self.disabled_types.write();
        for adapter_type in types {
            if disabled {
                disabled_set.insert(adapter_type.clone());
            } else {
                disabled_set.remove(adapter_type);
            }
        }
    }
    
    /// 检查适配器覆盖是否被暂停
    /// 
    /// 对应 Paperclip 的 overridePaused 状态：
    /// 当外部适配器被暂停时，系统回退到内置实现
    pub fn is_override_paused(&self, adapter_type: &str) -> bool {
        let paused = self.override_paused_types.read();
        paused.contains(adapter_type)
    }
    
    /// 设置覆盖暂停状态
    pub fn set_override_paused(&self, adapter_type: &str, paused: bool) {
        let mut paused_set = self.override_paused_types.write();
        if paused {
            paused_set.insert(adapter_type.to_string());
        } else {
            paused_set.remove(adapter_type);
        }
    }
    
    /// 检查适配器是否可用（未禁用且未暂停）
    pub fn is_available(&self, adapter_type: &str) -> bool {
        !self.is_disabled(adapter_type) && !self.is_override_paused(adapter_type)
    }
    
    /// 获取适配器的来源信息
    pub fn get_adapter_source(&self, adapter_type: &str) -> AdapterSource {
        let is_builtin = self.is_builtin(adapter_type);
        let has_external = self.is_overridden_by_external(adapter_type);
        let override_paused = self.is_override_paused(adapter_type);
        
        match (is_builtin, has_external, override_paused) {
            (true, true, false) => AdapterSource::ExternalOverride,
            (true, true, true) => AdapterSource::BuiltinFallback,
            (true, false, _) => AdapterSource::Builtin,
            (false, true, _) => AdapterSource::External,
            (false, false, _) => AdapterSource::Unknown,
        }
    }
}

impl Default for AdapterRegistryState {
    fn default() -> Self {
        Self::new().expect("Failed to create default AdapterRegistryState")
    }
}

/// 适配器来源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterSource {
    /// 内置适配器
    Builtin,
    /// 外部适配器
    External,
    /// 外部覆盖内置
    ExternalOverride,
    /// 内置回退（外部被暂停）
    BuiltinFallback,
    /// 未知来源
    Unknown,
}

impl AdapterSource {
    /// 转换为 Paperclip API 的 source 字符串
    pub fn to_api_string(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::External => "external",
            Self::ExternalOverride => "external-override",
            Self::BuiltinFallback => "builtin-fallback",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter_plugin_store::InMemoryAdapterPluginStore;
    
    fn create_test_state() -> AdapterRegistryState {
        AdapterRegistryState {
            plugin_store: Arc::new(RwLock::new(Box::new(InMemoryAdapterPluginStore::new()))),
            disabled_types: Arc::new(RwLock::new(HashSet::new())),
            override_paused_types: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    
    #[test]
    fn test_builtin_detection() {
        let state = create_test_state();
        assert!(state.is_builtin("process"));
        assert!(state.is_builtin("claude_local"));
        assert!(!state.is_builtin("custom_external"));
    }
    
    #[test]
    fn test_disabled_management() {
        let state = create_test_state();
        
        assert!(!state.is_disabled("process"));
        
        state.set_disabled("process", true);
        assert!(state.is_disabled("process"));
        
        state.set_disabled("process", false);
        assert!(!state.is_disabled("process"));
    }
    
    #[test]
    fn test_override_paused() {
        let state = create_test_state();
        
        assert!(!state.is_override_paused("claude_local"));
        
        state.set_override_paused("claude_local", true);
        assert!(state.is_override_paused("claude_local"));
        
        state.set_override_paused("claude_local", false);
        assert!(!state.is_override_paused("claude_local"));
    }
    
    #[test]
    fn test_adapter_source() {
        let state = create_test_state();
        
        // 内置适配器
        assert_eq!(state.get_adapter_source("process"), AdapterSource::Builtin);
        
        // 添加外部覆盖
        state.add_external_plugin(AdapterPluginRecord {
            package_name: "test-adapter".to_string(),
            local_path: None,
            version: Some("1.0.0".to_string()),
            adapter_type: "process".to_string(),
            installed_at: "2024-01-01T00:00:00Z".to_string(),
        });
        
        assert_eq!(state.get_adapter_source("process"), AdapterSource::ExternalOverride);
        
        // 暂停覆盖
        state.set_override_paused("process", true);
        assert_eq!(state.get_adapter_source("process"), AdapterSource::BuiltinFallback);
    }
    
    #[test]
    fn test_availability() {
        let state = create_test_state();
        
        // 默认可用
        assert!(state.is_available("process"));
        
        // 禁用后不可用
        state.set_disabled("process", true);
        assert!(!state.is_available("process"));
        
        state.set_disabled("process", false);
        assert!(state.is_available("process"));
        
        // 覆盖暂停后不可用
        state.set_override_paused("process", true);
        assert!(!state.is_available("process"));
    }
}
