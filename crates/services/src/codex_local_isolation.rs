use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

/// Codex Local 适配器的环境隔离
///
/// codex_local agents 默认继承主机的 ~/.codex 或 $CODEX_HOME 登录信息。
/// 我们只在 agent 设置了自己的 OPENAI_API_KEY 时才为其创建隔离的 CODEX_HOME，
/// 这样该 key 的 api-key auth.json 不会与其他 agent 使用的共享公司 home 冲突。
/// 没有 key 的 agents 共享主机凭证。
pub struct CodexLocalIsolation;

impl CodexLocalIsolation {
    /// 应用 Codex Local 密钥隔离
    ///
    /// # Arguments
    /// * `company_id` - 公司 ID
    /// * `agent_id` - Agent ID
    /// * `adapter_type` - 适配器类型
    /// * `adapter_config` - 适配器配置
    ///
    /// # Returns
    /// 修改后的适配器配置（如果需要隔离）
    pub fn apply_codex_local_key_isolation(
        company_id: Uuid,
        agent_id: Uuid,
        adapter_type: &str,
        adapter_config: Value,
    ) -> Value {
        // 只处理 codex_local 适配器
        if adapter_type != "codex_local" {
            return adapter_config;
        }

        let mut config = match adapter_config.as_object() {
            Some(obj) => obj.clone(),
            None => return adapter_config,
        };

        // 获取现有的 env 配置
        let existing_env = match config.get("env").and_then(|v| v.as_object()) {
            Some(env) => env.clone(),
            None => return Value::Object(config),
        };

        // 检查是否配置了 OPENAI_API_KEY
        if !Self::codex_local_env_key_configured(existing_env.get("OPENAI_API_KEY")) {
            return Value::Object(config);
        }

        // 如果已经配置了 CODEX_HOME，不覆盖
        if Self::codex_local_env_key_configured(existing_env.get("CODEX_HOME")) {
            return Value::Object(config);
        }

        // 添加隔离的 CODEX_HOME
        let mut new_env = existing_env;
        let codex_home = Self::codex_local_agent_home(company_id, agent_id);
        new_env.insert(
            "CODEX_HOME".to_string(),
            Value::String(codex_home.to_string_lossy().to_string()),
        );

        config.insert("env".to_string(), Value::Object(new_env));
        Value::Object(config)
    }

    /// 生成 Agent 的 Codex home 路径
    ///
    /// 路径格式：{instance_root}/companies/{company_id}/agents/{agent_id}/codex-home
    fn codex_local_agent_home(company_id: Uuid, agent_id: Uuid) -> PathBuf {
        let instance_root = Self::resolve_paperclip_instance_root();
        instance_root
            .join("companies")
            .join(company_id.to_string())
            .join("agents")
            .join(agent_id.to_string())
            .join("codex-home")
    }

