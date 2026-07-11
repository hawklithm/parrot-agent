pub mod adapter_trait;
pub mod registry;
pub mod process_adapter;
pub mod claude_local_adapter;

pub use adapter_trait::{AdapterType, ModelInfo, ServerAdapterModule, TestEnvironmentResult, TestEnvironmentInput};
pub use registry::AdapterRegistry;
pub use process_adapter::ProcessAdapter;
pub use claude_local_adapter::ClaudeLocalAdapter;

use std::sync::Arc;

/// 创建默认的适配器注册表，包含所有内置适配器
pub fn create_default_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    // 注册 Process 适配器
    registry.register(Arc::new(ProcessAdapter::new()));

    // 注册 Claude Local 适配器
    registry.register(Arc::new(ClaudeLocalAdapter::new()));

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registry() {
        let registry = create_default_registry();

        assert!(registry.contains(AdapterType::Process));
        assert!(registry.contains(AdapterType::ClaudeLocal));
        assert_eq!(registry.list_all().len(), 2);
    }
}
