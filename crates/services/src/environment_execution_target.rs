use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents an execution target for different environment types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentExecutionTarget {
    pub target_id: Uuid,
    pub name: String,
    pub environment_type: EnvironmentType,
    pub configuration: TargetConfiguration,
    pub status: TargetStatus,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentType {
    Development,
    Staging,
    Production,
    Test,
    Preview,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfiguration {
    pub endpoint: String,
    pub credentials: Option<String>,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub health_check_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetStatus {
    Active,
    Inactive,
    Degraded,
    Maintenance,
}

impl EnvironmentExecutionTarget {
    pub fn new(name: String, environment_type: EnvironmentType, endpoint: String) -> Self {
        Self {
            target_id: Uuid::new_v4(),
            name,
            environment_type,
            configuration: TargetConfiguration {
                endpoint,
                credentials: None,
                timeout_seconds: 300,
                max_retries: 3,
                health_check_url: None,
            },
            status: TargetStatus::Active,
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn with_credentials(mut self, credentials: String) -> Self {
        self.configuration.credentials = Some(credentials);
        self
    }
    
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.configuration.timeout_seconds = timeout_seconds;
        self
    }
    
    pub fn with_health_check(mut self, health_check_url: String) -> Self {
        self.configuration.health_check_url = Some(health_check_url);
        self
    }
    
    pub fn is_available(&self) -> bool {
        matches!(self.status, TargetStatus::Active)
    }
    
    pub fn is_production(&self) -> bool {
        self.environment_type == EnvironmentType::Production
    }
    
    pub fn set_status(&mut self, status: TargetStatus) {
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_target_creation() {
        let target = EnvironmentExecutionTarget::new(
            "prod-api".to_string(),
            EnvironmentType::Production,
            "https://api.example.com".to_string(),
        );
        
        assert!(target.is_production());
        assert!(target.is_available());
    }
    
    #[test]
    fn test_target_configuration() {
        let target = EnvironmentExecutionTarget::new(
            "dev-api".to_string(),
            EnvironmentType::Development,
            "http://localhost:8080".to_string(),
        )
        .with_credentials("dev-token".to_string())
        .with_timeout(60)
        .with_health_check("http://localhost:8080/health".to_string());
        
        assert_eq!(target.configuration.timeout_seconds, 60);
        assert!(target.configuration.credentials.is_some());
        assert!(target.configuration.health_check_url.is_some());
    }
}
