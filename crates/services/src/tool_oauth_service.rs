/// OAuth 服务
/// 
/// 为工具提供 OAuth 2.0 认证支持

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("token not found: {0}")]
    TokenNotFound(String),
    
    #[error("token expired: {0}")]
    TokenExpired(String),
    
    #[error("refresh failed: {0}")]
    RefreshFailed(String),
    
    #[error("provider not supported: {0}")]
    ProviderNotSupported(String),
}

pub type OAuthResult<T> = Result<T, OAuthError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub provider: String,
    pub scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthProvider {
    pub name: String,
    pub authorize_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

/// OAuth 服务
pub struct ToolOAuthService {
    tokens: Arc<RwLock<HashMap<(Uuid, String), OAuthToken>>>,
    providers: Arc<RwLock<HashMap<String, OAuthProvider>>>,
}

impl ToolOAuthService {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 注册 OAuth 提供商
    pub async fn register_provider(&self, provider: OAuthProvider) {
        self.providers.write().await.insert(provider.name.clone(), provider);
    }
    
    /// 存储 Token
    pub async fn store_token(&self, agent_id: Uuid, provider: String, token: OAuthToken) {
        self.tokens.write().await.insert((agent_id, provider), token);
    }
    
    /// 获取 Token
    pub async fn get_token(&self, agent_id: Uuid, provider: &str) -> OAuthResult<OAuthToken> {
        let tokens = self.tokens.read().await;
        
        tokens.get(&(agent_id, provider.to_string()))
            .cloned()
            .ok_or_else(|| OAuthError::TokenNotFound(format!("{}:{}", agent_id, provider)))
    }
    
    /// 检查 Token 是否过期
    pub fn is_token_expired(&self, token: &OAuthToken) -> bool {
        token.expires_at < chrono::Utc::now()
    }
    
    /// 刷新 Token
    pub async fn refresh_token(&self, agent_id: Uuid, provider: &str) -> OAuthResult<OAuthToken> {
        let mut tokens = self.tokens.write().await;
        let providers = self.providers.read().await;
        
        let old_token = tokens.get(&(agent_id, provider.to_string()))
            .ok_or_else(|| OAuthError::TokenNotFound(format!("{}:{}", agent_id, provider)))?;
        
        let _provider_config = providers.get(provider)
            .ok_or_else(|| OAuthError::ProviderNotSupported(provider.to_string()))?;
        
        // 实际刷新逻辑需要调用 OAuth 提供商的 API
        // 这里是简化实现
        let refresh_token = old_token.refresh_token.as_ref()
            .ok_or_else(|| OAuthError::RefreshFailed("No refresh token".to_string()))?;
        
        let new_token = OAuthToken {
            access_token: format!("refreshed_{}", refresh_token),
            refresh_token: old_token.refresh_token.clone(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            provider: provider.to_string(),
            scope: old_token.scope.clone(),
        };
        
        tokens.insert((agent_id, provider.to_string()), new_token.clone());
        
        Ok(new_token)
    }
    
    /// 撤销 Token
    pub async fn revoke_token(&self, agent_id: Uuid, provider: &str) -> OAuthResult<()> {
        self.tokens.write().await.remove(&(agent_id, provider.to_string()));
        Ok(())
    }
}

impl Default for ToolOAuthService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_store_and_get_token() {
        let service = ToolOAuthService::new();
        let agent_id = Uuid::new_v4();
        
        let token = OAuthToken {
            access_token: "test_token".to_string(),
            refresh_token: Some("refresh_token".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            provider: "github".to_string(),
            scope: vec!["repo".to_string()],
        };
        
        service.store_token(agent_id, "github".to_string(), token.clone()).await;
        
        let retrieved = service.get_token(agent_id, "github").await.unwrap();
        assert_eq!(retrieved.access_token, "test_token");
    }
    
    #[tokio::test]
    async fn test_token_expiry() {
        let service = ToolOAuthService::new();
        
        let expired_token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
            provider: "github".to_string(),
            scope: vec![],
        };
        
        assert!(service.is_token_expired(&expired_token));
    }
}
