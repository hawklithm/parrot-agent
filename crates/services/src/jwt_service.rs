use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use jwt::{SignWithKey, VerifyWithKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::ServiceError;

/// LocalAgent JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentJwtClaims {
    pub agent_id: Uuid,
    pub company_id: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
}

impl LocalAgentJwtClaims {
    /// Create new claims with default expiration (24 hours)
    pub fn new(agent_id: Uuid, company_id: Uuid) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(24);

        Self {
            agent_id,
            company_id,
            exp: exp.timestamp(),
            iat: now.timestamp(),
            iss: "parrot-agent".to_string(),
        }
    }

    /// Create claims with custom expiration
    pub fn with_expiration(agent_id: Uuid, company_id: Uuid, expires_in: Duration) -> Self {
        let now = Utc::now();
        let exp = now + expires_in;

        Self {
            agent_id,
            company_id,
            exp: exp.timestamp(),
            iat: now.timestamp(),
            iss: "parrot-agent".to_string(),
        }
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }

    /// Get expiration as DateTime
    pub fn expires_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.exp, 0).unwrap_or_else(|| Utc::now())
    }
}

/// Board User JWT Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardUserJwtClaims {
    pub user_id: Uuid,
    pub company_id: Uuid,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
}

impl BoardUserJwtClaims {
    pub fn new(user_id: Uuid, company_id: Uuid) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(24);

        Self {
            user_id,
            company_id,
            exp: exp.timestamp(),
            iat: now.timestamp(),
            iss: "parrot-agent".to_string(),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() > self.exp
    }
}

/// JWT Service trait
#[async_trait]
pub trait JwtService: Send + Sync {
    /// Sign LocalAgent JWT
    async fn sign_local_agent_jwt(&self, agent_id: Uuid, company_id: Uuid) -> Result<String, ServiceError>;

    /// Verify and decode LocalAgent JWT
    async fn verify_local_agent_jwt(&self, token: &str) -> Result<LocalAgentJwtClaims, ServiceError>;

    /// Sign Board User JWT
    async fn sign_board_user_jwt(&self, user_id: Uuid, company_id: Uuid) -> Result<String, ServiceError>;

    /// Verify and decode Board User JWT
    async fn verify_board_user_jwt(&self, token: &str) -> Result<BoardUserJwtClaims, ServiceError>;

    /// Derive company-specific signing key
    fn derive_company_key(&self, company_id: Uuid) -> Vec<u8>;
}

/// Default JWT Service Implementation
pub struct DefaultJwtService {
    master_secret: Vec<u8>,
}

impl DefaultJwtService {
    /// Create new JWT service with master secret
    pub fn new(master_secret: Vec<u8>) -> Self {
        Self { master_secret }
    }

    /// Create from environment variable
    pub fn from_env() -> Result<Self, ServiceError> {
        let secret = std::env::var("JWT_SECRET")
            .map_err(|_| ServiceError::Configuration("JWT_SECRET not set".to_string()))?;

        Ok(Self {
            master_secret: secret.into_bytes(),
        })
    }
}

#[async_trait]
impl JwtService for DefaultJwtService {
    async fn sign_local_agent_jwt(&self, agent_id: Uuid, company_id: Uuid) -> Result<String, ServiceError> {
        let claims = LocalAgentJwtClaims::new(agent_id, company_id);
        let key_bytes = self.derive_company_key(company_id);
        let key: Hmac<Sha256> = Hmac::new_from_slice(&key_bytes)
            .map_err(|e| ServiceError::Internal(format!("HMAC key error: {}", e)))?;

        let mut claims_map = BTreeMap::new();
        claims_map.insert("agent_id", claims.agent_id.to_string());
        claims_map.insert("company_id", claims.company_id.to_string());
        claims_map.insert("exp", claims.exp.to_string());
        claims_map.insert("iat", claims.iat.to_string());
        claims_map.insert("iss", claims.iss.clone());

        let token = claims_map
            .sign_with_key(&key)
            .map_err(|e| ServiceError::Internal(format!("JWT signing error: {}", e)))?;

        Ok(token)
    }

    async fn verify_local_agent_jwt(&self, token: &str) -> Result<LocalAgentJwtClaims, ServiceError> {
        // Extract company_id from token without verification (JWT payload is base64, not encrypted)
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(ServiceError::Unauthorized("Invalid JWT format".to_string()));
        }

        // Decode payload to get company_id
        let payload = base64::decode_config(parts[1], base64::URL_SAFE_NO_PAD)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid JWT payload: {}", e)))?;

        let payload_json: serde_json::Value = serde_json::from_slice(&payload)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid JWT JSON: {}", e)))?;

        let company_id_str = payload_json["company_id"]
            .as_str()
            .ok_or_else(|| ServiceError::Unauthorized("Missing company_id in JWT".to_string()))?;

        let company_id = Uuid::parse_str(company_id_str)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid company_id: {}", e)))?;

        // Now verify with company-specific key
        let key_bytes = self.derive_company_key(company_id);
        let key: Hmac<Sha256> = Hmac::new_from_slice(&key_bytes)
            .map_err(|e| ServiceError::Internal(format!("HMAC key error: {}", e)))?;

        let claims: BTreeMap<String, String> = token
            .verify_with_key(&key)
            .map_err(|e| ServiceError::Unauthorized(format!("JWT verification failed: {}", e)))?;

        let agent_id = Uuid::parse_str(claims.get("agent_id").ok_or_else(|| ServiceError::Unauthorized("Missing agent_id".to_string()))?)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid agent_id: {}", e)))?;

