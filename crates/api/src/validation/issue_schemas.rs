//! Issue and Case validation schemas
//!
//! Defines validation schemas for checkout/release/force-release operations
//! using serde validation attributes.

use serde::Deserialize;
use uuid::Uuid;
use std::collections::HashSet;

/// Valid issue status strings for checkout/release
const VALID_ISSUE_STATUSES: &[&str] = &[
    "backlog", "todo", "in_progress", "in_review", "blocked", "done", "cancelled",
];

/// Checkout issue schema with validation
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutIssueSchema {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    /// Expected current statuses (must be non-empty to prevent accidental checkout)
    pub expected_statuses: Vec<String>,
    pub checkout_run_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub execution_workspace_id: Option<Uuid>,
}

impl CheckoutIssueSchema {
    /// Validate the schema
    pub fn validate(&self) -> Result<(), String> {
        if self.expected_statuses.is_empty() {
            return Err("expected_statuses must not be empty".to_string());
        }

        for status in &self.expected_statuses {
            if !VALID_ISSUE_STATUSES.contains(&status.as_str()) {
                return Err(format!(
                    "Invalid status '{}'. Valid statuses: {:?}",
                    status, VALID_ISSUE_STATUSES
                ));
            }
        }

        // Check for duplicate statuses
        let mut seen = HashSet::new();
        for status in &self.expected_statuses {
            if !seen.insert(status) {
                return Err(format!("Duplicate status '{}' in expected_statuses", status));
            }
        }

        Ok(())
    }
}

/// Release issue schema with validation
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseIssueSchema {
    pub release_run_id: Uuid,
    pub result: Option<String>,
    pub target_status: Option<String>,
    pub release_lease: Option<bool>,
}

impl ReleaseIssueSchema {
    /// Validate the schema
    pub fn validate(&self) -> Result<(), String> {
        // Validate target_status if provided
        if let Some(ref target) = self.target_status {
            if !VALID_ISSUE_STATUSES.contains(&target.as_str()) {
                return Err(format!(
                    "Invalid target status '{}'. Valid statuses: {:?}",
                    target, VALID_ISSUE_STATUSES
                ));
            }
        }

        // Validate result if provided
        if let Some(ref result) = self.result {
            let valid_results = ["success", "failed", "cancelled", "needs_review"];
            if !valid_results.contains(&result.as_str()) {
                return Err(format!(
                    "Invalid result '{}'. Valid results: {:?}",
                    result, valid_results
                ));
            }
        }

        Ok(())
    }
}

/// Force release schema (admin operation)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceReleaseSchema {
    pub admin_user_id: Uuid,
    pub reason: String,
    pub release_lease: Option<bool>,
}

impl ForceReleaseSchema {
    /// Validate the schema
    pub fn validate(&self) -> Result<(), String> {
        if self.reason.trim().is_empty() {
            return Err("reason must not be empty for force release".to_string());
        }
        Ok(())
    }
}

/// Batch issue update schema
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIssueUpdateSchema {
    pub issue_ids: Vec<Uuid>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub assignee_agent_id: Option<Uuid>,
    pub assignee_user_id: Option<Uuid>,
}

impl BatchIssueUpdateSchema {
    /// Validate the schema
    pub fn validate(&self) -> Result<(), String> {
        if self.issue_ids.is_empty() {
            return Err("issue_ids must not be empty".to_string());
        }

        if self.issue_ids.len() > 100 {
            return Err("Cannot batch update more than 100 issues at once".to_string());
        }

        // Check for duplicate issue IDs
        let mut seen = HashSet::new();
        for id in &self.issue_ids {
            if !seen.insert(id) {
                return Err(format!("Duplicate issue ID '{}' in issue_ids", id));
            }
        }

        if let Some(ref status) = self.status {
            if !VALID_ISSUE_STATUSES.contains(&status.as_str()) {
                return Err(format!(
                    "Invalid status '{}'. Valid statuses: {:?}",
                    status, VALID_ISSUE_STATUSES
                ));
            }
        }

        if let Some(ref priority) = self.priority {
            let valid_priorities = ["critical", "high", "medium", "low"];
            if !valid_priorities.contains(&priority.as_str()) {
                return Err(format!(
                    "Invalid priority '{}'. Valid priorities: {:?}",
                    priority, valid_priorities
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_checkout_schema_valid() {
        let schema = CheckoutIssueSchema {
            agent_id: Some(Uuid::new_v4()),
            user_id: None,
            expected_statuses: vec!["todo".to_string(), "in_progress".to_string()],
            checkout_run_id: Uuid::new_v4(),
            environment_id: None,
            execution_workspace_id: None,
        };
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn test_checkout_schema_empty_statuses() {
        let schema = CheckoutIssueSchema {
            agent_id: Some(Uuid::new_v4()),
            user_id: None,
            expected_statuses: vec![],
            checkout_run_id: Uuid::new_v4(),
            environment_id: None,
            execution_workspace_id: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_checkout_schema_duplicate_statuses() {
        let schema = CheckoutIssueSchema {
            agent_id: Some(Uuid::new_v4()),
            user_id: None,
            expected_statuses: vec!["todo".to_string(), "todo".to_string()],
            checkout_run_id: Uuid::new_v4(),
            environment_id: None,
            execution_workspace_id: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_checkout_schema_invalid_status() {
        let schema = CheckoutIssueSchema {
            agent_id: Some(Uuid::new_v4()),
            user_id: None,
            expected_statuses: vec!["invalid_status".to_string()],
            checkout_run_id: Uuid::new_v4(),
            environment_id: None,
            execution_workspace_id: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_release_schema_valid() {
        let schema = ReleaseIssueSchema {
            release_run_id: Uuid::new_v4(),
            result: Some("success".to_string()),
            target_status: None,
            release_lease: Some(true),
        };
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn test_release_schema_invalid_result() {
        let schema = ReleaseIssueSchema {
            release_run_id: Uuid::new_v4(),
            result: Some("invalid_result".to_string()),
            target_status: None,
            release_lease: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_release_schema_invalid_target_status() {
        let schema = ReleaseIssueSchema {
            release_run_id: Uuid::new_v4(),
            result: None,
            target_status: Some("invalid_status".to_string()),
            release_lease: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_force_release_schema_valid() {
        let schema = ForceReleaseSchema {
            admin_user_id: Uuid::new_v4(),
            reason: "Admin override".to_string(),
            release_lease: Some(true),
        };
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn test_force_release_schema_empty_reason() {
        let schema = ForceReleaseSchema {
            admin_user_id: Uuid::new_v4(),
            reason: "   ".to_string(),
            release_lease: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_batch_update_schema_valid() {
        let schema = BatchIssueUpdateSchema {
            issue_ids: vec![Uuid::new_v4(), Uuid::new_v4()],
            status: Some("in_progress".to_string()),
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
        };
        assert!(schema.validate().is_ok());
    }

    #[test]
    fn test_batch_update_schema_empty_ids() {
        let schema = BatchIssueUpdateSchema {
            issue_ids: vec![],
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_batch_update_schema_too_many() {
        let schema = BatchIssueUpdateSchema {
            issue_ids: vec![Uuid::new_v4(); 101],
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
        };
        assert!(schema.validate().is_err());
    }

    #[test]
    fn test_batch_update_schema_duplicate_ids() {
        let id = Uuid::new_v4();
        let schema = BatchIssueUpdateSchema {
            issue_ids: vec![id, id],
            status: None,
            priority: None,
            assignee_agent_id: None,
            assignee_user_id: None,
        };
        assert!(schema.validate().is_err());
    }
}
