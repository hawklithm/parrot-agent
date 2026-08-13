use std::sync::LazyLock;
use std::collections::HashSet;

/// 内置适配器类型集合（对齐 Paperclip 的 BUILTIN_ADAPTER_TYPES）
/// 
/// 这些适配器是 Parrot-Agent 服务器内置的，不能被删除。
/// 与 Paperclip `server/src/adapters/builtin-adapter-types.ts` 逐项对齐。
/// 外部 plugin 可以 override（registry 会保留 builtin fallback），但不能删除。
pub const BUILTIN_ADAPTER_TYPE_LIST: [&str; 14] = [
    "acpx_local",
    "claude_local",
    "codex_local",
    "cursor_cloud",
    "cursor",
    "gemini_local",
    "grok_local",
    "hermes_gateway",
    "hermes_local",
    "openclaw_gateway",
    "opencode_local",
    "pi_local",
    "process",
    "http",
];

pub static BUILTIN_ADAPTER_TYPES: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| BUILTIN_ADAPTER_TYPE_LIST.iter().copied().collect());

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
        assert!(types.contains(&"http"));
        assert_eq!(types.len(), BUILTIN_ADAPTER_TYPE_LIST.len());
    }

    /// 内置集合必须是 `models::AdapterType` 的子集，避免出现无法解析的类型串。
    #[test]
    fn builtin_types_are_known_adapter_types() {
        for t in BUILTIN_ADAPTER_TYPE_LIST {
            assert!(
                models::AdapterType::from_str(t).is_some(),
                "unknown builtin adapter type: {t}"
            );
        }
    }
}
