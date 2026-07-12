use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::sync::Arc;

use models::{IssueComment, IssueCommentAuthorType, IssueThreadInteraction, ThreadInteractionKind, ThreadInteractionStatus, IssueStatus};
use repositories::IssueRepository;
use crate::{ServiceError, issue_comment_service::IssueCommentService};

/// Add comment input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCommentInput {
    pub body: String,
    #[serde(default)]
    pub reopen_requested: bool,
    #[serde(default)]
    pub interrupt: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Add comment actor
#[derive(Debug, Clone)]
pub struct CommentActor {
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

/// Create thread interaction input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInteractionInput {
    pub kind: ThreadInteractionKind,
    pub question: String,
    pub agent_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
}

/// Resolve interaction input
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveInteractionInput {
    pub status: ThreadInteractionStatus,
    pub response: Option<String>,
    pub resolver_agent_id: Option<Uuid>,
    pub resolver_user_id: Option<Uuid>,
}

/// Default Issue Comment Service Implementation
pub struct DefaultIssueCommentService {
    issue_repo: Arc<dyn IssueRepository>,
}

impl DefaultIssueCommentService {
    pub fn new(issue_repo: Arc<dyn IssueRepository>) -> Self {
        Self { issue_repo }
    }

    /// Verify actor is allowed to comment on issue
    async fn verify_comment_permission(
        &self,
        issue_id: Uuid,
        company_id: Uuid,
        actor: &CommentActor,
    ) -> Result<(), ServiceError> {
        // Get issue to verify company isolation
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.company_id != company_id {
            return Err(ServiceError::Forbidden(
                "Cannot comment on issue from different company".to_string()
            ));
        }

        // Basic permission check: agent or user must be specified
        if actor.agent_id.is_none() && actor.user_id.is_none() {
            return Err(ServiceError::Forbidden(
                "Either agent_id or user_id must be provided".to_string()
            ));
        }

        Ok(())
    }

    /// Determine author type from actor
    fn determine_author_type(actor: &CommentActor) -> IssueCommentAuthorType {
        if actor.agent_id.is_some() {
            IssueCommentAuthorType::Agent
        } else if actor.user_id.is_some() {
            IssueCommentAuthorType::User
        } else {
            IssueCommentAuthorType::System
        }
    }

    /// Handle reopen requested logic
    async fn handle_reopen_if_requested(
        &self,
        issue_id: Uuid,
        reopen_requested: bool,
    ) -> Result<(), ServiceError> {
        if !reopen_requested {
            return Ok(());
        }

        // Get issue and check if it's done or cancelled
        let issue = self.issue_repo
            .get_by_id(issue_id)
            .await
            .map_err(|e| ServiceError::Internal(format!("Failed to get issue: {}", e)))?
            .ok_or_else(|| ServiceError::NotFound(format!("Issue {} not found", issue_id)))?;

        if issue.status == IssueStatus::Done || issue.status == IssueStatus::Cancelled {
            // Reopen to todo
            let update_input = models::UpdateIssueInput {
                title: None,
                description: None,
                status: Some(IssueStatus::Todo),
                priority: None,
                assignee_agent_id: None,
                assignee_user_id: None,
                work_mode: None,
                responsible_user_id: None,
                source_trust: None,
                monitor_scheduled_by: None,
                monitor_notes: None,
                hidden_at: None,
                execution_workspace_preference: None,
                execution_workspace_settings: None,
                execution_policy: None,
                execution_state: None,
            };

            self.issue_repo
                .update(issue_id, update_input)
                .await
                .map_err(|e| ServiceError::Internal(format!("Failed to reopen issue: {}", e)))?;
        }

        Ok(())
    }
}

