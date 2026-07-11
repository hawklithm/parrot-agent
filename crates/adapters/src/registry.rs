use std::collections::HashMap;
use std::sync::Arc;

use super::adapter_trait::{AdapterType, ServerAdapterModule};

/// Adapter Registry - 管理所有注册的适配器
pub struct AdapterRegistry {
    adapters: HashMap<AdapterType, Arc<dyn ServerAdapterModule>>,
}

impl AdapterRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// 注册一个适配器
    pub fn register(&mut self, adapter: Arc<dyn ServerAdapterModule>) {
        let adapter_type = adapter.adapter_type();
        self.adapters.insert(adapter_type, adapter);
    }

    /// 查找适配器
    pub fn find_server_adapter(&self, adapter_type: AdapterType) -> Option<Arc<dyn ServerAdapterModule>> {
        self.adapters.get(&adapter_type).cloned()
    }

    /// 列举所有已注册的适配器
    pub fn list_all(&self) -> Vec<Arc<dyn ServerAdapterModule>> {
        self.adapters.values().cloned().collect()
    }

    /// 检查适配器是否已注册
    pub fn contains(&self, adapter_type: AdapterType) -> bool {
        self.adapters.contains_key(&adapter_type)
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_operations() {
        let mut registry = AdapterRegistry::new();
        assert_eq!(registry.list_all().len(), 0);
        assert!(!registry.contains(AdapterType::Process));
    }
}
