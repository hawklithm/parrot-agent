use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use super::secret_service::{
    Secret, CreateSecretInput, UpdateSecretInput, SecretService, SecretServiceError,
    EnvBinding, ResolvedAdapterConfig, RuntimeSecretManifestEntry, SecretResolutionOutcome,
};
use serde_json::Value as JsonValue;

/// 数据库支持的密钥服务实现
pub struct DatabaseSecretService {
    pool: PgPool,
}

impl DatabaseSecretService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 检查是否为敏感字段名
    fn is_sensitive_field_name(field_name: &str) -> bool {
        let lower = field_name.to_lowercase();
        lower.contains("api_key")
            || lower.contains("apikey")
            || lower.contains("access_token")
            || lower.contains("auth_token")
            || lower.contains("authorization")
            || lower.contains("bearer")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("passwd")
            || lower.contains("credential")
            || lower.contains("jwt")
            || lower.contains("private_key")
            || lower.contains("privatekey")
            || lower == "token"
    }

    /// 检查值是否看起来像密钥（用于自动检测）
    fn looks_like_secret_value(value: &str) -> bool {
        // 基于长度和格式的启发式检测
        if value.len() < 16 {
            return false;
        }

        // 包含常见密钥前缀
        let prefixes = ["sk-", "pk-", "Bearer ", "eyJ", "ghp_", "gho_", "glpat-"];
        if prefixes.iter().any(|p| value.starts_with(p)) {
            return true;
        }

        // Base64 格式（至少20个字符，主要是字母数字和+/=）
        if value.len() >= 20 {
            let base64_chars = value.chars().filter(|c| c.is_alphanumeric() || *c == '+' || *c == '/' || *c == '=').count();
            if base64_chars as f32 / value.len() as f32 > 0.9 {
                return true;
            }
        }

        false
    }

    /// 自动将明文敏感值转换为密钥引用
    async fn auto_create_secret_from_plain_value(
        &self,
        company_id: Uuid,
        field_path: &str,
        value: &str,
    ) -> Result<Uuid, SecretServiceError> {
        // 生成密钥名称（基于字段路径）
        let key = format!("auto_{}", field_path.replace('.', "_"));

        let input = CreateSecretInput {
            key: key.clone(),
            value: value.to_string(),
            description: Some(format!("Auto-created from {}", field_path)),
        };

        let secret = self.create_secret(company_id, input).await?;
        Ok(secret.id)
    }

    /// 规范化环境变量配置
    async fn normalize_env_config(
        &self,
        company_id: Uuid,
        env_value: &JsonValue,
        auto_convert: bool,
    ) -> Result<JsonValue, SecretServiceError> {
        let env_obj = env_value
            .as_object()
            .ok_or_else(|| SecretServiceError::InvalidBinding("env must be an object".to_string()))?;

        let mut normalized = serde_json::Map::new();

        for (key, value) in env_obj {
            // 验证环境变量名
            if key.is_empty() || !key.chars().next().unwrap().is_ascii_alphabetic() && key.chars().next().unwrap() != '_' {
                return Err(SecretServiceError::InvalidEnvKey(key.clone()));
            }

            // 尝试解析为 EnvBinding
            let binding = match EnvBinding::from_value(value) {
                Ok(b) => b,
                Err(_) => {
                    // 如果不是结构化绑定，当作明文值
                    if let Some(s) = value.as_str() {
                        EnvBinding::Plain { value: s.to_string() }
                    } else {
                        return Err(SecretServiceError::InvalidBinding(
                            format!("Invalid binding for key: {}", key)
                        ));
                    }
                }
            };

            // 如果启用自动转换且是敏感的明文值，转换为 SecretRef
            let final_binding = match binding {
                EnvBinding::Plain { ref value } if auto_convert => {
                    let is_sensitive = Self::is_sensitive_field_name(key)
                        || Self::looks_like_secret_value(value);

                    if is_sensitive && !value.is_empty() {
                        let secret_id = self
                            .auto_create_secret_from_plain_value(
                                company_id,
                                &format!("env.{}", key),
                                value,
                            )
                            .await?;

                        EnvBinding::SecretRef {
                            secret_id,
                            version: "latest".to_string(),
                        }
                    } else {
                        binding
                    }
                }
                _ => binding,
            };

            normalized.insert(key.clone(), serde_json::to_value(&final_binding)?);
        }

        Ok(JsonValue::Object(normalized))
    }

    /// 解析环境变量配置
    async fn resolve_env_config(
        &self,
        company_id: Uuid,
        env_value: &JsonValue,
    ) -> Result<(serde_json::Map<String, JsonValue>, Vec<RuntimeSecretManifestEntry>), SecretServiceError> {
        let env_obj = match env_value.as_object() {
            Some(obj) => obj,
            None => return Ok((serde_json::Map::new(), Vec::new())),
        };

        let mut resolved_env = serde_json::Map::new();
        let mut manifest = Vec::new();

        for (key, value) in env_obj {
            let binding = EnvBinding::from_value(value)?;

            match binding {
                EnvBinding::Plain { value } => {
                    resolved_env.insert(key.clone(), JsonValue::String(value));
                }
                EnvBinding::SecretRef { secret_id, version } => {
                    // 从数据库获取密钥值
                    match self.get_secret(company_id, secret_id).await {
                        Ok(secret) => {
                            resolved_env.insert(key.clone(), JsonValue::String(secret.value.clone()));

                            manifest.push(RuntimeSecretManifestEntry {
                                config_path: format!("env.{}", key),
                                env_key: Some(key.clone()),
                                secret_id,
                                secret_key: secret.key.clone(),
                                version: version.clone(),
                                outcome: SecretResolutionOutcome::Success,
                                error_code: None,
                            });
                        }
                        Err(e) => {
                            // 密钥解析失败，记录到 manifest 但不中断
                            tracing::warn!("Failed to resolve secret {} for env.{}: {}", secret_id, key, e);

                            manifest.push(RuntimeSecretManifestEntry {
                                config_path: format!("env.{}", key),
                                env_key: Some(key.clone()),
                                secret_id,
                                secret_key: format!("unknown-{}", secret_id),
                                version: version.clone(),
                                outcome: SecretResolutionOutcome::Failure,
                                error_code: Some("SECRET_NOT_FOUND".to_string()),
                            });
                        }
                    }
                }
                EnvBinding::UserSecretRef { key: user_key, required, .. } => {
                    // 用户密钥从环境变量获取
                    match std::env::var(&user_key) {
                        Ok(value) => {
                            resolved_env.insert(key.clone(), JsonValue::String(value));
                        }
                        Err(_) if required => {
                            return Err(SecretServiceError::ResolutionFailed(
                                format!("Required user secret not found: {}", user_key)
                            ));
                        }
                        Err(_) => {
                            // 非必需的用户密钥缺失，跳过
                            tracing::debug!("Optional user secret not found: {}", user_key);
                        }
                    }
                }
            }
        }

        Ok((resolved_env, manifest))
    }
}

