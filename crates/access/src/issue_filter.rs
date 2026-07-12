use serde::{Deserialize, Serialize};
use uuid::Uuid;
use models;

use super::models::{Action, Actor};
use super::service::IssueLike;

// Implement IssueLike for models::Issue (re-exported from models crate)
impl IssueLike for models::Issue {
    fn issue_id(&self) -> Uuid { self.id }
    fn issue_company_id(&self) -> Uuid { self.company_id }
    fn issue_project_id(&self) -> Option<Uuid> { self.project_id }
    fn issue_parent_id(&self) -> Option<Uuid> { self.parent_id }
    fn issue_assignee_agent_id(&self) -> Option<Uuid> { self.assignee_agent_id }
    fn issue_assignee_user_id(&self) -> Option<Uuid> { self.assignee_user_id }
    fn issue_status(&self) -> &str {
        // Use Display implementation of IssueStatus
        // Since we can't easily access it here, we use a simple match
        match self.status {
            models::IssueStatus::Backlog => "backlog",
            models::IssueStatus::Todo => "todo",
            models::IssueStatus::InProgress => "in_progress",
            models::IssueStatus::InReview => "in_review",
            models::IssueStatus::Blocked => "blocked",
            models::IssueStatus::Done => "done",
            models::IssueStatus::Cancelled => "cancelled",
        }
    }
}

/// Source trust levels for low-trust review
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrustLevel {
    /// Untrusted / quarantined output — must be reviewed
    Low,
    /// Medium trust — partially reviewed
    Medium,
    /// High trust — fully reviewed / promoted
    High,
}

impl SourceTrustLevel {
    /// Parse from string (JSONB field value)
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" | "quarantined" | "untrusted" => SourceTrustLevel::Low,
            "medium" => SourceTrustLevel::Medium,
            "high" | "trusted" | "promoted" => SourceTrustLevel::High,
            _ => SourceTrustLevel::Low, // Default to low for unknown
        }
    }

    /// Whether this level requires redaction for non-privileged viewers
    pub fn requires_redaction(&self) -> bool {
        matches!(self, SourceTrustLevel::Low | SourceTrustLevel::Medium)
    }
}

/// Redact issue fields based on actor's permissions and source trust level.
///
/// This implements field-level access control:
/// - Low-trust outputs have their `execution_state` and sensitive metadata redacted
/// - Only the original author or a Board user can see full low-trust details
pub fn redact_issue_for_actor(
    actor: &dyn Actor,
    issue: &mut serde_json::Value,
    source_trust: Option<&str>,
) {
    let trust_level = source_trust
        .map(SourceTrustLevel::from_str)
        .unwrap_or(SourceTrustLevel::High);

    // High trust — no redaction needed
    if trust_level == SourceTrustLevel::High {
        return;
    }

    // Board users / non-agent users can see everything
    if !actor.is_agent() {
        return;
    }

    // Check if actor has special bypass permission
    let has_bypass = actor.has_permission(Action::BoardLowTrustPromote);
    if has_bypass {
        return;
    }

    // Redact sensitive fields for low-trust issues
    if let Some(obj) = issue.as_object_mut() {
        // Redact execution_state — contains detailed runtime info
        if obj.contains_key("execution_state") {
            obj.insert(
                "execution_state".to_string(),
                serde_json::json!({
                    "redacted": true,
                    "reason": "Content is low-trust and requires review"
                }),
            );
        }

        // Redact execution_policy
        if obj.contains_key("execution_policy") {
            obj.insert(
                "execution_policy".to_string(),
                serde_json::json!({ "redacted": true }),
            );
        }

        // For low trust only: redact description (may contain generated content)
        if trust_level == SourceTrustLevel::Low {
            if let Some(desc) = obj.get("description") {
                if let Some(desc_str) = desc.as_str() {
                    if desc_str.len() > 200 {
                        let truncated = format!(
                            "{}...[Content redacted — low trust output, {} more chars]",
                            &desc_str[..200],
                            desc_str.len() - 200
                        );
                        obj.insert("description".to_string(), serde_json::json!(truncated));
                    }
                }
            }
        }
    }
}

