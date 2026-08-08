use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use uuid::Uuid;
use sqlx::{PgPool, Row};
use sha2::{Digest, Sha256};

/// 密钥服务错误
#[derive(Debug, Error)]
pub enum SecretServiceError {
    #[error("Secret not found: {0}")]
    SecretNotFound(String),

    #[error("Invalid binding: {0}")]
    InvalidBinding(String),

    #[error("Redacted sentinel cannot be persisted: {0}")]
    RedactedSentinel(String),

    #[error("Secret resolution failed: {0}")]
    ResolutionFailed(String),

    #[error("Invalid environment key: {0}")]
    InvalidEnvKey(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// 环境变量绑定类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvBinding {
    /// 明文值
    Plain { value: String },

    /// 密钥引用（公司级密钥）
    SecretRef {
        secret_id: Uuid,
        #[serde(default = "default_version")]
        version: String,
    },

    /// 用户密钥引用（需用户提供）
    UserSecretRef {
        key: String,
        #[serde(default = "default_version")]
        version: String,
        #[serde(default = "default_true")]
        required: bool,
        #[serde(default)]
        allow_missing_override: bool,
    },
}

fn default_version() -> String {
    "latest".to_string()
}

fn default_true() -> bool {
    true
}

impl EnvBinding {
    /// 从字符串或JSON创建绑定
    pub fn from_value(value: &JsonValue) -> Result<Self, SecretServiceError> {
        if let Some(s) = value.as_str() {
            return Ok(EnvBinding::Plain {
                value: s.to_string(),
            });
        }

        serde_json::from_value(value.clone())
            .map_err(|e| SecretServiceError::InvalidBinding(e.to_string()))
    }

    /// 标准化绑定（规范化字段）
    pub fn canonicalize(self) -> Self {
        match self {
            EnvBinding::Plain { value } => EnvBinding::Plain { value },
            EnvBinding::SecretRef { secret_id, version } => EnvBinding::SecretRef {
                secret_id,
                version: if version.is_empty() {
                    "latest".to_string()
                } else {
                    version
                },
            },
            EnvBinding::UserSecretRef {
                key,
                version,
                required,
                allow_missing_override,
            } => EnvBinding::UserSecretRef {
                key,
                version: if version.is_empty() {
                    "latest".to_string()
                } else {
                    version
                },
                required,
                allow_missing_override,
            },
        }
    }
}

/// 密钥引用（用于持久化存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretReference {
    pub secret_id: Uuid,
    pub version: String,
}

/// 运行时密钥清单条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSecretManifestEntry {
    pub config_path: String,
    pub env_key: Option<String>,
    pub secret_id: Uuid,
    pub secret_key: String,
    pub version: String,
    pub outcome: SecretResolutionOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretResolutionOutcome {
    Success,
    Failure,
}

/// 适配器配置解析结果（用于运行时）
#[derive(Debug, Clone)]
pub struct ResolvedAdapterConfig {
    pub config: JsonValue,
    pub secret_keys: Vec<String>,
    pub manifest: Vec<RuntimeSecretManifestEntry>,
}

/// 密钥数据结构
///
/// `value` 是**运行时明文**（按 latest_version 解密 `company_secret_versions.material` 得到）。
/// 列表中为 `None`（不返回实际值）。数据库层无扁平 `value` 列（paperclip 版本化模型）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub id: Uuid,
    pub company_id: Uuid,
    pub key: String,
    pub name: String,
    pub provider: String,
    pub status: String,
    pub scope: String,
    pub description: Option<String>,
    pub latest_version: i32,
    pub value: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 密钥创建输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSecretInput {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

/// 密钥更新输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSecretInput {
    pub value: Option<String>,
    pub description: Option<String>,
}