#[async_trait]
impl SecretService for DatabaseSecretService {
    async fn normalize_adapter_config_for_persistence(
        &self,
        company_id: Uuid,
        adapter_config: JsonValue,
        _adapter_type: Option<&str>,
    ) -> Result<JsonValue, SecretServiceError> {
        let mut config_obj = adapter_config
            .as_object()
            .ok_or_else(|| {
                SecretServiceError::InvalidBinding("adapter_config must be an object".to_string())
            })?
            .clone();

        // 规范化 env 字段（启用自动转换）
        if let Some(env_value) = config_obj.get("env") {
            let normalized_env = self.normalize_env_config(company_id, env_value, true).await?;
            config_obj.insert("env".to_string(), normalized_env);
        }

        // 规范化顶层敏感字段
        for (key, value) in config_obj.clone().iter() {
            if Self::is_sensitive_field_name(key) {
                if let Some(plain_value) = value.as_str() {
                    if !plain_value.is_empty() && Self::looks_like_secret_value(plain_value) {
                        // 自动创建密钥
                        let secret_id = self
                            .auto_create_secret_from_plain_value(
                                company_id,
                                key,
                                plain_value,
                            )
                            .await?;

                        let secret_ref = EnvBinding::SecretRef {
                            secret_id,
                            version: "latest".to_string(),
                        };

                        config_obj.insert(key.clone(), serde_json::to_value(&secret_ref)?);
                    }
                }
            }
        }

        Ok(JsonValue::Object(config_obj))
    }

