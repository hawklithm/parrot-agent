use std::sync::LazyLock;
use std::collections::HashSet;

/// 内置适配器类型集合（对齐 Paperclip 的 BUILTIN_ADAPTER_TYPES）
/// 
/// 这些适配器是 Parrot-Agent 服务器内置的，不能被删除。
pub static BUILTIN_ADAPTER_TYPES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("process");
    set.insert("claude_local");
    // 可以根据需要添加更多内置类型
    set
});

/// 检查给定的适配器类型是否为内置类型
pub fn is_builtin_adapter_type(adapter_type: &str) -> bool {
    BUILTIN_ADAPTER_TYPES.contains(adapter_type)
}

/// 列出所有内置适配器类型
pub fn list_builtin_adapter_types() -> Vec<&'static str> {
    BUILTIN_ADAPTER_TYPES.iter().copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_builtin_types() {
        assert!(is_builtin_adapter_type("process"));
        assert!(is_builtin_adapter_type("claude_local"));
        assert!(is_builtin_adapter_type("codex_local"));
        assert!(!is_builtin_adapter_type("custom_external"));
    }
    
    #[test]
    fn test_list_builtin_types() {
        let types = list_builtin_adapter_types();
        assert!(types.contains(&"process"));
        assert!(types.len() >= 3);
    }
}