        let exp = claims.get("exp")
            .ok_or_else(|| ServiceError::Unauthorized("Missing exp".to_string()))?
            .parse::<i64>()
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid exp: {}", e)))?;

        let iat = claims.get("iat")
            .ok_or_else(|| ServiceError::Unauthorized("Missing iat".to_string()))?
            .parse::<i64>()
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid iat: {}", e)))?;

        let iss = claims.get("iss")
            .ok_or_else(|| ServiceError::Unauthorized("Missing iss".to_string()))?
            .clone();

        let jwt_claims = LocalAgentJwtClaims {
            agent_id,
            company_id,
            exp,
            iat,
            iss,
        };

        // Check expiration
        if jwt_claims.is_expired() {
            return Err(ServiceError::Unauthorized("JWT expired".to_string()));
        }

        Ok(jwt_claims)
    }

    async fn sign_board_user_jwt(&self, user_id: Uuid, company_id: Uuid) -> Result<String, ServiceError> {
        let claims = BoardUserJwtClaims::new(user_id, company_id);
        let key_bytes = self.derive_company_key(company_id);
        let key: Hmac<Sha256> = Hmac::new_from_slice(&key_bytes)
            .map_err(|e| ServiceError::Internal(format!("HMAC key error: {}", e)))?;

        let mut claims_map = BTreeMap::new();
        claims_map.insert("user_id", claims.user_id.to_string());
        claims_map.insert("company_id", claims.company_id.to_string());
        claims_map.insert("exp", claims.exp.to_string());
        claims_map.insert("iat", claims.iat.to_string());
        claims_map.insert("iss", claims.iss.clone());

        let token = claims_map
            .sign_with_key(&key)
            .map_err(|e| ServiceError::Internal(format!("JWT signing error: {}", e)))?;

        Ok(token)
    }

    async fn verify_board_user_jwt(&self, token: &str) -> Result<BoardUserJwtClaims, ServiceError> {
        // Similar to LocalAgent verification
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(ServiceError::Unauthorized("Invalid JWT format".to_string()));
        }

        let payload = base64::decode_config(parts[1], base64::URL_SAFE_NO_PAD)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid JWT payload: {}", e)))?;

        let payload_json: serde_json::Value = serde_json::from_slice(&payload)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid JWT JSON: {}", e)))?;

        let company_id_str = payload_json["company_id"]
            .as_str()
            .ok_or_else(|| ServiceError::Unauthorized("Missing company_id in JWT".to_string()))?;

        let company_id = Uuid::parse_str(company_id_str)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid company_id: {}", e)))?;

        let key_bytes = self.derive_company_key(company_id);
        let key: Hmac<Sha256> = Hmac::new_from_slice(&key_bytes)
            .map_err(|e| ServiceError::Internal(format!("HMAC key error: {}", e)))?;

        let claims: BTreeMap<String, String> = token
            .verify_with_key(&key)
            .map_err(|e| ServiceError::Unauthorized(format!("JWT verification failed: {}", e)))?;

        let user_id = Uuid::parse_str(claims.get("user_id").ok_or_else(|| ServiceError::Unauthorized("Missing user_id".to_string()))?)
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid user_id: {}", e)))?;

        let exp = claims.get("exp")
            .ok_or_else(|| ServiceError::Unauthorized("Missing exp".to_string()))?
            .parse::<i64>()
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid exp: {}", e)))?;

        let iat = claims.get("iat")
            .ok_or_else(|| ServiceError::Unauthorized("Missing iat".to_string()))?
            .parse::<i64>()
            .map_err(|e| ServiceError::Unauthorized(format!("Invalid iat: {}", e)))?;

        let iss = claims.get("iss")
            .ok_or_else(|| ServiceError::Unauthorized("Missing iss".to_string()))?
            .clone();

        let jwt_claims = BoardUserJwtClaims {
            user_id,
            company_id,
            exp,
            iat,
            iss,
        };

        if jwt_claims.is_expired() {
            return Err(ServiceError::Unauthorized("JWT expired".to_string()));
        }

        Ok(jwt_claims)
    }

    fn derive_company_key(&self, company_id: Uuid) -> Vec<u8> {
        use sha2::Digest;

        let mut hasher = Sha256::new();
        hasher.update(&self.master_secret);
        hasher.update(company_id.as_bytes());
        hasher.update(b"parrot-agent-jwt-v1");

        hasher.finalize().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_agent_jwt_roundtrip() {
        let service = DefaultJwtService::new(b"test-secret-key".to_vec());
        let agent_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = service.sign_local_agent_jwt(agent_id, company_id).await.unwrap();
        let claims = service.verify_local_agent_jwt(&token).await.unwrap();

        assert_eq!(claims.agent_id, agent_id);
        assert_eq!(claims.company_id, company_id);
        assert!(!claims.is_expired());
    }

    #[tokio::test]
    async fn test_board_user_jwt_roundtrip() {
        let service = DefaultJwtService::new(b"test-secret-key".to_vec());
        let user_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();

        let token = service.sign_board_user_jwt(user_id, company_id).await.unwrap();
        let claims = service.verify_board_user_jwt(&token).await.unwrap();

        assert_eq!(claims.user_id, user_id);
        assert_eq!(claims.company_id, company_id);
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_company_key_derivation() {
        let service = DefaultJwtService::new(b"test-secret".to_vec());
        let company1 = Uuid::new_v4();
        let company2 = Uuid::new_v4();

        let key1 = service.derive_company_key(company1);
        let key2 = service.derive_company_key(company2);

        // Different companies should have different keys
        assert_ne!(key1, key2);

        // Same company should produce same key
        let key1_again = service.derive_company_key(company1);
        assert_eq!(key1, key1_again);
    }
}
