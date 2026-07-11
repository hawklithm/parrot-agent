use serde_json::{Map, Value};

/// 脱敏配置中的敏感字段（如API密钥、密码等）
pub fn redact_config(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, val) in map {
                let key_lower = key.to_lowercase();

                // 检查是否是敏感字段
                if is_sensitive_field(&key_lower) {
                    // 如果值是对象或数组，递归处理；否则脱敏
                    match val {
                        Value::Object(_) | Value::Array(_) => {
                            redacted.insert(key.clone(), redact_config(val));
                        }
                        _ => {
                            redacted.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                        }
                    }
                } else if key_lower == "env" {
                    // 递敏感变量
                    redacted.insert(key.clone(), redact_env_object(val));
                } else {
                    // 递归处理嵌套对象
                    redacted.insert(key.clone(), redact_config(val));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| redact_config(v)).collect())
        }
        _ => value.clone(),
    }
}

/// 检查字段名是否是敏感字段
fn is_sensitive_field(key: &str) -> bool {
    const SENSITIVE_PATTERNS: &[&str] = &[
        "key",
        "token",
        "secret",
        "password",
        "credential",
        "auth",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "private_key",
        "client_secret",
    ];

    SENSITIVE_PATTERNS.iter().any(|pattern| key.contains(pattern))
}

/// 脱敏env对象
fn redact_env_object(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, val) in map {
                let key_upper = key.to_uppercase();

                // 检查环境变量名是否包含敏感信息
                if is_sensitive_env_var(&key_upper) {
                    redacted.insert(key.clone(), redact_env_binding(val));
                } else {
                    redacted.insert(key.clone(), val.clone());
                }
            }
            Value::Object(redacted)
        }
        _ => value.clone(),
    }
}

/// 检查环境变量名是否敏感
fn is_sensitive_env_var(key: &str) -> bool {
    const SENSITIVE_ENV_PATTERNS: &[&str] = &[
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "CREDENTIAL",
        "AUTH",
        "PRIVATE",
    ];

    SENSITIVE_ENV_PATTERNS.iter().any(|pattern| key.contains(pattern))
}

/// 脱敏环境变量绑定值
fn redact_env_binding(value: &Value) -> Value {
    match value {
        // 如果是字符串，直接脱敏
        Value::String(_) => Value::String("[REDACTED]".to_string()),
        // 如果是对象（如 {type: "plain", value: "..."}），保留结构但脱敏value
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, val) in map {
                if key == "value" {
                    redacted.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    redacted.insert(key.clone(), val.clone());
                }
            }
            Value::Object(redacted)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_redact_api_key() {
        let config = json!({
            "api_key": "sk-1234567890",
            "name": "test-agent"
        });

        let redacted = redact_config(&config);
        assert_eq!(redacted["api_key"], "[REDACTED]");
        assert_eq!(redacted["name"], "test-agent");
    }

    #[test]
    fn test_redact_env_vars() {
        let config = json!({
            "env": {
                "OPENAI_API_KEY": "sk-1234567890",
                "DATABASE_URL": "postgres://localhost"
            }
        });

        let redacted = redact_config(&config);
        assert_eq!(redacted["env"]["OPENAI_API_KEY"], "[REDACTED]");
        assert_eq!(redacted["env"]["DATABASE_URL"], "postgres://localhost");
    }

    #[test]
    fn test_redact_nested_objects() {
        let config = json!({
            "adapter": {
                "auth": {
                    "api_key": "secret-key"
                }
            }
        });

        let redacted = redact_config(&config);
        println!("Redacted: {}", serde_json::to_string_pretty(&redacted).unwrap());
        assert_eq!(redacted["adapter"]["auth"]["api_key"], "[REDACTED]");
    }

    #[test]
    fn test_redact_env_binding_object() {
        let config = json!({
            "env": {
                "OPENAI_API_KEY": {
                    "type": "plain",
                    "value": "sk-1234567890"
                }
            }
        });

        let redacted = redact_config(&config);
        let binding = &redacted["env"]["OPENAI_API_KEY"];
        assert_eq!(binding["type"], "plain");
        assert_eq!(binding["value"], "[REDACTED]");
    }
}
