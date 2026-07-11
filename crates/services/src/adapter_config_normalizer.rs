use std::collections::HashMap;
use serde_json::{Value as JsonValue, json};
use models::AdapterType;

/// 适配器配置标准化服务
/// 负责处理适配器配置的持久化前标准化、敏感信息处理等
pub struct AdapterConfigNormalizer;

impl AdapterConfigNormalizer {
    /// 标准化适配器配置以供持久化
    ///
    /// 处理敏感信息、移除临时字段、验证必需字段
    pub fn normalize_adapter_config_for_persistence(
        &self,
        adapter_type: AdapterType,
        config: &JsonValue,
    ) -> Result<JsonValue, String> {
        let mut normalized = config.clone();

        // 移除临时字段
        if let Some(obj) = normalized.as_object_mut() {
            obj.remove("_temp");
            obj.remove("_ui_state");
            obj.remove("_validation");
        }

        // 按适配器类型处理
        match adapter_type {
            AdapterType::ClaudeLocal => self.normalize_claude_local_config(&mut normalized)?,
            AdapterType::CodexLocal => self.normalize_codex_local_config(&mut normalized)?,
            AdapterType::Process => self.normalize_process_config(&mut normalized)?,
            AdapterType::Http => self.normalize_http_config(&mut normalized)?,
            _ => {
                // 其他适配器使用默认处理
                self.normalize_generic_config(&mut normalized)?;
            }
        }

        Ok(normalized)
    }

    /// 标准化运行时配置中的适配器配置
    ///
    /// 处理 runtime_config 中嵌套的适配器配置
    pub fn normalize_runtime_config_adapter_configs_for_persistence(
        &self,
        runtime_config: &JsonValue,
    ) -> Result<JsonValue, String> {
        let mut normalized = runtime_config.clone();

        // 处理 modelProfiles 中的适配器配置
        if let Some(obj) = normalized.as_object_mut() {
            if let Some(model_profiles) = obj.get_mut("modelProfiles") {
                if let Some(profiles_obj) = model_profiles.as_object_mut() {
                    for (_profile_name, profile_config) in profiles_obj.iter_mut() {
                        if let Some(adapter_config) = profile_config.get_mut("adapterConfig") {
                            // 递归标准化嵌套的适配器配置
                            self.normalize_nested_adapter_config(adapter_config)?;
                        }
                    }
                }
            }

            // 处理 env bindings
            if let Some(env) = obj.get_mut("env") {
                self.normalize_env_bindings(env)?;
            }
        }

        Ok(normalized)
    }

    /// 按适配器类型应用创建默认值
    ///
    /// 为新创建的Agent填充适配器特定的默认配置
    pub fn apply_create_defaults_by_adapter_type(
        &self,
        adapter_type: AdapterType,
        config: &JsonValue,
    ) -> JsonValue {
        let mut with_defaults = config.clone();

        match adapter_type {
            AdapterType::ClaudeLocal => {
                self.apply_claude_local_defaults(&mut with_defaults);
            }
            AdapterType::CodexLocal => {
                self.apply_codex_local_defaults(&mut with_defaults);
            }
            AdapterType::Process => {
                self.apply_process_defaults(&mut with_defaults);
            }
            AdapterType::CursorCloud => {
                self.apply_cursor_cloud_defaults(&mut with_defaults);
            }
            AdapterType::GeminiLocal => {
                self.apply_gemini_local_defaults(&mut with_defaults);
            }
            _ => {
                // 通用默认值
                self.apply_generic_defaults(&mut with_defaults);
            }
        }

        with_defaults
    }

    // ========== 私有辅助方法 ==========

    fn normalize_claude_local_config(&self, config: &mut JsonValue) -> Result<(), String> {
        let obj = config.as_object_mut()
            .ok_or("Config must be an object")?;

        // 确保必需字段存在
        if !obj.contains_key("model") {
            obj.insert("model".to_string(), json!("claude-opus-4"));
        }

        // 标准化引擎配置
        if let Some(engine) = obj.get("engine") {
            if engine == "auto" {
                obj.insert("engine".to_string(), json!("acp"));
            }
        }

        // 标准化 thinking effort
        if let Some(thinking_effort) = obj.get("thinkingEffort") {
            if thinking_effort.as_str() == Some("") {
                obj.remove("thinkingEffort");
            }
        }

        Ok(())
    }

    fn normalize_codex_local_config(&self, config: &mut JsonValue) -> Result<(), String> {
        let obj = config.as_object_mut()
            .ok_or("Config must be an object")?;

        // Codex 特定的标准化逻辑
        if !obj.contains_key("model") {
            obj.insert("model".to_string(), json!("codex"));
        }

        Ok(())
    }

    fn normalize_process_config(&self, config: &mut JsonValue) -> Result<(), String> {
        let obj = config.as_object_mut()
            .ok_or("Config must be an object")?;

        // Process 适配器标准化
        if !obj.contains_key("command") {
            return Err("Process adapter requires 'command' field".to_string());
        }

        // 标准化 args
        if let Some(args) = obj.get("args") {
            if args.as_str() == Some("") {
                obj.insert("args".to_string(), json!([]));
            }
        }

        Ok(())
    }

    fn normalize_http_config(&self, config: &mut JsonValue) -> Result<(), String> {
        let obj = config.as_object_mut()
            .ok_or("Config must be an object")?;

        // HTTP 适配器标准化
        if !obj.contains_key("url") {
            return Err("HTTP adapter requires 'url' field".to_string());
        }

        // 标准化 headers
        if !obj.contains_key("headers") {
            obj.insert("headers".to_string(), json!({}));
        }

        Ok(())
    }