    async fn resolve_adapter_config_for_runtime(
        &self,
        company_id: Uuid,
        adapter_config: JsonValue,
    ) -> Result<ResolvedAdapterConfig, SecretServiceError> {
        let config_obj = adapter_config
            .as_object()
            .ok_or_else(|| {
                SecretServiceError::InvalidBinding("adapter_config must be an object".to_string())
            })?;

        let mut resolved = config_obj.clone();
        let mut secret_keys = Vec::new();
        let mut manifest = Vec::new();

        // 解析 env 字段
        if let Some(env_value) = config_obj.get("env") {
            let (resolved_env, env_manifest) = self.resolve_env_config(company_id, env_value).await?;

            for key in resolved_env.keys() {
                secret_keys.push(format!("env.{}", key));
            }

            manifest.extend(env_manifest);
            resolved.insert("env".to_string(), JsonValue::Object(resolved_env));
        }

        // 解析顶层 SecretRef 字段
        for (key, value) in config_obj.iter() {
            if key == "env" {
                continue; // 已处理
            }

            if let Ok(binding) = EnvBinding::from_value(value) {
                if let EnvBinding::SecretRef { secret_id, version } = binding {
                    match self.get_secret(company_id, secret_id).await {
                        Ok(secret) => {
                            resolved.insert(key.clone(), JsonValue::String(secret.value.clone()));
                            secret_keys.push(key.clone());

                            manifest.push(RuntimeSecretManifestEntry {
                                config_path: key.clone(),
                                env_key: None,
                                secret_id,
                                secret_key: secret.key.clone(),
                                version: version.clone(),
                                outcome: SecretResolutionOutcome::Success,
                                error_code: None,
                            });
                        }
                        Err(e) => {
                  tracing::warn!("Failed to resolve secret {} for field {}: {}", secret_id, key, e);

                            manifest.push(RuntimeSecretManifestEntry {
                                config_path: key.clone(),
                                env_key: None,
                                secret_id,
                                secret_key: format!("unknown-{}", secret_id),
                                version: version.clone(),
                                outcome: SecretResolutionOutcome::Failure,
                                error_code: Some("SECRET_NOT_FOUND".to_string()),
                            });
                        }
                    }
                }
            }
        }

        Ok(ResolvedAdapterConfig {
            config: JsonValue::Object(resolved),
            secret_keys,
            manifest,
        })
    }

    fn redact_config(&self, adapter_config: JsonValue) -> JsonValue {
        let mut config_obj = match adapter_config.as_object() {
            Some(obj) => obj.clone(),
            None => return adapter_config,
        };

        // 脱敏 env 字段
        if let Some(env_value) = config_obj.get("env") {
            if let Some(env_obj) = env_value.as_object() {
                let mut redacted_env = serde_json::Map::new();

                for (key, value) in env_obj {
                    // 如果是 SecretRef，保留结构但隐藏 secret_id
                    if let Ok(binding) = EnvBinding::from_value(value) {
                        match binding {
                            EnvBinding::SecretRef { version, .. } => {
                                redacted_env.insert(
                                    key.clone(),
                                    serde_json::json!({
                                        "type": "secret_ref",
                                        "secretId": "***REDACTED***",
                                        "version": version
                                    }),
                                );
                                continue;
                            }
                            EnvBinding::Plain { .. } if Self::is_sensitive_field_name(key) => {
                                redacted_env.insert(
                                    key.clone(),
                                    JsonValue::String("***REDACTED***".to_string()),
                                );
                                continue;
                            }
                            _ => {}
                        }
                    }

                    redacted_env.insert(key.clone(), value.clone());
                }

                config_obj.insert("env".to_string(), JsonValue::Object(redacted_env));
            }
        }

        // 脱敏已知敏感字段
        for (key, value) in config_obj.clone().iter() {
            if Self::is_sensitive_field_name(key) {
                // 如果是 SecretRef，保留结构
                if let Ok(EnvBinding::SecretRef { version, .. }) = EnvBinding::from_value(value) {
                    config_obj.insert(
                        key.clone(),
                        serde_json::json!({
                            "type": "secret_ref",
                            "secretId": "***REDACTED***",
                            "version": version
                        }),
                    );
                } else {
                    config_obj.insert(
                        key.clone(),
                        JsonValue::String("***REDACTED***".to_string()),
                    );
                }
            }
        }

        JsonValue::Object(config_obj)
    }

