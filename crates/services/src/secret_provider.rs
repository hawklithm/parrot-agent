use async_trait::async_trait;
use models::SecretProviderType;
use serde_json::Value as JsonValue;
use sha2::{Sha256, Digest};
use hex;
use std::env;

/// Load the AES-256 encryption key used by the local encrypted secret provider.
///
/// Expects `PARROT_SECRET_ENCRYPTION_KEY` to be a 64-char hex string (32 bytes).
/// Falls back to an all-zero key when unset (dev/test only — never use in prod).
pub fn load_secret_encryption_key() -> Vec<u8> {
    match env::var("PARROT_SECRET_ENCRYPTION_KEY") {
        Ok(s) if s.len() == 64 => {
            match hex::decode(&s) {
                Ok(bytes) if bytes.len() == 32 => return bytes,
                _ => tracing::warn!("PARROT_SECRET_ENCRYPTION_KEY is not valid 32-byte hex; using zero key"),
            }
        }
        Ok(_) => tracing::warn!("PARROT_SECRET_ENCRYPTION_KEY must be 64 hex chars; using zero key"),
        Err(_) => tracing::warn!("PARROT_SECRET_ENCRYPTION_KEY not set; using zero key (dev only)"),
    }
    vec![0u8; 32]
}

/// SHA-256 hex of a secret plaintext value (mirrors paperclip value_sha256).
pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Provider error: {0}")]
    Provider(String),
}

/// Secret provider trait for external secret management
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// Store a secret value
    async fn store(&self, key: &str, value: &str, metadata: Option<JsonValue>) -> Result<String, ProviderError>;

    /// Retrieve a secret value by reference
    async fn retrieve(&self, value_ref: &str) -> Result<String, ProviderError>;

    /// Delete a secret
    async fn delete(&self, value_ref: &str) -> Result<(), ProviderError>;

    /// Rotate a secret (generate new value)
    async fn rotate(&self, value_ref: &str) -> Result<String, ProviderError>;

    /// Provider type identifier
    fn provider_type(&self) -> SecretProviderType;
}

/// Local encrypted provider using AES-256-GCM
pub struct LocalEncryptedProvider {
    encryption_key: Vec<u8>,
}

impl LocalEncryptedProvider {
    pub fn new(encryption_key: Vec<u8>) -> Result<Self, ProviderError> {
        if encryption_key.len() != 32 {
            return Err(ProviderError::InvalidConfig(
                "Encryption key must be 32 bytes for AES-256".to_string(),
            ));
        }
        Ok(Self { encryption_key })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, ProviderError> {
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        use aes_gcm::aead::Aead;

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| ProviderError::Encryption(format!("Failed to create cipher: {}", e)))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| ProviderError::Encryption(format!("Encryption failed: {}", e)))?;

        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(hex::encode(result))
    }

    pub fn decrypt(&self, encrypted_hex: &str) -> Result<String, ProviderError> {
        use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
        use aes_gcm::aead::Aead;

        let encrypted = hex::decode(encrypted_hex)
            .map_err(|e| ProviderError::Decryption(format!("Invalid hex: {}", e)))?;

        if encrypted.len() < 12 {
            return Err(ProviderError::Decryption("Invalid encrypted data".to_string()));
        }

        let (nonce_bytes, ciphertext) = encrypted.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| ProviderError::Decryption(format!("Failed to create cipher: {}", e)))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| ProviderError::Decryption(format!("Decryption failed: {}", e)))?;

        String::from_utf8(plaintext)
            .map_err(|e| ProviderError::Decryption(format!("Invalid UTF-8: {}", e)))
    }
}

#[async_trait]
impl SecretProvider for LocalEncryptedProvider {
    async fn store(&self, key: &str, value: &str, _metadata: Option<JsonValue>) -> Result<String, ProviderError> {
        let encrypted = self.encrypt(value)?;
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(encrypted.as_bytes());
        let hash = hasher.finalize();
        let value_ref = format!("local:{}:{}", hex::encode(hash), encrypted);
        Ok(value_ref)
    }

    async fn retrieve(&self, value_ref: &str) -> Result<String, ProviderError> {
        if !value_ref.starts_with("local:") {
            return Err(ProviderError::Provider("Invalid value_ref prefix".to_string()));
        }

        let parts: Vec<&str> = value_ref.splitn(3, ':').collect();
        if parts.len() != 3 {
            return Err(ProviderError::Provider("Invalid value_ref format".to_string()));
        }

        self.decrypt(parts[2])
    }

    async fn delete(&self, _value_ref: &str) -> Result<(), ProviderError> {
        Ok(())
    }

    async fn rotate(&self, value_ref: &str) -> Result<String, ProviderError> {
        let old_value = self.retrieve(value_ref).await?;
        let rotated_value = format!("{}-rotated-{}", old_value, uuid::Uuid::new_v4());
        self.store("rotated", &rotated_value, None).await
    }

    fn provider_type(&self) -> SecretProviderType {
        SecretProviderType::LocalEncrypted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_encrypted_provider_store_retrieve() {
        let key = vec![0u8; 32];
        let provider = LocalEncryptedProvider::new(key).unwrap();

        let value_ref = provider.store("test_key", "secret_value", None).await.unwrap();
        assert!(value_ref.starts_with("local:"));

        let retrieved = provider.retrieve(&value_ref).await.unwrap();
        assert_eq!(retrieved, "secret_value");
    }

    #[tokio::test]
    async fn test_local_encrypted_provider_different_values() {
        let key = vec![1u8; 32];
        let provider = LocalEncryptedProvider::new(key).unwrap();

        let ref1 = provider.store("key1", "value1", None).await.unwrap();
        let ref2 = provider.store("key2", "value2", None).await.unwrap();

        assert_ne!(ref1, ref2);

        let val1 = provider.retrieve(&ref1).await.unwrap();
        let val2 = provider.retrieve(&ref2).await.unwrap();

        assert_eq!(val1, "value1");
        assert_eq!(val2, "value2");
    }

    #[tokio::test]
    async fn test_local_encrypted_provider_rotate() {
        let key = vec![2u8; 32];
        let provider = LocalEncryptedProvider::new(key).unwrap();

        let original_ref = provider.store("test", "original", None).await.unwrap();
        let rotated_ref = provider.rotate(&original_ref).await.unwrap();

        assert_ne!(original_ref, rotated_ref);

        let rotated_value = provider.retrieve(&rotated_ref).await.unwrap();
        assert!(rotated_value.contains("original"));
        assert!(rotated_value.contains("-rotated-"));
    }
}
