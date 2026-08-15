/// Diagnostic Service
/// 
/// 系统诊断、健康检查和问题排查

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticError {
    #[error("diagnostic failed: {0}")]
    Failed(String),
    
    #[error("check not found: {0}")]
    NotFound(String),
}

pub type DiagnosticResult<T> = Result<T, DiagnosticError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub response_time_ms: u64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl HealthCheck {
    pub fn new(name: String, status: HealthStatus) -> Self {
        Self {
            name,
            status,
            message: None,
            checked_at: chrono::Utc::now(),
            response_time_ms: 0,
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
    
    pub fn with_response_time(mut self, ms: u64) -> Self {
        self.response_time_ms = ms;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemDiagnostics {
    pub overall_status: HealthStatus,
    pub checks: Vec<HealthCheck>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl SystemDiagnostics {
    pub fn new() -> Self {
        Self {
            overall_status: HealthStatus::Unknown,
            checks: Vec::new(),
            generated_at: chrono::Utc::now(),
        }
    }
    
    pub fn add_check(&mut self, check: HealthCheck) {
        self.checks.push(check);
        self.update_overall_status();
    }
    
    fn update_overall_status(&mut self) {
        if self.checks.is_empty() {
            self.overall_status = HealthStatus::Unknown;
            return;
        }
        
        let has_unhealthy = self.checks.iter().any(|c| c.status == HealthStatus::Unhealthy);
        let has_degraded = self.checks.iter().any(|c| c.status == HealthStatus::Degraded);
        
        self.overall_status = if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub id: Uuid,
    pub report_type: ReportType,
    pub title: String,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub recommendations: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportType {
    Performance,
    Security,
    Reliability,
    General,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: FindingSeverity,
    pub category: String,
    pub description: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingSeverity {
    Critical = 4,
    High = 3,
    Medium = 2,
    Low = 1,
}

pub struct DiagnosticService {
    check_functions: HashMap<String, Box<dyn Fn() -> HealthCheck + Send + Sync>>,
    reports: Vec<DiagnosticReport>,
}

impl DiagnosticService {
    pub fn new() -> Self {
        Self {
            check_functions: HashMap::new(),
            reports: Vec::new(),
        }
    }
    
    /// 注册健康检查
    pub fn register_check<F>(&mut self, name: String, check_fn: F)
    where
        F: Fn() -> HealthCheck + Send + Sync + 'static,
    {
        self.check_functions.insert(name, Box::new(check_fn));
    }
    
    /// 执行所有健康检查
    pub fn run_all_checks(&self) -> DiagnosticResult<SystemDiagnostics> {
        let mut diagnostics = SystemDiagnostics::new();
        
        for (name, check_fn) in &self.check_functions {
            let start = std::time::Instant::now();
            let mut check = check_fn();
            let elapsed = start.elapsed().as_millis() as u64;
            
            check.response_time_ms = elapsed;
            diagnostics.add_check(check);
        }
        
        Ok(diagnostics)
    }
    
    /// 执行单个健康检查
    pub fn run_check(&self, check_name: &str) -> DiagnosticResult<HealthCheck> {
        let check_fn = self.check_functions.get(check_name)
            .ok_or_else(|| DiagnosticError::NotFound(check_name.to_string()))?;
        
        let start = std::time::Instant::now();
        let mut check = check_fn();
        let elapsed = start.elapsed().as_millis() as u64;
        
        check.response_time_ms = elapsed;
        Ok(check)
    }
    
    /// 创建诊断报告
    pub fn create_report(
        &mut self,
        report_type: ReportType,
        title: String,
        summary: String,
        findings: Vec<Finding>,
        recommendations: Vec<String>,
    ) -> Uuid {
        let report = DiagnosticReport {
            id: Uuid::new_v4(),
            report_type,
            title,
            summary,
            findings,
            recommendations,
            created_at: chrono::Utc::now(),
        };
        
        let id = report.id;
        self.reports.push(report);
        id
    }
    
    /// 获取报告
    pub fn get_report(&self, id: Uuid) -> Option<&DiagnosticReport> {
        self.reports.iter().find(|r| r.id == id)
    }
    
    /// 列出所有报告
    pub fn list_reports(&self) -> Vec<&DiagnosticReport> {
        self.reports.iter().collect()
    }
    
    /// 分析系统资源使用
    pub fn analyze_resource_usage(&self) -> ResourceUsageAnalysis {
        // 简化实现 - 实际应该收集真实的系统指标
        ResourceUsageAnalysis {
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0,
            disk_usage_mb: 0,
            network_in_mbps: 0.0,
            network_out_mbps: 0.0,
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// 检测异常模式
    pub fn detect_anomalies(&self) -> Vec<Anomaly> {
        // 简化实现 - 实际应该基于历史数据进行异常检测
        Vec::new()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceUsageAnalysis {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub disk_usage_mb: u64,
    pub network_in_mbps: f64,
    pub network_out_mbps: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Anomaly {
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub anomaly_type: String,
    pub description: String,
    pub severity: FindingSeverity,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_health_check_creation() {
        let check = HealthCheck::new("database".to_string(), HealthStatus::Healthy)
            .with_message("Connection successful".to_string())
            .with_response_time(15);
        
        assert_eq!(check.name, "database");
        assert_eq!(check.status, HealthStatus::Healthy);
        assert_eq!(check.response_time_ms, 15);
    }
    
    #[test]
    fn test_system_diagnostics() {
        let mut diagnostics = SystemDiagnostics::new();
        
        diagnostics.add_check(HealthCheck::new("db".to_string(), HealthStatus::Healthy));
        diagnostics.add_check(HealthCheck::new("api".to_string(), HealthStatus::Healthy));
        
        assert_eq!(diagnostics.overall_status, HealthStatus::Healthy);
        assert_eq!(diagnostics.checks.len(), 2);
    }
    
    #[test]
    fn test_degraded_status() {
        let mut diagnostics = SystemDiagnostics::new();
        
        diagnostics.add_check(HealthCheck::new("db".to_string(), HealthStatus::Healthy));
        diagnostics.add_check(HealthCheck::new("cache".to_string(), HealthStatus::Degraded));
        
        assert_eq!(diagnostics.overall_status, HealthStatus::Degraded);
    }
    
    #[test]
    fn test_unhealthy_status() {
        let mut diagnostics = SystemDiagnostics::new();
        
        diagnostics.add_check(HealthCheck::new("db".to_string(), HealthStatus::Healthy));
        diagnostics.add_check(HealthCheck::new("api".to_string(), HealthStatus::Unhealthy));
        
        assert_eq!(diagnostics.overall_status, HealthStatus::Unhealthy);
    }
    
    #[test]
    fn test_register_and_run_check() {
        let mut service = DiagnosticService::new();
        
        service.register_check("test".to_string(), || {
            HealthCheck::new("test".to_string(), HealthStatus::Healthy)
        });
        
        let check = service.run_check("test").unwrap();
        assert_eq!(check.name, "test");
        assert_eq!(check.status, HealthStatus::Healthy);
    }
    
    #[test]
    fn test_diagnostic_report() {
        let mut service = DiagnosticService::new();
        
        let findings = vec![
            Finding {
                severity: FindingSeverity::High,
                category: "Performance".to_string(),
                description: "High memory usage detected".to_string(),
                details: None,
            }
        ];
        
        let id = service.create_report(
            ReportType::Performance,
            "Performance Analysis".to_string(),
            "System performance review".to_string(),
            findings,
            vec!["Increase memory allocation".to_string()],
        );
        
        let report = service.get_report(id).unwrap();
        assert_eq!(report.title, "Performance Analysis");
        assert_eq!(report.findings.len(), 1);
    }
}