    async fn create_secret(
        &self,
        company_id: Uuid,
        input: CreateSecretInput,
    ) -> Result<Secret, SecretServiceError> {
        let secret_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO company_secrets (id, company_id, key, value, description, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            secret_id,
            company_id,
            input.key,
            input.value,
            input.description,
            now,
            now,
        )
        .execute(&self.pool)
        .await?;

        Ok(Secret {
            id: secret_id,
            company_id,
            key: input.key,
            value: input.value,
            description: input.description,
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<Secret, SecretServiceError> {
        let row = sqlx::query!(
            r#"
            SELECT id, company_id, key, value, description, created_at, updated_at
            FROM company_secrets
            WHERE id = $1 AND company_id = $2
            "#,
            secret_id,
            company_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;

        Ok(Secret {
            id: row.id,
            company_id: row.company_id,
            key: row.key,
            value: row.value,
            description: row.description,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn update_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
        input: UpdateSecretInput,
    ) -> Result<Secret, SecretServiceError> {
        let now = chrono::Utc::now();

        // 构建动态更新 SQL
        let mut updates = Vec::new();
        let mut values: Vec<Box<dyn sqlx::Encode<'_, sqlx::Postgres> + Send + 'static>> = Vec::new();
        let mut param_index = 1;

        if let Some(value) = input.value {
            updates.push(format!("value = ${}", param_index));
            values.push(Box::new(value));
            param_index += 1;
        }

        if let Some(description) = input.description {
            updates.push(format!("description = ${}", param_index));
            values.push(Box::new(description));
            param_index += 1;
        }

        updates.push(format!("updated_at = ${}", param_index));

        if updates.is_empty() {
            // 没有更新，直接返回当前密钥
            return self.get_secret(company_id, secret_id).await;
        }

        let query_str = format!(
            "UPDATE company_secrets SET {} WHERE id = ${} AND company_id = ${} RETURNING id, company_id, key, value, description, created_at, updated_at",
            updates.join(", "),
            param_index + 1,
            param_index + 2,
        );

        // 由于参数绑定的限制，这里使用简化版本
        // 实际生产环境可以使用 sqlx::query_builder 或其他方法
        let row = sqlx::query!(
            r#"
            UPDATE company_secrets
            SET value = COALESCE($1, value),
                description = COALESCE($2, description),
                updated_at = $3
            WHERE id = $4 AND company_id = $5
            RETURNING id, company_id, key, value, description, created_at, updated_at
            "#,
            input.value,
            input.description,
            now,
            secret_id,
            company_id,
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;

        Ok(Secret {
            id: row.id,
            company_id: row.company_id,
            key: row.key,
            value: row.value,
            description: row.description,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    async fn delete_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<(), SecretServiceError> {
        let result = sqlx::query!(
            r#"
            DELETE FROM company_secrets
            WHERE id = $1 AND company_id = $2
            "#,
            secret_id,
            company_id,
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(SecretServiceError::SecretNotFound(secret_id.to_string()));
        }

        Ok(())
    }

    async fn list_secrets(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<Secret>, SecretServiceError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, company_id, key, description, created_at, updated_at
            FROM company_secrets
            WHERE company_id = $1
            ORDER BY created_at DESC
            "#,
            company_id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Secret {
                id: row.id,
                company_id: row.company_id,
                key: row.key,
                value: "***REDACTED***".to_string(), // 列表中不返回实际值
                description: row.description,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sensitive_field_name() {
        assert!(DatabaseSecretService::is_sensitive_field_name("api_key"));
        assert!(DatabaseSecretService::is_sensitive_field_name("API_KEY"));
        assert!(DatabaseSecretService::is_sensitive_field_name("database_password"));
        assert!(DatabaseSecretService::is_sensitive_field_name("bearer_token"));
        assert!(!DatabaseSecretService::is_sensitive_field_name("database_host"));
        assert!(!DatabaseSecretService::is_sensitive_field_name("port"));
    }

    #[test]
    fn test_looks_like_secret_value() {
        assert!(DatabaseSecretService::looks_like_secret_value("sk-1234567890abcdef1234567890abcdef"));
        assert!(DatabaseSecretService::looks_like_secret_value("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(DatabaseSecretService::looks_like_secret_value("ghp_1234567890abcdefABCDEF1234567890"));
        assert!(!DatabaseSecretService::looks_like_secret_value("short"));
        assert!(!DatabaseSecretService::looks_like_secret_value("this is just text"));
    }
}