/// 密钥服务 trait
#[async_trait]
pub trait SecretService: Send + Sync {
    /// 规范化适配器配置以便持久化
    ///
    /// # 功能
    /// - 检测敏感字段（env.*, adapter schema中标记为secret的字段）
    /// - 将明文值转换为SecretRef（自动创建密钥）
    /// - 验证SecretRef指向的密钥存在且属于同公司
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - adapter_config: 原始适配器配置
    /// - adapter_type: 适配器类型（用于推断敏感字段）
    ///
    /// # 返回
    /// - Ok(JsonValue): 规范化后的配置（明文值已替换为SecretRef）
    /// - Err: 规范化失败
    async fn normalize_adapter_config_for_persistence(
        &self,
        company_id: Uuid,
        adapter_config: JsonValue,
        adapter_type: Option<&str>,
    ) -> Result<JsonValue, SecretServiceError>;

    /// 解析适配器配置以便运行时使用
    ///
    /// # 功能
    /// - 解析SecretRef，获取实际密钥值
    /// - 解析UserSecretRef，从用户环境获取密钥
    /// - 构建运行时密钥清单（用于审计和指纹计算）
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - adapter_config: 持久化的适配器配置（含SecretRef）
    ///
    /// # 返回
    /// - Ok(ResolvedAdapterConfig): 解析后的配置（SecretRef已替换为实际值）
    /// - Err: 解析失败
    async fn resolve_adapter_config_for_runtime(
        &self,
        company_id: Uuid,
        adapter_config: JsonValue,
    ) -> Result<ResolvedAdapterConfig, SecretServiceError>;

    /// 脱敏配置（用于API响应）
    ///
    /// # 功能
    /// - 将敏感字段值替换为 "***REDACTED***"
    /// - 保留SecretRef结构（但隐藏secret_id）
    ///
    /// # 参数
    /// - adapter_config: 适配器配置
    ///
    /// # 返回
    /// - 脱敏后的配置
    fn redact_config(&self, adapter_config: JsonValue) -> JsonValue;

    /// 创建新密钥
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - input: 密钥创建输入
    ///
    /// # 返回
    /// - Ok(Secret): 创建的密钥
    /// - Err: 创建失败
    async fn create_secret(
        &self,
        company_id: Uuid,
        input: CreateSecretInput,
    ) -> Result<Secret, SecretServiceError>;

    /// 获取密钥
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - secret_id: 密钥ID
    ///
    /// # 返回
    /// - Ok(Secret): 密钥信息
    /// - Err: 获取失败
    async fn get_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<Secret, SecretServiceError>;

    /// 更新密钥
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - secret_id: 密钥ID
    /// - input: 密钥更新输入
    ///
    /// # 返回
    /// - Ok(Secret): 更新后的密钥
    /// - Err: 更新失败
    async fn update_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
        input: UpdateSecretInput,
    ) -> Result<Secret, SecretServiceError>;

    /// 删除密钥
    ///
    /// # 参数
    /// - company_id: 公司ID
    /// - secret_id: 密钥ID
    ///
    /// # 返回
    /// - Ok(()): 删除成功
    /// - Err: 删除失败
    async fn delete_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<(), SecretServiceError>;

    /// 列出公司的所有密钥
    ///
    /// # 参数
    /// - company_id: 公司ID
    ///
    /// # 返回
    /// - Ok(Vec<Secret>): 密钥列表（value字段已脱敏）
    /// - Err: 列出失败
    async fn list_secrets(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<Secret>, SecretServiceError>;
}

/// 默认密钥服务实现（占位符）
pub struct DefaultSecretService {
    pool: Option<PgPool>,
}

impl DefaultSecretService {
    pub fn new() -> Self {
        Self { pool: None }
    }

    pub fn with_pool(pool: PgPool) -> Self {
        Self { pool: Some(pool) }
    }

    fn pool(&self) -> Result<&PgPool, SecretServiceError> {
        self.pool.as_ref().ok_or_else(|| SecretServiceError::ResolutionFailed("secret persistence is not configured".into()))
    }