    /// 解析 Paperclip 实例根目录
    fn resolve_paperclip_instance_root() -> PathBuf {
        // 优先使用 PAPERCLIP_HOME
        if let Ok(home) = std::env::var("PAPERCLIP_HOME") {
            if !home.is_empty() {
                return PathBuf::from(home);
            }
        }

        // 使用 PAPERCLIP_INSTANCE_ID 构建路径
        if let Ok(instance_id) = std::env::var("PAPERCLIP_INSTANCE_ID") {
            if !instance_id.is_empty() {
                if let Ok(home_dir) = std::env::var("HOME") {
                    return PathBuf::from(home_dir)
                        .join(".paperclip")
                        .join("instances")
                        .join(instance_id);
                }
            }
        }

        // 默认使用 ~/.paperclip
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".paperclip"))
            .unwrap_or_else(|_| PathBuf::from(".paperclip"))
    }

    /// 检查环境变量值是否已配置
    ///
    /// 支持两种格式：
    /// 1. 环境变量绑定字符串（如 "${ENV_VAR}"）
    /// 2. Secret 引用（{ "type": "secret_ref", "secretId": "..." }）
    fn codex_local_env_key_configured(value: Option<&Value>) -> bool {
        match value {
            Some(Value::String(s)) => {
                // 检查是否是环境变量绑定
                Self::is_env_binding_string(s)
            }
            Some(Value::Object(obj)) => {
                // 检查是否是 secret_ref
                obj.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "secret_ref")
                    .unwrap_or(false)
                    && obj.get("secretId").and_then(|s| s.as_str()).is_some()
            }
            _ => false,
        }
    }

    /// 检查字符串是否是环境变量绑定格式
    fn is_env_binding_string(s: &str) -> bool {
        s.starts_with("${") && s.ends_with('}')
    }

    /// 清理 Agent 的 Codex home 目录
    ///
    /// # Arguments
    /// * `company_id` - 公司 ID
    /// * `agent_id` - Agent ID
    ///
    /// # Returns
    /// 清理是否成功
    pub fn cleanup_codex_local_agent_home(
        company_id: Uuid,
        agent_id: Uuid,
    ) -> Result<(), std::io::Error> {
        let home_path = Self::codex_local_agent_home(company_id, agent_id);

        if home_path.exists() {
            std::fs::remove_dir_all(&home_path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_apply_isolation_non_codex_local() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({ "model": "gpt-4" });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "anthropic",
            config.clone(),
        );

        assert_eq!(result, config);
    }

    #[test]
    fn test_apply_isolation_no_env() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({ "model": "codex" });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "codex_local",
            config.clone(),
        );

        assert_eq!(result, config);
    }

    #[test]
    fn test_apply_isolation_no_openai_key() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({
            "env": {
                "OTHER_VAR": "value"
            }
        });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "codex_local",
            config.clone(),
        );

        assert_eq!(result, config);
    }

    #[test]
    fn test_apply_isolation_with_openai_key() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({
            "env": {
                "OPENAI_API_KEY": "${MY_KEY}"
            }
        });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "codex_local",
            config.clone(),
        );

        let result_obj = result.as_object().unwrap();
        let env = result_obj.get("env").unwrap().as_object().unwrap();

        assert!(env.contains_key("CODEX_HOME"));
        assert!(env.get("CODEX_HOME").unwrap().as_str().unwrap().contains(&agent_id.to_string()));
    }

    #[test]
    fn test_apply_isolation_with_secret_ref() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({
            "env": {
                "OPENAI_API_KEY": {
                    "type": "secret_ref",
                    "secretId": "secret-123"
                }
            }
        });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "codex_local",
            config.clone(),
        );

        let result_obj = result.as_object().unwrap();
        let env = result_obj.get("env").unwrap().as_object().unwrap();

        assert!(env.contains_key("CODEX_HOME"));
    }

    #[test]
    fn test_apply_isolation_with_existing_codex_home() {
        let company_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();
        let config = json!({
            "env": {
                "OPENAI_API_KEY": "${MY_KEY}",
                "CODEX_HOME": "/custom/path"
            }
        });

        let result = CodexLocalIsolation::apply_codex_local_key_isolation(
            company_id,
            agent_id,
            "codex_local",
            config.clone(),
        );

        // 不应该修改已存在的 CODEX_HOME
        assert_eq!(result, config);
    }

    #[test]
    fn test_is_env_binding_string() {
        assert!(CodexLocalIsolation::is_env_binding_string("${VAR}"));
        assert!(CodexLocalIsolation::is_env_binding_string("${MY_KEY}"));
        assert!(!CodexLocalIsolation::is_env_binding_string("plain_value"));
        assert!(!CodexLocalIsolation::is_env_binding_string("$VAR"));
        assert!(!CodexLocalIsolation::is_env_binding_string("${VAR"));
    }

    #[test]
    fn test_codex_local_env_key_configured() {
        // 环境变量绑定
        assert!(CodexLocalIsolation::codex_local_env_key_configured(Some(
            &json!("${MY_KEY}")
        )));

        // Secret 引用
        assert!(CodexLocalIsolation::codex_local_env_key_configured(Some(
            &json!({
                "type": "secret_ref",
                "secretId": "secret-123"
            })
        )));

        // 普通字符串（不是绑定格式）
        assert!(!CodexLocalIsolation::codex_local_env_key_configured(Some(
            &json!("plain_value")
        )));

        // None
        assert!(!CodexLocalIsolation::codex_local_env_key_configured(None));
    }

    #[test]
    fn test_codex_local_agent_home_path() {
        let company_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let agent_id = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();

        let home = CodexLocalIsolation::codex_local_agent_home(company_id, agent_id);
        let home_str = home.to_string_lossy();

        assert!(home_str.contains("companies"));
        assert!(home_str.contains("550e8400-e29b-41d4-a716-446655440000"));
        assert!(home_str.contains("agents"));
        assert!(home_str.contains("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
        assert!(home_str.contains("codex-home"));
    }
}
