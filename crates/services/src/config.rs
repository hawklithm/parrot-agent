//! 配置系统
//!
//! 提供 DeploymentMode, Config 结构体、配置加载优先级等
//! 对应 pipeline-adapter-tasks.md §8 配置系统

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// 核心枚举
// ============================================================================

/// 部署模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    LocalTrusted,
    Authenticated,
}

/// 部署暴露范围
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentExposure {
    Private,
    Public,
}

/// 绑定模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindMode {
    Loopback,
    Lan,
    Tailnet,
    Custom,
}

/// 数据库模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseMode {
    EmbeddedPostgres,
    Postgres,
}

/// 存储提供商
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageProvider {
    LocalDisk,
    S3,
}

/// 密钥提供商
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretProvider {
    LocalEncrypted,
    AwsSecretsManager,
    GcpSecretManager,
    Vault,
}

// ============================================================================
// 配置结构体
// ============================================================================

/// 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub deployment_mode: DeploymentMode,
    pub exposure: DeploymentExposure,
    pub bind_mode: BindMode,
    pub host: String,
    pub port: u16,
    pub external_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            deployment_mode: DeploymentMode::LocalTrusted,
            exposure: DeploymentExposure::Private,
            bind_mode: BindMode::Loopback,
            host: "127.0.0.1".to_string(),
            port: 3100,
            external_url: None,
        }
    }
}

/// 认证配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: Option<String>,
    pub session_ttl_seconds: u64,
    pub max_login_attempts: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: None,
            session_ttl_seconds: 86400,
            max_login_attempts: 5,
        }
    }
}

/// 数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub mode: DatabaseMode,
    pub url: String,
    pub max_connections: u32,
    pub timeout_seconds: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            mode: DatabaseMode::Postgres,
            url: "postgres://localhost:5432/paperclip".to_string(),
            max_connections: 20,
            timeout_seconds: 30,
        }
    }
}

/// 存储配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub provider: StorageProvider,
    pub local_path: Option<PathBuf>,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            provider: StorageProvider::LocalDisk,
            local_path: Some(PathBuf::from("./data")),
            s3_bucket: None,
            s3_region: None,
        }
    }
}

/// 密钥管理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    pub provider: SecretProvider,
    pub master_key_path: Option<PathBuf>,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            provider: SecretProvider::LocalEncrypted,
            master_key_path: Some(PathBuf::from("./data/master.key")),
        }
    }
}

/// 心跳配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub max_missed_heartbeats: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            timeout_seconds: 10,
            max_missed_heartbeats: 3,
        }
    }
}

/// 主配置结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub secrets: SecretsConfig,
    pub heartbeat: HeartbeatConfig,
    pub extra: HashMap<String, String>,
}

impl Config {
    /// 使用默认值创建配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 从文件加载配置
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::IoError(e.to_string()))?;

        // Try JSON first (most compatible)
        serde_json::from_str(&content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// 加载配置（优先级：环境变量 > 配置文件 > 默认值）
    pub fn load(config_path: Option<&str>) -> Result<Self, ConfigError> {
        let mut config = Config::default();

        // 1. 从文件加载
        if let Some(path) = config_path {
            if std::path::Path::new(path).exists() {
                let file_config = Config::from_file(path)?;
                config = file_config;
            }
        }

        // 2. 环境变量覆盖
        config.apply_env_overrides();

        // 3. 校验
        config.validate()?;

        Ok(config)
    }

    /// 应用环境变量覆盖
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("PAPERCLIP_SERVER_HOST") {
            self.server.host = val;
        }
        if let Ok(val) = std::env::var("PAPERCLIP_SERVER_PORT") {
            if let Ok(port) = val.parse::<u16>() {
                self.server.port = port;
            }
        }
        if let Ok(val) = std::env::var("PAPERCLIP_DATABASE_URL") {
            self.database.url = val;
        }
        if let Ok(val) = std::env::var("PAPERCLIP_JWT_SECRET") {
            self.auth.jwt_secret = Some(val);
        }
        // 支持嵌套 key
        for (key, value) in std::env::vars() {
            if key.starts_with("PAPERCLIP_EXTRA_") {
                let extra_key = key.trim_start_matches("PAPERCLIP_EXTRA_").to_lowercase();
                self.extra.insert(extra_key, value);
            }
        }
    }

    /// 校验配置
    fn validate(&self) -> Result<(), ConfigError> {
        // port is u16, always within 1-65535 range
        if self.database.url.is_empty() {
            return Err(ConfigError::MissingRequired("database.url".to_string()));
        }
        if self.database.max_connections == 0 {
            return Err(ConfigError::InvalidValue("database.max_connections must be > 0".to_string()));
        }
        Ok(())
    }

    /// 重新加载配置
    pub fn reload(&mut self, config_path: Option<&str>) -> Result<(), ConfigError> {
        let new_config = Config::load(config_path)?;
        *self = new_config;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            auth: AuthConfig::default(),
            database: DatabaseConfig::default(),
            storage: StorageConfig::default(),
            secrets: SecretsConfig::default(),
            heartbeat: HeartbeatConfig::default(),
            extra: HashMap::new(),
        }
    }
}

// ============================================================================
// 错误类型
// ============================================================================

#[derive(Debug, Clone)]
pub enum ConfigError {
    MissingRequired(String),
    InvalidValue(String),
    ParseError(String),
    IoError(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingRequired(msg) => write!(f, "Missing required config: {}", msg),
            ConfigError::InvalidValue(msg) => write!(f, "Invalid config value: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Config parse error: {}", msg),
            ConfigError::IoError(msg) => write!(f, "Config IO error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}
