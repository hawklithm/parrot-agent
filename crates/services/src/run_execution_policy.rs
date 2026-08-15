use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Defines policies for how runs should be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExecutionPolicy {
    pub policy_id: Uuid,
    pub name: String,
    pub max_concurrent_runs: Option<u32>,
    pub max_run_duration_seconds: Option<u64>,
    pub retry_policy: RetryPolicy,
    pub resource_limits: ResourceLimits,
    pub isolation_level: IsolationLevel,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub retry_delay_seconds: u64,
    pub exponential_backoff: bool,
    pub retry_on_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_percent: Option<u32>,
    pub max_disk_mb: Option<u64>,
    pub max_network_requests: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IsolationLevel {
    None,
    Process,
    Container,
    Vm,
}

impl RunExecutionPolicy {
    pub fn default_policy() -> Self {
        Self {
            policy_id: Uuid::new_v4(),
            name: "Default Policy".to_string(),
            max_concurrent_runs: Some(10),
            max_run_duration_seconds: Some(3600), // 1 hour
            retry_policy: RetryPolicy {
                max_retries: 3,
                retry_delay_seconds: 60,
                exponential_backoff: true,
                retry_on_errors: vec![
                    "timeout".to_string(),
                    "network_error".to_string(),
                ],
            },
            resource_limits: ResourceLimits {
                max_memory_mb: Some(1024),
                max_cpu_percent: Some(80),
                max_disk_mb: Some(5000),
                max_network_requests: Some(1000),
            },
            isolation_level: IsolationLevel::Process,
            enabled: true,
        }
    }
    
    pub fn strict_policy() -> Self {
        Self {
            policy_id: Uuid::new_v4(),
            name: "Strict Policy".to_string(),
            max_concurrent_runs: Some(5),
            max_run_duration_seconds: Some(1800), // 30 minutes
            retry_policy: RetryPolicy {
                max_retries: 1,
                retry_delay_seconds: 30,
                exponential_backoff: false,
                retry_on_errors: vec![],
            },
            resource_limits: ResourceLimits {
                max_memory_mb: Some(512),
                max_cpu_percent: Some(50),
                max_disk_mb: Some(1000),
                max_network_requests: Some(100),
            },
            isolation_level: IsolationLevel::Container,
            enabled: true,
        }
    }
    
    pub fn permissive_policy() -> Self {
        Self {
            policy_id: Uuid::new_v4(),
            name: "Permissive Policy".to_string(),
            max_concurrent_runs: Some(50),
            max_run_duration_seconds: Some(7200), // 2 hours
            retry_policy: RetryPolicy {
                max_retries: 5,
                retry_delay_seconds: 120,
                exponential_backoff: true,
                retry_on_errors: vec![
                    "timeout".to_string(),
                    "network_error".to_string(),
                    "rate_limit".to_string(),
                ],
            },
            resource_limits: ResourceLimits {
                max_memory_mb: Some(4096),
                max_cpu_percent: Some(100),
                max_disk_mb: Some(10000),
                max_network_requests: Some(10000),
            },
            isolation_level: IsolationLevel::Process,
            enabled: true,
        }
    }
    
    pub fn should_retry(&self, error: &str, attempt: u32) -> bool {
        if attempt >= self.retry_policy.max_retries {
            return false;
        }
        
        if self.retry_policy.retry_on_errors.is_empty() {
            return true;
        }
        
        self.retry_policy.retry_on_errors
            .iter()
            .any(|e| error.contains(e))
    }
    
    pub fn get_retry_delay(&self, attempt: u32) -> u64 {
        if self.retry_policy.exponential_backoff {
            self.retry_policy.retry_delay_seconds * 2u64.pow(attempt)
        } else {
            self.retry_policy.retry_delay_seconds
        }
    }
    
    pub fn is_within_resource_limits(&self, usage: &ResourceUsage) -> bool {
        let memory_ok = self.resource_limits.max_memory_mb
            .map(|limit| usage.memory_mb <= limit)
            .unwrap_or(true);
        
        let cpu_ok = self.resource_limits.max_cpu_percent
            .map(|limit| usage.cpu_percent <= limit)
            .unwrap_or(true);
        
        let disk_ok = self.resource_limits.max_disk_mb
            .map(|limit| usage.disk_mb <= limit)
            .unwrap_or(true);
        
        let network_ok = self.resource_limits.max_network_requests
            .map(|limit| usage.network_requests <= limit)
            .unwrap_or(true);
        
        memory_ok && cpu_ok && disk_ok && network_ok
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_mb: u64,
    pub cpu_percent: u32,
    pub disk_mb: u64,
    pub network_requests: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_policy() {
        let policy = RunExecutionPolicy::default_policy();
        assert!(policy.enabled);
        assert_eq!(policy.max_concurrent_runs, Some(10));
    }
    
    #[test]
    fn test_retry_logic() {
        let policy = RunExecutionPolicy::default_policy();
        
        assert!(policy.should_retry("timeout", 0));
        assert!(policy.should_retry("timeout", 2));
        assert!(!policy.should_retry("timeout", 3));
        assert!(!policy.should_retry("unknown_error", 0));
    }
    
    #[test]
    fn test_exponential_backoff() {
        let policy = RunExecutionPolicy::default_policy();
        
        assert_eq!(policy.get_retry_delay(0), 60);
        assert_eq!(policy.get_retry_delay(1), 120);
        assert_eq!(policy.get_retry_delay(2), 240);
    }
}
