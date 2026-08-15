use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Smoke testing lab for quick validation of changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeLab {
    pub lab_id: Uuid,
    pub name: String,
    pub test_suites: Vec<SmokeTestSuite>,
    pub environment: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeTestSuite {
    pub suite_id: Uuid,
    pub name: String,
    pub tests: Vec<SmokeTest>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeTest {
    pub test_id: Uuid,
    pub name: String,
    pub test_type: TestType,
    pub expected_result: ExpectedResult,
    pub timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TestType {
    HealthCheck,
    ApiEndpoint,
    DatabaseConnection,
    CacheConnection,
    FileSystemAccess,
    NetworkAccess,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedResult {
    pub status: ExpectedStatus,
    pub response_time_ms: Option<u32>,
    pub custom_checks: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedStatus {
    Success,
    Failure,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeTestResult {
    pub test_id: Uuid,
    pub actual_status: ExpectedStatus,
    pub actual_response_time_ms: u32,
    pub passed: bool,
    pub error_message: Option<String>,
    pub executed_at: chrono::DateTime<chrono::Utc>,
}

impl SmokeLab {
    pub fn new(name: String, environment: String) -> Self {
        Self {
            lab_id: Uuid::new_v4(),
            name,
            test_suites: Vec::new(),
            environment,
            created_at: chrono::Utc::now(),
        }
    }
    
    pub fn add_test_suite(&mut self, suite: SmokeTestSuite) {
        self.test_suites.push(suite);
    }
    
    pub fn total_tests(&self) -> usize {
        self.test_suites
            .iter()
            .map(|s| s.tests.len())
            .sum()
    }
    
    pub fn enabled_tests(&self) -> usize {
        self.test_suites
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.tests.len())
            .sum()
    }
}

impl SmokeTestSuite {
    pub fn new(name: String) -> Self {
        Self {
            suite_id: Uuid::new_v4(),
            name,
            tests: Vec::new(),
            enabled: true,
        }
    }
    
    pub fn add_test(&mut self, test: SmokeTest) {
        self.tests.push(test);
    }
    
    pub fn quick_health_check_suite() -> Self {
        let mut suite = Self::new("Quick Health Check".to_string());
        
        suite.add_test(SmokeTest {
            test_id: Uuid::new_v4(),
            name: "API Health".to_string(),
            test_type: TestType::HealthCheck,
            expected_result: ExpectedResult {
                status: ExpectedStatus::Success,
                response_time_ms: Some(1000),
                custom_checks: HashMap::new(),
            },
            timeout_seconds: 5,
        });
        
        suite.add_test(SmokeTest {
            test_id: Uuid::new_v4(),
            name: "Database Connection".to_string(),
            test_type: TestType::DatabaseConnection,
            expected_result: ExpectedResult {
                status: ExpectedStatus::Success,
                response_time_ms: Some(500),
                custom_checks: HashMap::new(),
            },
            timeout_seconds: 10,
        });
        
        suite
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_smoke_lab_creation() {
        let mut lab = SmokeLab::new("Dev Lab".to_string(), "development".to_string());
        let suite = SmokeTestSuite::quick_health_check_suite();
        
        lab.add_test_suite(suite);
        
        assert_eq!(lab.test_suites.len(), 1);
        assert_eq!(lab.total_tests(), 2);
        assert_eq!(lab.enabled_tests(), 2);
    }
}
