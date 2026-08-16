use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

const VERSION: &str = "decision-spec-v1";
const MIN_SECRET_LENGTH: usize = 32;

/// 决策签名服务错误
#[derive(Debug, Error)]
pub enum SigningError {
    #[error("Invalid signature format")]
    InvalidFormat,

    #[error("Signature verification failed")]
    VerificationFailed,

    #[error("Secret key error: {0}")]
    SecretKeyError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// 决策签名服务
pub struct DecisionSigningService {
    secret_key: String,
}

impl DecisionSigningService {
    /// 创建新的签名服务实例
    pub fn new() -> Result<Self, SigningError> {
        let secret_key = Self::resolve_signing_secret()?;
        Ok(Self { secret_key })
    }

    /// 从环境变量或生成的密钥文件中解析签名密钥
    fn resolve_signing_secret() -> Result<String, SigningError> {
        // 1. 首先尝试从环境变量读取
        if let Ok(secret) = std::env::var("PAPERCLIP_DECISION_SIGNING_SECRET") {
            if secret.len() >= MIN_SECRET_LENGTH {
                return Ok(secret);
            }
            return Err(SigningError::SecretKeyError(
                format!("Environment secret too short (minimum {} bytes)", MIN_SECRET_LENGTH)
            ));
        }

        // 2. 如果没有环境变量，从文件加载或生成
        Self::load_or_create_generated_secret()
    }

    /// 加载或创建生成的密钥文件
    fn load_or_create_generated_secret() -> Result<String, SigningError> {
        let key_path = Self::resolve_generated_secret_file_path()?;

        // 如果文件存在，读取它
        if key_path.exists() {
            return Self::read_generated_secret(&key_path);
        }

        // 文件不存在，生成新密钥
        Self::generate_and_save_secret(&key_path)
    }

    /// 解析生成的密钥文件路径
    fn resolve_generated_secret_file_path() -> Result<PathBuf, SigningError> {
        // 使用 ~/.paperclip/secrets/decision-signing.key
        let home = std::env::var("HOME")
            .map_err(|_| SigningError::SecretKeyError("HOME environment variable not set".to_string()))?;
        
        let secrets_dir = PathBuf::from(home).join(".paperclip").join("secrets");
        Ok(secrets_dir.join("decision-signing.key"))
    }

    /// 从文件读取密钥
    fn read_generated_secret(key_path: &PathBuf) -> Result<String, SigningError> {
        let secret = fs::read_to_string(key_path)?;
        let secret = secret.trim();

        if secret.len() < MIN_SECRET_LENGTH {
            return Err(SigningError::SecretKeyError(
                format!("Key file contains secret shorter than {} bytes", MIN_SECRET_LENGTH)
            ));
        }

        Ok(secret.to_string())
    }

    /// 生成并保存新密钥
    fn generate_and_save_secret(key_path: &PathBuf) -> Result<String, SigningError> {
        use rand::Rng;

        // 确保目录存在
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent)?;
            
 // 设置目录权限为 700 (仅所有者可读写执行)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(parent)?.permissions();
                perms.set_mode(0o700);
                fs::set_permissions(parent, perms)?;
            }
        }

        // 生成64字节的随机密钥
        let mut rng = rand::thread_rng();
        let random_bytes: Vec<u8> = (0..64).map(|_| rng.gen()).collect();
        let secret = hex::encode(random_bytes);

        // 写入文件
        fs::write(key_path, &secret)?;

        // 设置文件权限为 600 (仅所有者可读写)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(key_path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(key_path, perms)?;
        }

        Ok(secret)
    }

    /// 对决策spec进行签名
    pub fn sign_decision_spec(&self, value: &serde_json::Value) -> Result<String, SigningError> {
        let canonical = Self::canonical_json(value)?;
        let message = format!("{}:{}", VERSION, canonical);

        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|e| SigningError::SecretKeyError(e.to_string()))?;
        
        mac.update(message.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());

        Ok(format!("{}.{}", VERSION, signature))
    }

    /// 验证决策spec的签名
    pub fn verify_decision_spec(&self, value: &serde_json::Value, signature: &str) -> Result<bool, SigningError> {
        // 解析签名格式: "decision-spec-v1.<hex>"
        let parts: Vec<&str> = signature.split('.').collect();
        if parts.len() != 2 || parts[0] != VERSION {
            return Err(SigningError::InvalidFormat);
        }
        let expected_sig = parts[1];
        let canonical = Self::canonical_json(value)?;
        let message = format!("{}:{}", VERSION, canonical);

        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|e| SigningError::SecretKeyError(e.to_string()))?;
        
        mac.update(message.as_bytes());
        let result = mac.finalize();
        let computed_sig = hex::encode(result.into_bytes());

        // 使用constant-time比较防止时序攻击
        Ok(Self::timing_safe_equal(expected_sig.as_bytes(), computed_sig.as_bytes()))
    }

    /// 规范化JSON（确保一致的序列化）
    fn canonical_json(value: &serde_json::Value) -> Result<String, SigningError> {
        serde_json::to_string(value)
            .map_err(|e| SigningError::SerializationError(e.to_string()))
    }

    /// 时序安全的字节数组比较
    fn timing_safe_equal(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }

        let mut result = 0u8;
        for (byte_a, byte_b) in a.iter().zip(b.iter()) {
            result |= byte_a ^ byte_b;
        }

        result == 0
    }
}

impl Default for DecisionSigningService {
    fn default() -> Self {
        Self::new().expect("Failed to initialize decision signing service")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sign_and_verify() {
        let service = DecisionSigningService::new().unwrap();
        let spec = json!({
            "decision_id": "test-123",
            "action": "approve",
            "timestamp": "2026-08-16T00:00:00Z"
        });

        let signature = service.sign_decision_spec(&spec).unwrap();
        assert!(signature.starts_with("decision-spec-v1."));

        let is_valid = service.verify_decision_spec(&spec, &signature).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_verify_invalid_signature() {
        let service = DecisionSigningService::new().unwrap();
        let spec = json!({"test": "data"});

        let result = service.verify_decision_spec(&spec, "invalid-signature");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_tampered_data() {
        let service = DecisionSigningService::new().unwrap();
        let original_spec = json!({"amount": 100});
        let signature = service.sign_decision_spec(&original_spec).unwrap();

        let tampered_spec = json!({"amount": 999});
        let is_valid = service.verify_decision_spec(&tampered_spec, &signature).unwrap();
        assert!(!is_valid);
    }

    #[test]
    fn test_timing_safe_equal() {
        assert!(DecisionSigningService::timing_safe_equal(b"hello", b"hello"));
        assert!(!DecisionSigningService::timing_safe_equal(b"hello", b"world"));
        assert!(!DecisionSigningService::timing_safe_equal(b"hello", b"hello!"));
    }
}
