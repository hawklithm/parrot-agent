use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Tracks drift in managed resources (differences between desired and actual state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedResourceDrift {
    pub resource_id: Uuid,
    pub resource_type: String,
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub drift_items: Vec<DriftItem>,
    pub severity: DriftSeverity,
    pub auto_correctable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftItem {
    pub field_path: String,
    pub desired_value: serde_json::Value,
    pub actual_value: serde_json::Value,
    pub drift_type: DriftType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftType {
    ValueMismatch,
    Missing,
    Unexpected,
    TypeMismatch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl ManagedResourceDrift {
    pub fn new(resource_id: Uuid, resource_type: String) -> Self {
        Self {
            resource_id,
            resource_type,
            detected_at: chrono::Utc::now(),
            drift_items: Vec::new(),
            severity: DriftSeverity::Info,
            auto_correctable: true,
        }
    }
    
    pub fn add_drift(
        &mut self,
        field_path: String,
        desired_value: serde_json::Value,
        actual_value: serde_json::Value,
        drift_type: DriftType,
    ) {
        let item = DriftItem {
            field_path,
            desired_value,
            actual_value,
            drift_type,
        };
        
        self.drift_items.push(item);
        self.recalculate_severity();
    }
    
    fn recalculate_severity(&mut self) {
        if self.drift_items.is_empty() {
            self.severity = DriftSeverity::Info;
            return;
        }
        
        // Calculate severity based on drift types and count
        let has_missing = self.drift_items.iter().any(|d| d.drift_type == DriftType::Missing);
        let has_type_mismatch = self.drift_items.iter().any(|d| d.drift_type == DriftType::TypeMismatch);
        let drift_count = self.drift_items.len();
        
        self.severity = if has_type_mismatch || drift_count >= 10 {
            DriftSeverity::Critical
        } else if has_missing || drift_count >= 5 {
            DriftSeverity::Error
        } else if drift_count >= 2 {
            DriftSeverity::Warning
        } else {
            DriftSeverity::Info
        };
        
        // Auto-correction not possible for type mismatches
        self.auto_correctable = !has_type_mismatch;
    }
    
    pub fn has_drift(&self) -> bool {
        !self.drift_items.is_empty()
    }
    
    pub fn is_critical(&self) -> bool {
        matches!(self.severity, DriftSeverity::Critical | DriftSeverity::Error)
    }
    
    pub fn drift_count(&self) -> usize {
        self.drift_items.len()
    }
    
    pub fn to_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut summary = HashMap::new();
        summary.insert("resource_id".to_string(), serde_json::json!(self.resource_id));
        summary.insert("resource_type".to_string(), serde_json::json!(self.resource_type));
        summary.insert("drift_count".to_string(), serde_json::json!(self.drift_count()));
        summary.insert("severity".to_string(), serde_json::json!(self.severity));
        summary.insert("auto_correctable".to_string(), serde_json::json!(self.auto_correctable));
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_drift_detection() {
        let resource_id = Uuid::new_v4();
        let mut drift = ManagedResourceDrift::new(resource_id, "config".to_string());
        
        assert!(!drift.has_drift());
        
        drift.add_drift(
            "timeout".to_string(),
            serde_json::json!(60),
            serde_json::json!(30),
            DriftType::ValueMismatch,
        );
        
        assert!(drift.has_drift());
        assert_eq!(drift.drift_count(), 1);
    }
    
    #[test]
    fn test_severity_calculation() {
        let mut drift = ManagedResourceDrift::new(Uuid::new_v4(), "agent".to_string());
        
        // Add multiple drifts
        for i in 0..5 {
            drift.add_drift(
                format!("field_{}", i),
                serde_json::json!(true),
                serde_json::json!(false),
                DriftType::ValueMismatch,
            );
        }
        
        assert!(matches!(drift.severity, DriftSeverity::Error | DriftSeverity::Warning));
    }
    
    #[test]
    fn test_type_mismatch_severity() {
        let mut drift = ManagedResourceDrift::new(Uuid::new_v4(), "config".to_string());
        
        drift.add_drift(
            "port".to_string(),
            serde_json::json!(8080),
            serde_json::json!("8080"),
            DriftType::TypeMismatch,
        );
        
        assert_eq!(drift.severity, DriftSeverity::Critical);
        assert!(!drift.auto_correctable);
    }
}
