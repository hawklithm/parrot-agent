use base64::Engine;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

/// URL-safe base64 引擎（无填充），对应 base64 0.22 API。
const BASE64_ENGINE: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// JWT配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub ttl_seconds: i64,
    pub issuer: String,
    pub audience: String,
    pub instance_id: String,
}

impl JwtConfig {
    /// 创建新的JWT配置
    pub fn new(
        secret: String,
        ttl_seconds: i64,
        issuer: String,
        audience: String,
        instance_id: String,
    ) -> Self {
        Self {
            secret,
            ttl_seconds,
            issuer,
            audience,
            instance_id,
        }
    }

    /// 从环境变量加载配置
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("JWT_SECRET").ok()?;
        let ttl_seconds = std::env::var("JWT_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);
        let issuer = std::env::var("JWT_ISSUER").unwrap_or_else(|_| "parrot-agent".to_string());
        let audience = std::env::var("JWT_AUDIENCE").unwrap_or_else(|_| "agent-runtime".to_string());
        let instance_id = std::env::var("INSTANCE_ID").ok()?;

        Some(Self::new(secret, ttl_seconds, issuer, audience, instance_id))
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<(), String> {
        if self.secret.is_empty() {
            return Err("JWT secret cannot be empty".to_string());
        }
        if self.ttl_seconds <= 0 || self.ttl_seconds > 86400 {
            return Err("JWT TTL must be between 1 second and 24 hours".to_string());
        }
        if self.instance_id.is_empty() {
            return Err("Instance ID cannot be empty".to_string());
        }
        Ok(())
    }
}

/// Local Agent JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentJwtClaims {
    /// Unique token id, allowing downstream replay protection/auditing.
    #[serde(default)]
    pub jti: String,
    /// Subject (Agent ID)
    pub sub: String,
    /// 公司ID
    pub company_id: String,
    /// Adapter类型
    pub adapter_type: String,
    /// 运行时ID
    pub run_id: Option<String>,
    /// 负责用户ID
    pub responsible_user_id: Option<String>,
    /// 密钥范围
    pub key_scope: Option<serde_json::Value>,
    /// 签发时间
    pub iat: i64,
    /// 过期时间
    pub exp: i64,
    /// 签发者
    pub iss: String,
    /// 受众
    pub aud: String,
    /// 实例ID
    pub instance_id: String,
}

impl LocalAgentJwtClaims {
    /// 创建新的Claims
    pub fn new(
        agent_id: Uuid,
        company_id: Uuid,
        adapter_type: String,
        run_id: Option<Uuid>,
        responsible_user_id: Option<Uuid>,
        key_scope: Option<serde_json::Value>,
        config: &JwtConfig,
    ) -> Self {
        let now = Utc::now().timestamp();
        Self {
            jti: Uuid::new_v4().to_string(),
            sub: agent_id.to_string(),
            company_id: company_id.to_string(),
            adapter_type,
            run_id: run_id.map(|id| id.to_string()),
            responsible_user_id: responsible_user_id.map(|id| id.to_string()),
            key_scope,
            iat: now,
            exp: now + config.ttl_seconds,
            iss: config.issuer.clone(),
            aud: config.audience.clone(),
            instance_id: config.instance_id.clone(),
        }
    }

    /// 检查Claims是否已过期
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    /// 验证Claims的issuer和audience
    pub fn verify_metadata(&self, config: &JwtConfig) -> bool {
        self.iss == config.issuer
            && self.aud == config.audience
            && self.instance_id == config.instance_id
    }
}

