use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum RemoteHttpEndpointGuardError {
    #[error("endpoint blocked: {0}")]
    Blocked(String),
    #[error("invalid endpoint: {0}")]
    Invalid(String),
    #[error("rate limit exceeded")]
    RateLimitExceeded,
}

pub type GuardResult<T> = Result<T, RemoteHttpEndpointGuardError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointGuardConfig {
    pub allowed_domains: HashSet<String>,
    pub blocked_domains: HashSet<String>,
    pub require_https: bool,
    pub max_requests_per_minute: u32,
}

impl Default for EndpointGuardConfig {
    fn default() -> Self {
        Self {
            allowed_domains: HashSet::new(),
            blocked_domains: HashSet::new(),
            require_https: true,
            max_requests_per_minute: 60,
        }
    }
}

#[async_trait]
pub trait RemoteHttpEndpointGuard: Send + Sync {
    fn validate_url(&self, url: &str) -> GuardResult<()>;
    fn check_domain(&self, domain: &str) -> GuardResult<()>;
    async fn check_rate_limit(&self, endpoint: &str) -> GuardResult<()>;
    fn is_allowed(&self, url: &str) -> bool;
}

pub struct RemoteHttpEndpointGuardImpl {
    config: EndpointGuardConfig,
}

impl RemoteHttpEndpointGuardImpl {
    pub fn new(config: EndpointGuardConfig) -> Self {
        Self { config }
    }
    
    pub fn with_defaults() -> Self {
        Self {
            config: EndpointGuardConfig::default(),
        }
    }
    
    fn extract_domain(url: &str) -> Option<String> {
        url.split("://")
            .nth(1)?
            .split('/')
            .next()
            .map(|s| s.to_string())
    }
}

#[async_trait]
impl RemoteHttpEndpointGuard for RemoteHttpEndpointGuardImpl {
    fn validate_url(&self, url: &str) -> GuardResult<()> {
        // Check HTTPS requirement
        if self.config.require_https && !url.starts_with("https://") {
            return Err(RemoteHttpEndpointGuardError::Invalid(
                "HTTPS required".to_string()
            ));
        }
        
        // Extract and check domain
        let domain = Self::extract_domain(url)
            .ok_or_else(|| RemoteHttpEndpointGuardError::Invalid(
                "Invalid URL format".to_string()
            ))?;
        
        self.check_domain(&domain)?;
        
        Ok(())
    }
    
    fn check_domain(&self, domain: &str) -> GuardResult<()> {
        // Check if domain is blocked
        if self.config.blocked_domains.contains(domain) {
            return Err(RemoteHttpEndpointGuardError::Blocked(
                format!("Domain {} is blocked", domain)
            ));
        }
        
        // If allowlist is configured, check if domain is allowed
        if !self.config.allowed_domains.is_empty() 
            && !self.config.allowed_domains.contains(domain) {
            return Err(RemoteHttpEndpointGuardError::Blocked(
                format!("Domain {} is not in allowlist", domain)
            ));
        }
        
        Ok(())
    }
    
    async fn check_rate_limit(&self, _endpoint: &str) -> GuardResult<()> {
        // Simplified rate limiting - would use a real rate limiter in production
        // For now, just return Ok
        Ok(())
    }
    
    fn is_allowed(&self, url: &str) -> bool {
        self.validate_url(url).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_https_validation() {
        let guard = RemoteHttpEndpointGuardImpl::with_defaults();
        
        assert!(guard.validate_url("https://example.com").is_ok());
        assert!(guard.validate_url("http://example.com").is_err());
    }
    
    #[test]
    fn test_domain_blocking() {
        let mut config = EndpointGuardConfig::default();
        config.blocked_domains.insert("evil.com".to_string());
        
        let guard = RemoteHttpEndpointGuardImpl::new(config);
        
        assert!(guard.validate_url("https://example.com").is_ok());
        assert!(guard.validate_url("https://evil.com").is_err());
    }
    
    #[test]
    fn test_domain_allowlist() {
        let mut config = EndpointGuardConfig::default();
        config.allowed_domains.insert("trusted.com".to_string());
        
        let guard = RemoteHttpEndpointGuardImpl::new(config);
        
        assert!(guard.validate_url("https://trusted.com").is_ok());
        assert!(guard.validate_url("https://untrusted.com").is_err());
    }
}