/// Filter issues by source trust level — removes issues the actor shouldn't see.
///
/// Unlike `redact_issue_for_actor` which keeps the issue but redacts fields,
/// this filter can remove entire issues from results based on trust policies.
pub fn filter_issues_by_source_trust<T>(
    actor: &dyn Actor,
    issues: Vec<T>,
    get_source_trust: impl Fn(&T) -> Option<&str>,
) -> Vec<T> {
    // Board users / non-agent users see all
    if !actor.is_agent() {
        return issues;
    }

    let has_bypass = actor.has_permission(Action::BoardLowTrustPromote);
    if has_bypass {
        return issues;
    }

    issues
        .into_iter()
        .filter(|issue| {
            let trust = get_source_trust(issue);
            match trust {
                // Low trust issues hidden from non-privileged agents
                Some(t) if SourceTrustLevel::from_str(t) == SourceTrustLevel::Low => false,
                _ => true,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentActor, UserActor};

    fn create_agent_actor(permissions: serde_json::Value) -> impl Actor {
        AgentActor {
            agent_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            permissions,
        }
    }

    fn create_user_actor() -> impl Actor {
        UserActor {
            user_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
            is_admin: false,
        }
    }

    #[test]
    fn test_source_trust_level_parsing() {
        assert_eq!(SourceTrustLevel::from_str("low"), SourceTrustLevel::Low);
        assert_eq!(SourceTrustLevel::from_str("quarantined"), SourceTrustLevel::Low);
        assert_eq!(SourceTrustLevel::from_str("medium"), SourceTrustLevel::Medium);
        assert_eq!(SourceTrustLevel::from_str("high"), SourceTrustLevel::High);
        assert_eq!(SourceTrustLevel::from_str("promoted"), SourceTrustLevel::High);
        assert_eq!(SourceTrustLevel::from_str("trusted"), SourceTrustLevel::High);
        assert_eq!(SourceTrustLevel::from_str("unknown"), SourceTrustLevel::Low);
    }

    #[test]
    fn test_redact_low_trust_for_agent() {
        let actor = create_agent_actor(serde_json::json!({}));
        let mut issue = serde_json::json!({
            "id": "123",
            "title": "Test Issue",
            "description": "A" .repeat(500),
            "execution_state": {"status": "running", "data": "sensitive"},
            "execution_policy": {"max_retries": 3},
        });

        redact_issue_for_actor(&actor, &mut issue, Some("low"));

        // execution_state should be redacted
        assert_eq!(
            issue["execution_state"]["redacted"],
            serde_json::json!(true)
        );

        // execution_policy should be redacted
        assert_eq!(
            issue["execution_policy"]["redacted"],
            serde_json::json!(true)
        );

        // description should be truncated for low trust
        let desc = issue["description"].as_str().unwrap();
        assert!(desc.contains("[Content redacted"));
    }

    #[test]
    fn test_redact_skipped_for_user() {
        let actor = create_user_actor();
        let mut issue = serde_json::json!({
            "id": "123",
            "title": "Test Issue",
            "execution_state": {"status": "running"},
        });

        redact_issue_for_actor(&actor, &mut issue, Some("low"));

        // User (non-agent) should see everything — redacted field should not exist
        assert!(issue["execution_state"]["redacted"].is_null());
    }

    #[test]
    fn test_redact_skipped_for_high_trust() {
        let actor = create_agent_actor(serde_json::json!({}));
        let mut issue = serde_json::json!({
            "id": "123",
            "execution_state": {"status": "running"},
        });

        redact_issue_for_actor(&actor, &mut issue, Some("high"));

        // High trust — no redaction
        assert!(issue["execution_state"]["redacted"].is_null());
    }

    #[test]
    fn test_filter_issues_by_source_trust() {
        let actor = create_agent_actor(serde_json::json!({}));

        let issues = vec![
            serde_json::json!({"id": "1", "source_trust": "low"}),
            serde_json::json!({"id": "2", "source_trust": "high"}),
            serde_json::json!({"id": "3", "source_trust": "medium"}),
        ];

        let filtered = filter_issues_by_source_trust(
            &actor,
            issues,
            |issue| issue.get("source_trust").and_then(|v| v.as_str()),
        );

        // Low trust issue should be filtered out
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0]["id"], "2");
        assert_eq!(filtered[1]["id"], "3");
    }

    #[test]
    fn test_bypass_with_permission() {
        let actor = create_agent_actor(serde_json::json!({
            "can_promote_low_trust": true
        }));

        let issues = vec![
            serde_json::json!({"id": "1", "source_trust": "low"}),
            serde_json::json!({"id": "2", "source_trust": "high"}),
        ];

        let filtered = filter_issues_by_source_trust(
            &actor,
            issues,
            |issue| issue.get("source_trust").and_then(|v| v.as_str()),
        );

        // Agent with bypass permission should see all
        assert_eq!(filtered.len(), 2);
    }
}