    fn row_to_secret(row: &sqlx::postgres::PgRow, value: Option<String>) -> Secret {
        Secret {
            id: row.get("id"), company_id: row.get("company_id"), key: row.get("key"),
            name: row.get("name"), provider: row.get("provider"), status: row.get("status"),
            scope: row.get("scope"), description: row.get("description"),
            latest_version: row.get("latest_version"), value,
            created_at: row.get("created_at"), updated_at: row.get("updated_at"),
        }
    }

    /// 检查环境变量名是否合法
    fn is_valid_env_key(key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        let first = key.chars().next().unwrap();
        if !first.is_ascii_alphabetic() && first != '_' {
            return false;
        }
        key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// 检查是否为敏感环境变量名
    fn is_sensitive_env_key(key: &str) -> bool {
        let lower = key.to_lowercase();
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
            || lower.contains("cookie")
    }

    /// 规范化环境变量配置
    async fn normalize_env_config(
        &self,
        _company_id: Uuid,
        env_value: &JsonValue,
    ) -> Result<JsonValue, SecretServiceError> {
        let env_obj = env_value
            .as_object()
            .ok_or_else(|| SecretServiceError::InvalidBinding("env must be an object".to_string()))?;

        let mut normalized = serde_json::Map::new();

        for (key, value) in env_obj {
            if !Self::is_valid_env_key(key) {
                return Err(SecretServiceError::InvalidEnvKey(key.clone()));
            }

            let binding = EnvBinding::from_value(value)?;
            normalized.insert(key.clone(), serde_json::to_value(&binding)?);
        }

        Ok(JsonValue::Object(normalized))
    }
}

impl Default for DefaultSecretService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretService for DefaultSecretService {
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

        // 规范化 env 字段
        if let Some(env_value) = config_obj.get("env") {
            let normalized_env = self.normalize_env_config(company_id, env_value).await?;
            config_obj.insert("env".to_string(), normalized_env);
        }

        // TODO: 规范化 adapter schema 中标记为 secret 的字段
        // 需要从 adapter.getConfigSchema() 获取字段列表

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

        // 解析 env 字段中的密钥引用
        if let Some(env_value) = config_obj.get("env") {
            if let Some(env_obj) = env_value.as_object() {
                let mut resolved_env = serde_json::Map::new();

                for (key, value) in env_obj {
                    let binding = EnvBinding::from_value(value)?;

                    match binding {
                        EnvBinding::Plain { value } => {
                            resolved_env.insert(key.clone(), JsonValue::String(value));
                        }
                        EnvBinding::SecretRef { secret_id, version } => {
                            // TODO: 从数据库解析密钥值
                            // 占位实现：返回占位符
                            let pool = self.pool()?;
                            let material: Option<JsonValue> = if version == "latest" {
                                sqlx::query_scalar("SELECT v.material FROM company_secret_versions v JOIN company_secrets s ON s.id=v.secret_id WHERE s.id=$1 AND s.company_id=$2 AND s.deleted_at IS NULL ORDER BY v.version DESC LIMIT 1").bind(secret_id).bind(company_id).fetch_optional(pool).await?
                            } else {
                                let parsed: i32 = version.parse().map_err(|_| SecretServiceError::ResolutionFailed(format!("invalid secret version '{}'", version)))?;
                                sqlx::query_scalar("SELECT v.material FROM company_secret_versions v JOIN company_secrets s ON s.id=v.secret_id WHERE s.id=$1 AND s.company_id=$2 AND s.deleted_at IS NULL AND v.version=$3").bind(secret_id).bind(company_id).bind(parsed).fetch_optional(pool).await?
                            };
                            let value = material.and_then(|m| m.get("value").and_then(|v| v.as_str()).map(str::to_owned)).ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;
                            resolved_env.insert(key.clone(), JsonValue::String(value));

                            secret_keys.push(key.clone());
                            manifest.push(RuntimeSecretManifestEntry {
                                config_path: format!("env.{}", key),
                                env_key: Some(key.clone()),
                                secret_id,
                                secret_key: format!("secret-{}", secret_id),
                                version,
                                outcome: SecretResolutionOutcome::Success,
                                error_code: None,
                            });
                        }
                        EnvBinding::UserSecretRef { key: user_key, .. } => {
                            // TODO: 从用户环境解析密钥
                            // 占位实现：返回占位符
                            return Err(SecretServiceError::ResolutionFailed(format!("user secret '{}' requires an authenticated user context", user_key)));
                        }
                    }
                }

                resolved.insert("env".to_string(), JsonValue::Object(resolved_env));
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
                    if Self::is_sensitive_env_key(key) {
                        // 敏感字段替换为 REDACTED
                        redacted_env.insert(
                            key.clone(),
                            JsonValue::String("***REDACTED***".to_string()),
                        );
                    } else {
                        redacted_env.insert(key.clone(), value.clone());
                    }
                }

                config_obj.insert("env".to_string(), JsonValue::Object(redacted_env));
            }
        }