    fn normalize_generic_config(&self, config: &mut JsonValue) -> Result<(), String> {
        // 通用配置标准化：移除空值、标准化字段格式
        if let Some(obj) = config.as_object_mut() {
            obj.retain(|_k, v| !v.is_null());
        }
        Ok(())
    }

    fn normalize_nested_adapter_config(&self, config: &mut JsonValue) -> Result<(), String> {
        // 递归处理嵌套的适配器配置
        if let Some(obj) = config.as_object_mut() {
            // 移除临时字段
            obj.remove("_temp");
            obj.remove("_ui_state");

            // 移除空值
            obj.retain(|_k, v| !v.is_null());
        }
        Ok(())
    }

    fn normalize_env_bindings(&self, env: &mut JsonValue) -> Result<(), String> {
        // 标准化环境变量绑定
        if let Some(obj) = env.as_object_mut() {
            for (_key, value) in obj.iter_mut() {
                // 标准化空字符串为 null
                if value.as_str() == Some("") {
                    *value = JsonValue::Null;
                }
            }
        }
        Ok(())
    }

    fn apply_claude_local_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("model".to_string())
                .or_insert(json!("claude-opus-4"));
            obj.entry("engine".to_string())
                .or_insert(json!("acp"));
            obj.entry("acpMode".to_string())
                .or_insert(json!("persistent"));
            obj.entry("acpNonInteractivePermissions".to_string())
                .or_insert(json!("deny"));
        }
    }

    fn apply_codex_local_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("model".to_string())
                .or_insert(json!("codex"));
            obj.entry("engine".to_string())
                .or_insert(json!("acp"));
        }
    }

    fn apply_process_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("args".to_string())
                .or_insert(json!([]));
            obj.entry("env".to_string())
                .or_insert(json!({}));
        }
    }

    fn apply_cursor_cloud_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("runtime".to_string())
                .or_insert(json!("cloud"));
            obj.entry("repos".to_string())
                .or_insert(json!([]));
        }
    }

    fn apply_gemini_local_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("model".to_string())
                .or_insert(json!("gemini-pro"));
            obj.entry("engine".to_string())
                .or_insert(json!("acp"));
        }
    }

    fn apply_generic_defaults(&self, config: &mut JsonValue) {
        if let Some(obj) = config.as_object_mut() {
            obj.entry("timeout".to_string())
                .or_insert(json!(300));
        }
    }
}

impl Default for AdapterConfigNormalizer {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_claude_local_config() {
        let normalizer = AdapterConfigNormalizer::default();
        let config = json!({
            "thinkingEffort": "",
            "engine": "auto",
            "_temp": "temporary_data"
        });

        let result = normalizer.normalize_adapter_config_for_persistence(
            AdapterType::ClaudeLocal,
            &config,
        ).unwrap();

        let obj = result.as_object().unwrap();
        assert!(!obj.contains_key("_temp"));
        assert!(!obj.contains_key("thinkingEffort"));
        assert_eq!(obj.get("engine").unwrap(), "acp");
        assert_eq!(obj.get("model").unwrap(), "claude-opus-4");
    }

    #[test]
    fn test_apply_claude_local_defaults() {
        let normalizer = AdapterConfigNormalizer::default();
        let config = json!({});

        let result = normalizer.apply_create_defaults_by_adapter_type(
            AdapterType::ClaudeLocal,
            &config,
        );

        let obj = result.as_object().unwrap();
        assert_eq!(obj.get("model").unwrap(), "claude-opus-4");
        assert_eq!(obj.get("engine").unwrap(), "acp");
        assert_eq!(obj.get("acpMode").unwrap(), "persistent");
    }

    #[test]
    fn test_normalize_process_config_requires_command() {
        let normalizer = AdapterConfigNormalizer::default();
        let config = json!({});

        let result = normalizer.normalize_adapter_config_for_persistence(
            AdapterType::Process,
            &config,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("command"));
    }

    #[test]
    fn test_normalize_runtime_config_model_profiles() {
        let normalizer = AdapterConfigNormalizer::default();
        let runtime_config = json!({
            "modelProfiles": {
                "cheap": {
                    "adapterConfig": {
                        "model": "claude-haiku",
                        "_temp": "temporary"
                    }
                }
            }
        });

        let result = normalizer.normalize_runtime_config_adapter_configs_for_persistence(
            &runtime_config,
        ).unwrap();

        let profiles = result.get("modelProfiles").unwrap()
            .get("cheap").unwrap()
            .get("adapterConfig").unwrap();
        assert!(!profiles.as_object().unwrap().contains_key("_temp"));
        assert_eq!(profiles.get("model").unwrap(), "claude-haiku");
    }

    #[test]
    fn test_normalize_env_bindings_removes_empty_strings() {
        let normalizer = AdapterConfigNormalizer::default();
        let runtime_config = json!({
            "env": {
                "API_KEY": "secret123",
                "EMPTY_VAR": "",
                "NULL_VAR": null
            }
        });

        let result = normalizer.normalize_runtime_config_adapter_configs_for_persistence(
            &runtime_config,
        ).unwrap();

        let env = result.get("env").unwrap().as_object().unwrap();
        assert_eq!(env.get("API_KEY").unwrap(), "secret123");
        assert!(env.get("EMPTY_VAR").unwrap().is_null());
    }
}
