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

/// Load the previous AES-256 key during a rotation window
/// (`PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS`). Returns None when unset — normal
/// steady-state operation. During rotation, decrypts fall back to this key so
/// envelopes written under the old key remain readable until re-encrypted.
pub fn load_previous_secret_encryption_key() -> Option<Vec<u8>> {
    match env::var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS") {
        Ok(s) if s.len() == 64 => match hex::decode(&s) {
            Ok(bytes) if bytes.len() == 32 => Some(bytes),
            _ => {
                tracing::warn!("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS is not valid 32-byte hex; ignored");
                None
            }
        },
        Ok(_) => {
            tracing::warn!("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS must be 64 hex chars; ignored");
            None
        }
        Err(_) => None,
    }
}

/// SHA-256 hex of a secret plaintext value (mirrors paperclip value_sha256).
pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

/// Encrypt a managed secret for persistence and return its plaintext digest.
/// The digest is used only for change detection/fingerprinting; the value itself
/// never belongs in a database JSON envelope. The envelope carries a `version`
/// field (currently 1) so future key-derivation or format changes can be
/// detected and migrated per-envelope.
pub fn encrypt_secret_material(
    plaintext: &str,
) -> Result<(JsonValue, String), ProviderError> {
    let provider = LocalEncryptedProvider::new(load_secret_encryption_key())?;
    let ciphertext = provider.encrypt(plaintext)?;
    Ok((
        serde_json::json!({ "version": 1, "ciphertext": ciphertext }),
        sha256_hex(plaintext),
    ))
}

/// Decrypt a persisted local-encrypted material envelope.
///
/// Failure recovery during key rotation: when the current key cannot decrypt
/// (the envelope was written under the previous key), falls back to
/// `PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS` so old envelopes remain readable
/// until re-encrypted by `rotate_secret_material`.
pub fn decrypt_secret_material(material: &JsonValue) -> Result<String, ProviderError> {
    let Some(ciphertext) = material.get("ciphertext").and_then(|value| value.as_str()) else {
        // Read-only compatibility for rows written before encrypted material
        // became mandatory. All current writers use the ciphertext branch.
        return material
            .get("value")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .ok_or_else(|| ProviderError::Decryption("missing ciphertext in material".into()));
    };
    // `version` is informational (currently always 1); unknown future versions
    // still attempt decryption — format migration is a per-envelope concern.
    let current = LocalEncryptedProvider::new(load_secret_encryption_key());
    match current {
        Ok(provider) => match provider.decrypt(ciphertext) {
            Ok(plaintext) => return Ok(plaintext),
            Err(first_error) => {
                // Rotation window: try the previous key before giving up.
                if let Some(previous) = load_previous_secret_encryption_key() {
                    if let Ok(previous_provider) = LocalEncryptedProvider::new(previous) {
                        if let Ok(plaintext) = previous_provider.decrypt(ciphertext) {
                            return Ok(plaintext);
                        }
                    }
                }
                return Err(first_error);
            }
        },
        Err(config_error) => Err(config_error),
    }
}

/// Re-encrypt a material envelope under the current key, bumping its version.
///
/// Key-rotation recovery: decrypts with the current key (or the previous key
/// during a rotation window) and re-encrypts with the current key. Returns the
/// new envelope; callers persist it in place of the old one.
pub fn rotate_secret_material(material: &JsonValue) -> Result<JsonValue, ProviderError> {
    let plaintext = decrypt_secret_material(material)?;
    let provider = LocalEncryptedProvider::new(load_secret_encryption_key())?;
    let ciphertext = provider.encrypt(&plaintext)?;
    let version = material
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    Ok(serde_json::json!({ "version": version + 1, "ciphertext": ciphertext }))
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

    #[test]
    fn test_persisted_material_roundtrip() {
        let (material, digest) = encrypt_secret_material("persisted-value").unwrap();
        assert_eq!(digest, sha256_hex("persisted-value"));
        assert!(material.get("ciphertext").and_then(|v| v.as_str()).is_some());
        assert_eq!(material.get("version").and_then(|v| v.as_u64()), Some(1), "envelope carries version");
        assert_eq!(decrypt_secret_material(&material).unwrap(), "persisted-value");
    }

    #[test]
    fn test_legacy_material_is_read_only_compatible() {
        let material = serde_json::json!({ "value": "legacy-value" });
        assert_eq!(decrypt_secret_material(&material).unwrap(), "legacy-value");
    }

    #[test]
    fn test_rotation_window_and_key_change_failure() {
        // Env mutation is process-global; keep the whole scenario in one test
        // so parallel test threads cannot interleave key changes.
        // Phase 1: write under old key, rotate to new key with PREVIOUS set.
        unsafe {
            std::env::set_var("PARROT_SECRET_ENCRYPTION_KEY", "0".repeat(64));
            std::env::remove_var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS");
        }
        let (material, _) = encrypt_secret_material("rotation-value").unwrap();

        unsafe {
            std::env::set_var("PARROT_SECRET_ENCRYPTION_KEY", "1".repeat(64));
            std::env::set_var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS", "0".repeat(64));
        }
        assert_eq!(
            decrypt_secret_material(&material).unwrap(),
            "rotation-value",
            "previous-key fallback must decrypt old envelopes during rotation"
        );

        // rotate_secret_material re-encrypts under the CURRENT key (bumps version).
        let rotated = rotate_secret_material(&material).unwrap();
        assert_eq!(rotated.get("version").and_then(|v| v.as_u64()), Some(2), "version bumps on rotate");
        // After rotation the previous key is no longer needed.
        unsafe {
            std::env::remove_var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS");
        }
        assert_eq!(
            decrypt_secret_material(&rotated).unwrap(),
            "rotation-value",
            "rotated envelope decrypts with the current key alone"
        );

        // Phase 2: change the key WITHOUT a previous-key window — decrypt must
        // fail loudly (never silently corrupt or return wrong plaintext).
        unsafe {
            std::env::set_var("PARROT_SECRET_ENCRYPTION_KEY", "2".repeat(64));
            std::env::remove_var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS");
        }
        assert!(
            decrypt_secret_material(&material).is_err(),
            "without the previous key, changed-key envelopes must fail loudly (not corrupt)"
        );

        unsafe {
            std::env::remove_var("PARROT_SECRET_ENCRYPTION_KEY");
            std::env::remove_var("PARROT_SECRET_ENCRYPTION_KEY_PREVIOUS");
        }
    }
}