        // 脱敏已知敏感字段
        for sensitive_key in &["api_key", "apiKey", "access_token", "accessToken", "secret"] {
            if config_obj.contains_key(*sensitive_key) {
                config_obj.insert(
                    sensitive_key.to_string(),
                    JsonValue::String("***REDACTED***".to_string()),
                );
            }
        }

        JsonValue::Object(config_obj)
    }

    async fn create_secret(
        &self,
        company_id: Uuid,
        input: CreateSecretInput,
    ) -> Result<Secret, SecretServiceError> {
        // Create secret with initial version
        if !Self::is_valid_env_key(&input.key) { return Err(SecretServiceError::InvalidEnvKey(input.key)); }
        if input.value.is_empty() { return Err(SecretServiceError::ResolutionFailed("secret value cannot be empty".into())); }
        let pool = self.pool()?;
        let row = sqlx::query("INSERT INTO company_secrets (company_id,key,name,description) VALUES ($1,$2,$2,$3) RETURNING id,company_id,key,name,provider,status,scope,description,latest_version,created_at,updated_at")
            .bind(company_id).bind(&input.key).bind(&input.description).fetch_one(pool).await?;
        let id: Uuid = row.get("id"); let mut h=Sha256::new(); h.update(input.value.as_bytes()); let digest=format!("{:x}",h.finalize());
        sqlx::query("INSERT INTO company_secret_versions (secret_id,version,material,value_sha256,fingerprint_sha256) VALUES ($1,1,$2,$3,$3)")
            .bind(id).bind(serde_json::json!({"value": input.value})).bind(digest).execute(pool).await?;
        Ok(Self::row_to_secret(&row, None))
    }

    async fn get_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<Secret, SecretServiceError> {
        // Query secret by ID
        let row = sqlx::query("SELECT id,company_id,key,name,provider,status,scope,description,latest_version,created_at,updated_at FROM company_secrets WHERE id=$1 AND company_id=$2 AND deleted_at IS NULL")
            .bind(secret_id).bind(company_id).fetch_optional(self.pool()?).await?.ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;
        Ok(Self::row_to_secret(&row, None))
    }

    async fn update_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
        input: UpdateSecretInput,
    ) -> Result<Secret, SecretServiceError> {
        // Update secret metadata and optionally create new version
        let pool = self.pool()?;
        let row = sqlx::query("UPDATE company_secrets SET description=COALESCE($3,description), latest_version=CASE WHEN $4::text IS NULL THEN latest_version ELSE latest_version+1 END, updated_at=now() WHERE id=$1 AND company_id=$2 AND deleted_at IS NULL RETURNING id,company_id,key,name,provider,status,scope,description,latest_version,created_at,updated_at")
            .bind(secret_id).bind(company_id).bind(input.description).bind(input.value.as_deref()).fetch_optional(pool).await?.ok_or_else(|| SecretServiceError::SecretNotFound(secret_id.to_string()))?;
        if let Some(value) = input.value { let version:i32=row.get("latest_version"); let mut h=Sha256::new(); h.update(value.as_bytes()); let digest=format!("{:x}",h.finalize()); sqlx::query("INSERT INTO company_secret_versions (secret_id,version,material,value_sha256,fingerprint_sha256) VALUES ($1,$2,$3,$4,$4)").bind(secret_id).bind(version).bind(serde_json::json!({"value":value})).bind(digest).execute(pool).await?; }
        Ok(Self::row_to_secret(&row, None))
    }

    async fn delete_secret(
        &self,
        company_id: Uuid,
        secret_id: Uuid,
    ) -> Result<(), SecretServiceError> {
        // Soft delete secret
        let result=sqlx::query("UPDATE company_secrets SET deleted_at=now(),updated_at=now() WHERE id=$1 AND company_id=$2 AND deleted_at IS NULL").bind(secret_id).bind(company_id).execute(self.pool()?).await?;
        if result.rows_affected()==0 { return Err(SecretServiceError::SecretNotFound(secret_id.to_string())); } Ok(())
    }

    async fn list_secrets(
        &self,
        company_id: Uuid,
    ) -> Result<Vec<Secret>, SecretServiceError> {
        // List all active secrets for company
        let rows=sqlx::query("SELECT id,company_id,key,name,provider,status,scope,description,latest_version,created_at,updated_at FROM company_secrets WHERE company_id=$1 AND deleted_at IS NULL ORDER BY created_at DESC").bind(company_id).fetch_all(self.pool()?).await?;
        Ok(rows.iter().map(|r| Self::row_to_secret(r,None)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_binding_plain() {
        let value = serde_json::json!("plain_value");
        let binding = EnvBinding::from_value(&value).unwrap();
        assert!(matches!(binding, EnvBinding::Plain { .. }));
    }

    #[test]
    fn test_env_binding_secret_ref() {
        let value = serde_json::json!({
            "type": "secret_ref",       "secret_id": "00000000-0000-0000-0000-000000000001",
        });
        let binding = EnvBinding::from_value(&value).unwrap();
        assert!(matches!(binding, EnvBinding::SecretRef { .. }));
    }

    #[test]
    fn test_valid_env_key() {
        assert!(DefaultSecretService::is_valid_env_key("API_KEY"));
        assert!(DefaultSecretService::is_valid_env_key("_PRIVATE"));
        assert!(DefaultSecretService::is_valid_env_key("VAR123"));
        assert!(!DefaultSecretService::is_valid_env_key("123VAR"));
        assert!(!DefaultSecretService::is_valid_env_key("VAR-NAME"));
    }

    #[test]
    fn test_sensitive_env_key() {
        assert!(DefaultSecretService::is_sensitive_env_key("API_KEY"));
        assert!(DefaultSecretService::is_sensitive_env_key("ACCESS_TOKEN"));
        assert!(DefaultSecretService::is_sensitive_env_key("DATABASE_PASSWORD"));
        assert!(!DefaultSecretService::is_sensitive_env_key("DATABASE_HOST"));
    }

    #[tokio::test]
    async fn test_redact_config() {
        let service = DefaultSecretService::new();

        let config = serde_json::json!({
            "env": {
                "API_KEY": "secret123",
                "DATABASE_HOST": "localhost",
            },
            "api_key": "another_secret",
        });

        let redacted = service.redact_config(config);

        assert_eq!(
            redacted["env"]["API_KEY"],
            JsonValue::String("***REDACTED***".to_string())
        );
        assert_eq!(redacted["env"]["DATABASE_HOST"], "localhost");
        assert_eq!(
            redacted["api_key"],
            JsonValue::String("***REDACTED***".to_string())
        );
    }
}