/// 派生公司级签名密钥（HMAC-SHA256）
///
/// 使用主密钥 + 公司ID + 实例ID派生，确保跨公司密钥隔离
pub fn derive_company_signing_key(
    secret: &str,
    company_id: Uuid,
    instance_id: &str,
) -> Result<Vec<u8>, String> {
    let input = format!("{}:{}:{}", secret, company_id, instance_id);

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("Failed to create HMAC: {}", e))?;

    mac.update(input.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 创建Local Agent JWT
///
/// 配置不存在或验证失败时返回None
pub fn create_local_agent_jwt(
    config: &JwtConfig,
    agent_id: Uuid,
    company_id: Uuid,
    adapter_type: String,
    run_id: Option<Uuid>,
    responsible_user_id: Option<Uuid>,
    key_scope: Option<serde_json::Value>,
) -> Option<String> {
    // 验证配置
    config.validate().ok()?;

    // 创建Claims
    let claims = LocalAgentJwtClaims::new(
        agent_id,
        company_id,
        adapter_type,
        run_id,
        responsible_user_id,
        key_scope,
        config,
    );

    // 派生公司签名密钥
    let signing_key = derive_company_signing_key(&config.secret, company_id, &config.instance_id).ok()?;

    // 序列化Claims
    let claims_json = serde_json::to_string(&claims).ok()?;
    let claims_b64 = BASE64_ENGINE.encode(claims_json.as_bytes());

    // 创建JWT Header
    let header = serde_json::json!({
        "alg": "HS256",
        "typ": "JWT"
    });
    let header_json = serde_json::to_string(&header).ok()?;
    let header_b64 = BASE64_ENGINE.encode(header_json.as_bytes());

    // 签名
    let message = format!("{}.{}", header_b64, claims_b64);
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(&signing_key).ok()?;
    mac.update(message.as_bytes());
    let signature = mac.finalize().into_bytes();
    let signature_b64 = BASE64_ENGINE.encode(&signature[..]);

    Some(format!("{}.{}", message, signature_b64))
}

/// 验证Local Agent JWT
///
/// 处理四种失败场景：配置缺失 / 格式无效 / 签名无效 / 已过期，均返回None
pub fn verify_local_agent_jwt(
    config: &JwtConfig,
    token: &str,
) -> Option<LocalAgentJwtClaims> {
    // 验证配置
    config.validate().ok()?;

    // 解析JWT格式
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let (header_b64, claims_b64, signature_b64) = (parts[0], parts[1], parts[2]);

    // 解析Claims
    let claims_json = BASE64_ENGINE.decode(claims_b64).ok()?;
    let claims: LocalAgentJwtClaims = serde_json::from_slice(&claims_json).ok()?;

    // 检查过期
    if claims.is_expired() {
        return None;
    }

    // 验证metadata
    if !claims.verify_metadata(config) {
        return None;
    }

    // 解析company_id用于密钥派生
    let company_id = Uuid::parse_str(&claims.company_id).ok()?;

    // 派生签名密钥
    let signing_key = derive_company_signing_key(&config.secret, company_id, &config.instance_id).ok()?;

    // 验证签名（constant-time比较）
    let message = format!("{}.{}", header_b64, claims_b64);
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(&signing_key).ok()?;
    mac.update(message.as_bytes());

    let provided_signature = BASE64_ENGINE.decode(signature_b64).ok()?;

    // 常量时间比较（hmac 0.12 内置 verify_slice）
    if mac.verify_slice(&provided_signature).is_ok() {
        Some(claims)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> JwtConfig {
        JwtConfig::new(
            "test_secret_key_12345".to_string(),
            3600,
            "test-issuer".to_string(),
            "test-audience".to_string(),
            "test-instance".to_string(),
        )
    }

    #[test]
    fn test_jwt_config_validation() {
        let valid_config = test_config();
        assert!(valid_config.validate().is_ok());

        let invalid_secret = JwtConfig::new(
            "".to_string(),
            3600,
            "issuer".to_string(),
            "audience".to_string(),
            "instance".to_string(),
        );
        assert!(invalid_secret.validate().is_err());

        let invalid_ttl = JwtConfig::new(
            "secret".to_string(),
            100000,
            "issuer".to_string(),
            "audience".to_string(),
            "instance".to_string(),
        );
        assert!(invalid_ttl.validate().is_err());
    }

    #[test]
    fn test_derive_company_signing_key() {
        let company_id1 = Uuid::new_v4();
        let company_id2 = Uuid::new_v4();

        let key1 = derive_company_signing_key("secret", company_id1, "instance1").unwrap();
        let key2 = derive_company_signing_key("secret", company_id2, "instance1").unwrap();
        let key3 = derive_company_signing_key("secret", company_id1, "instance2").unwrap();

        // 不同公司/实例应产生不同密钥
        assert_ne!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_create_and_verify_jwt() {
        let config = test_config();
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = create_local_agent_jwt(
            &config,
            agent_id,
            company_id,
            "docker".to_string(),
            None,
            None,
            None,
        )
        .unwrap();

        let claims = verify_local_agent_jwt(&config, &token).unwrap();
        assert_eq!(claims.sub, agent_id.to_string());
        assert_eq!(claims.company_id, company_id.to_string());
        assert_eq!(claims.adapter_type, "docker");
    }

    #[test]
    fn test_verify_invalid_jwt() {
        let config = test_config();

        // 格式无效
        assert!(verify_local_agent_jwt(&config, "invalid.token").is_none());

        // 签名无效
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let token = create_local_agent_jwt(
            &config,
            agent_id,
            company_id,
            "docker".to_string(),
            None,
            None,
            None,
        )
        .unwrap();

        let tampered = token.replace(".", "X");
        assert!(verify_local_agent_jwt(&config, &tampered).is_none());
    }

    #[test]
    fn test_jwt_expiration() {
        let mut config = test_config();
        config.ttl_seconds = 1;

        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = create_local_agent_jwt(
            &config,
            agent_id,
            company_id,
            "docker".to_string(),
            None,
            None,
            None,
        )
        .unwrap();

        // 立即验证应该成功
        assert!(verify_local_agent_jwt(&config, &token).is_some());

        // 等待过期
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(verify_local_agent_jwt(&config, &token).is_none());
    }
}
