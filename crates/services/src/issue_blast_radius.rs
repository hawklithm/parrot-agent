use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Analyzes and tracks the blast radius of an issue - what might be affected if this issue changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueBlastRadius {
    pub issue_id: Uuid,
    pub affected_agents: HashSet<Uuid>,
    pub affected_routines: HashSet<Uuid>,
    pub affected_runs: HashSet<Uuid>,
    pub affected_files: HashSet<String>,
    pub dependent_issues: HashSet<Uuid>,
    pub risk_level: RiskLevel,
    pub impact_score: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl IssueBlastRadius {
    pub fn new(issue_id: Uuid) -> Self {
        Self {
            issue_id,
            affected_agents: HashSet::new(),
            affected_routines: HashSet::new(),
            affected_runs: HashSet::new(),
            affected_files: HashSet::new(),
            dependent_issues: HashSet::new(),
            risk_level: RiskLevel::Low,
            impact_score: 0.0,
        }
    }
    
    pub fn add_affected_agent(&mut self, agent_id: Uuid) {
        self.affected_agents.insert(agent_id);
        self.recalculate_risk();
    }
    
    pub fn add_affected_routine(&mut self, routine_id: Uuid) {
        self.affected_routines.insert(routine_id);
        self.recalculate_risk();
    }
    
    pub fn add_affected_run(&mut self, run_id: Uuid) {
        self.affected_runs.insert(run_id);
        self.recalculate_risk();
    }
    
    pub fn add_affected_file(&mut self, file_path: String) {
        self.affected_files.insert(file_path);
        self.recalculate_risk();
    }
    
    pub fn add_dependent_issue(&mut self, issue_id: Uuid) {
        self.dependent_issues.insert(issue_id);
        self.recalculate_risk();
    }
    
    fn recalculate_risk(&mut self) {
        // Calculate impact score based on affected entities
        let agent_score = self.affected_agents.len() as f64 * 2.0;
        let routine_score = self.affected_routines.len() as f64 * 1.5;
        let run_score = self.affected_runs.len() as f64 * 1.0;
        let file_score = self.affected_files.len() as f64 * 0.5;
        let dependency_score = self.dependent_issues.len() as f64 * 3.0;
        
        self.impact_score = agent_score + routine_score + run_score + file_score + dependency_score;
        
        // Determine risk level based on impact score
        self.risk_level = if self.impact_score >= 20.0 {
            RiskLevel::Critical
        } else if self.impact_score >= 10.0 {
            RiskLevel::High
        } else if self.impact_score >= 5.0 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };
    }
    
    pub fn total_affected_count(&self) -> usize {
        self.affected_agents.len()
            + self.affected_routines.len()
            + self.affected_runs.len()
            + self.affected_files.len()
            + self.dependent_issues.len()
    }
    
    pub fn is_high_risk(&self) -> bool {
        matches!(self.risk_level, RiskLevel::High | RiskLevel::Critical)
    }
    
    pub fn to_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut summary = HashMap::new();
        summary.insert("issue_id".to_string(), serde_json::json!(self.issue_id));
        summary.insert("risk_level".to_string(), serde_json::json!(self.risk_level));
        summary.insert("impact_score".to_string(), serde_json::json!(self.impact_score));
        summary.insert("total_affected".to_string(), serde_json::json!(self.total_affected_count()));
        summary.insert("affected_agents".to_string(), serde_json::json!(self.affected_agents.len()));
        summary.insert("affected_routines".to_string(), serde_json::json!(self.affected_routines.len()));
        summary.insert("affected_runs".to_string(), serde_json::json!(self.affected_runs.len()));
        summary.insert("affected_files".to_string(), serde_json::json!(self.affected_files.len()));
        summary.insert("dependent_issues".to_string(), serde_json::json!(self.dependent_issues.len()));
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_blast_radius_calculation() {
        let issue_id = Uuid::new_v4();
        let mut radius = IssueBlastRadius::new(issue_id);
        
        assert_eq!(radius.risk_level, RiskLevel::Low);
        
        // Add some affected entities
        radius.add_affected_agent(Uuid::new_v4());
        radius.add_affected_agent(Uuid::new_v4());
        radius.add_affected_routine(Uuid::new_v4());
        radius.add_affected_file("src/main.rs".to_string());
        
        assert!(radius.impact_score > 0.0);
        assert!(radius.risk_level != RiskLevel::Low);
    }
    
    #[test]
    fn test_critical_risk() {
        let mut radius = IssueBlastRadius::new(Uuid::new_v4());
        
        // Add many dependencies to trigger critical risk
        for _ in 0..5 {
            radius.add_dependent_issue(Uuid::new_v4());
        }
        
        for _ in 0..5 {
            radius.add_affected_agent(Uuid::new_v4());
        }
        
        assert_eq!(radius.risk_level, RiskLevel::Critical);
        assert!(radius.is_high_risk());
    }
}
